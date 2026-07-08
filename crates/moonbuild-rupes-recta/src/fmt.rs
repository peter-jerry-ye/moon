// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! The formatter's pipeline
//!
//! The formatter only needs a bare minimum project to run, so its pipeline
//! bypasses thew regular compilation pipeline of resolving and discovering
//! modules and packages.
//!
//! This pipeline still strives to use as much of the existing infrastructure
//! as possible.
//!
//! # Maintainers
//!
//! If a similar no-resolving, files-only command is needed, refactor this
//! module into a more generic one, probably named "source utility" or similar.

use log::*;
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    path::{Path, PathBuf},
};

use anyhow::Context;
use moonutil::dirs::ProjectManifest;
use moonutil::dry_run::dry_run_path_order_key;
use moonutil::mooncakes::{ModuleSourceKind, result::ResolvedModule};
use moonutil::toolchain::BINARIES;
use moonutil::{
    cond_expr::OptLevel,
    constants::{MOON_MOD, MOON_MOD_JSON, MOON_PKG, MOON_PKG_JSON, MOON_WORK},
    manifest::validate_module_dsl_deps,
};
use n2::graph::Build;

use crate::{
    build_lower::{build_ins, build_n2_fileloc, build_outs},
    discover::{DiscoveredLocalProject, DiscoveredPackage, discover_local_project},
    model::PackageId,
    pkg_name::{PackageFQN, PackagePath},
    resolve::ResolveError,
    target_layout::TargetLayout,
    user_warning::UserWarning,
};

pub type FmtResolveOutput = DiscoveredLocalProject;

/// Perform a barebones, faked resolving process for `moon fmt`.
///
/// This supports either a single module rooted at `source_dir` or a workspace
/// rooted there via `moon.work`.
pub fn resolve_for_fmt(
    source_dir: &Path,
    project_manifest: &ProjectManifest,
) -> Result<FmtResolveOutput, ResolveError> {
    info!(
        "Resolving formatter environment for {}",
        source_dir.display()
    );
    discover_local_project(source_dir, project_manifest).map_err(ResolveError::from)
}

pub struct FmtConfig {
    /// Checks the formatting without writing to files
    pub check_only: bool,

    /// Extra arguments to pass to the formatter
    pub extra_args: Vec<String>,

    /// Warn instead of showing differences
    pub warn_only: bool,

    /// Migrate moon.mod.json to moon.mod when only the JSON file exists.
    pub migrate_moon_mod_json: bool,

    /// Migrate moon.pkg.json to moon.pkg when only the JSON file exists.
    pub migrate_moon_pkg_json: bool,
}

pub struct FmtBuildOutput {
    pub graph: n2::graph::Graph,
    pub dry_run: Option<FmtDryRun>,
    pub user_warnings: Vec<UserWarning>,
}

/// Formatter dry-run command capture.
#[derive(Debug, Clone, Default)]
pub struct FmtDryRun {
    steps: Vec<FmtStep>,
}

/// A command emitted by [`FmtDryRun`] for dry-run/debug output.
#[derive(Debug, Clone)]
pub struct FmtCommand {
    commandline: String,
    inputs: Vec<PathBuf>,
    outputs: Vec<PathBuf>,
}

impl FmtCommand {
    pub fn commandline(&self) -> &str {
        &self.commandline
    }

    pub fn inputs(&self) -> &[PathBuf] {
        &self.inputs
    }

    pub fn outputs(&self) -> &[PathBuf] {
        &self.outputs
    }
}

#[derive(Debug, Clone)]
enum FmtStep {
    RunMoonfmt {
        source: PathBuf,
        output: PathBuf,
        write_back: bool,
        extra_args: Vec<String>,
    },
    RunFormatAndDiff {
        source: PathBuf,
        output: PathBuf,
        warn: bool,
        extra_args: Vec<String>,
    },
    RunWorkspaceFormatter {
        source: PathBuf,
        output: PathBuf,
        mode: WorkspaceFormatMode,
    },
    CopyMigratedManifest {
        from: PathBuf,
        to: PathBuf,
    },
    RemoveDeprecatedManifest {
        path: PathBuf,
        after: PathBuf,
    },
}

#[derive(Debug, Clone, Copy)]
enum WorkspaceFormatMode {
    Write,
    Check,
    Warn,
}

struct FmtGraphBuilder {
    graph: n2::graph::Graph,
    dry_run: Option<FmtDryRun>,
}

trait FmtBuildSink {
    fn run_moonfmt(
        &mut self,
        source: &Path,
        output: &Path,
        write_back: bool,
        extra_args: &[String],
    ) -> anyhow::Result<()>;

    fn run_format_and_diff(
        &mut self,
        source: &Path,
        output: &Path,
        warn: bool,
        extra_args: &[String],
    ) -> anyhow::Result<()>;

    fn run_workspace_formatter(
        &mut self,
        source: &Path,
        output: &Path,
        mode: WorkspaceFormatMode,
    ) -> anyhow::Result<()>;

    fn copy_migrated_manifest(&mut self, from: &Path, to: &Path) -> anyhow::Result<()>;

    fn remove_deprecated_manifest(&mut self, path: &Path, after: &Path) -> anyhow::Result<()>;
}

impl FmtDryRun {
    pub fn dry_run_commands(&self) -> Vec<FmtCommand> {
        self.ordered_step_indices()
            .into_iter()
            .map(|idx| self.steps[idx].command())
            .collect()
    }

    fn ordered_step_indices(&self) -> Vec<usize> {
        let mut step_by_output = HashMap::new();
        let mut input_paths = HashSet::new();
        for (idx, step) in self.steps.iter().enumerate() {
            for input in step.scheduler_inputs() {
                input_paths.insert(input);
            }
            for output in step.scheduler_outputs() {
                step_by_output.insert(output, idx);
            }
        }

        let mut start_outputs = step_by_output
            .keys()
            .filter(|output| !input_paths.contains(*output))
            .cloned()
            .collect::<Vec<_>>();
        start_outputs.sort_by_key(|path| dry_run_path_order_key(path));

        let mut stack = start_outputs
            .into_iter()
            .map(|path| (path, false))
            .collect::<Vec<_>>();
        let mut visited = HashSet::new();
        let mut ordered = Vec::new();
        while let Some((path, pop)) = stack.pop() {
            let Some(&idx) = step_by_output.get(&path) else {
                continue;
            };
            if pop {
                ordered.push(idx);
            } else if visited.insert(idx) {
                stack.push((path, true));
                let mut inputs = self.steps[idx].scheduler_inputs();
                inputs.sort_by_key(|path| dry_run_path_order_key(path));
                stack.extend(inputs.into_iter().map(|path| (path, false)));
            }
        }
        ordered
    }
}

impl FmtGraphBuilder {
    fn new(capture_dry_run: bool) -> Self {
        Self {
            graph: n2::graph::Graph::default(),
            dry_run: capture_dry_run.then(FmtDryRun::default),
        }
    }

    fn finish(self) -> (n2::graph::Graph, Option<FmtDryRun>) {
        (self.graph, self.dry_run)
    }

    fn emit(&mut self, step: FmtStep) -> anyhow::Result<()> {
        let command = step.command();
        let scheduler_outputs = step.scheduler_outputs();
        let ins = build_ins(&mut self.graph, command.inputs());
        let outs = build_outs(&mut self.graph, &scheduler_outputs);
        let mut build = Build::new(step.fileloc(), ins, outs);
        build.cmdline = Some(command.commandline().to_owned());
        build.can_dirty_on_output = step.can_dirty_on_output();
        self.graph.add_build(build)?;

        if let Some(dry_run) = &mut self.dry_run {
            dry_run.steps.push(step);
        }
        Ok(())
    }
}

impl FmtBuildSink for FmtGraphBuilder {
    fn run_moonfmt(
        &mut self,
        source: &Path,
        output: &Path,
        write_back: bool,
        extra_args: &[String],
    ) -> anyhow::Result<()> {
        self.emit(FmtStep::RunMoonfmt {
            source: source.to_path_buf(),
            output: output.to_path_buf(),
            write_back,
            extra_args: extra_args.to_vec(),
        })
    }

    fn run_format_and_diff(
        &mut self,
        source: &Path,
        output: &Path,
        warn: bool,
        extra_args: &[String],
    ) -> anyhow::Result<()> {
        self.emit(FmtStep::RunFormatAndDiff {
            source: source.to_path_buf(),
            output: output.to_path_buf(),
            warn,
            extra_args: extra_args.to_vec(),
        })
    }

    fn run_workspace_formatter(
        &mut self,
        source: &Path,
        output: &Path,
        mode: WorkspaceFormatMode,
    ) -> anyhow::Result<()> {
        self.emit(FmtStep::RunWorkspaceFormatter {
            source: source.to_path_buf(),
            output: output.to_path_buf(),
            mode,
        })
    }

    fn copy_migrated_manifest(&mut self, from: &Path, to: &Path) -> anyhow::Result<()> {
        self.emit(FmtStep::CopyMigratedManifest {
            from: from.to_path_buf(),
            to: to.to_path_buf(),
        })
    }

    fn remove_deprecated_manifest(&mut self, path: &Path, after: &Path) -> anyhow::Result<()> {
        self.emit(FmtStep::RemoveDeprecatedManifest {
            path: path.to_path_buf(),
            after: after.to_path_buf(),
        })
    }
}

impl FmtStep {
    fn command(&self) -> FmtCommand {
        let args = self.command_args();
        FmtCommand {
            commandline: moonutil::shlex::join_native(args.iter().map(|arg| arg.as_str())),
            inputs: self.scheduler_inputs(),
            outputs: self.dry_run_outputs(),
        }
    }

    fn command_args(&self) -> Vec<String> {
        match self {
            FmtStep::RunMoonfmt {
                source,
                output,
                write_back,
                extra_args,
            } => {
                let mut cmd = vec![BINARIES.moonfmt.to_string_lossy().into_owned()];
                cmd.push(source.to_string_lossy().into_owned());
                if *write_back {
                    cmd.push("-w".into());
                }
                cmd.push("-o".into());
                cmd.push(output.to_string_lossy().into_owned());
                cmd.extend_from_slice(extra_args);
                cmd
            }
            FmtStep::RunFormatAndDiff {
                source,
                output,
                warn,
                extra_args,
            } => {
                let mut cmd = vec![
                    BINARIES.moonbuild.to_string_lossy().into_owned(),
                    "tool".into(),
                    "format-and-diff".into(),
                    "--old".into(),
                    source.to_string_lossy().into_owned(),
                    "--new".into(),
                    output.to_string_lossy().into_owned(),
                ];
                if *warn {
                    cmd.push("--warn".into());
                }
                cmd.extend_from_slice(extra_args);
                cmd
            }
            FmtStep::RunWorkspaceFormatter {
                source,
                output,
                mode,
            } => {
                let mut cmd = vec![
                    BINARIES.moonbuild.to_string_lossy().into_owned(),
                    "tool".into(),
                    "format-workspace".into(),
                    "--old".into(),
                    source.to_string_lossy().into_owned(),
                ];
                match mode {
                    WorkspaceFormatMode::Write => {
                        cmd.push("--write".into());
                        cmd.push("--new".into());
                        cmd.push(output.to_string_lossy().into_owned());
                    }
                    WorkspaceFormatMode::Check => {
                        cmd.push("--new".into());
                        cmd.push(output.to_string_lossy().into_owned());
                        cmd.push("--check".into());
                    }
                    WorkspaceFormatMode::Warn => {
                        cmd.push("--new".into());
                        cmd.push(output.to_string_lossy().into_owned());
                        cmd.push("--warn".into());
                    }
                }
                cmd
            }
            FmtStep::CopyMigratedManifest { from, to } => {
                if cfg!(windows) {
                    vec![
                        "cmd".into(),
                        "/c".into(),
                        "copy".into(),
                        from.to_string_lossy().into_owned(),
                        to.to_string_lossy().into_owned(),
                    ]
                } else {
                    vec![
                        "cp".into(),
                        from.to_string_lossy().into_owned(),
                        to.to_string_lossy().into_owned(),
                    ]
                }
            }
            FmtStep::RemoveDeprecatedManifest { path, .. } => {
                if cfg!(windows) {
                    vec![
                        "cmd".into(),
                        "/c".into(),
                        "del".into(),
                        path.to_string_lossy().into_owned(),
                    ]
                } else {
                    vec!["rm".into(), path.to_string_lossy().into_owned()]
                }
            }
        }
    }

    fn fileloc(&self) -> n2::graph::FileLoc {
        build_n2_fileloc(match self {
            FmtStep::RunMoonfmt { source, .. } => format!("format {}", source.display()),
            FmtStep::RunFormatAndDiff { source, .. } => {
                format!("check format {}", source.display())
            }
            FmtStep::RunWorkspaceFormatter { source, .. } => {
                format!("format workspace {}", source.display())
            }
            FmtStep::CopyMigratedManifest { to, .. } => {
                format!("copy migrated manifest {}", to.display())
            }
            FmtStep::RemoveDeprecatedManifest { path, .. } => {
                format!("remove deprecated manifest {}", path.display())
            }
        })
    }

    fn scheduler_inputs(&self) -> Vec<PathBuf> {
        match self {
            FmtStep::RunMoonfmt { source, .. }
            | FmtStep::RunFormatAndDiff { source, .. }
            | FmtStep::RunWorkspaceFormatter { source, .. } => vec![source.clone()],
            FmtStep::CopyMigratedManifest { from, .. } => vec![from.clone()],
            FmtStep::RemoveDeprecatedManifest { after, .. } => vec![after.clone()],
        }
    }

    fn dry_run_outputs(&self) -> Vec<PathBuf> {
        match self {
            FmtStep::RunMoonfmt { output, .. }
            | FmtStep::RunFormatAndDiff { output, .. }
            | FmtStep::RunWorkspaceFormatter { output, .. } => vec![output.clone()],
            FmtStep::CopyMigratedManifest { to, .. } => vec![to.clone()],
            FmtStep::RemoveDeprecatedManifest { .. } => Vec::new(),
        }
    }

    fn scheduler_outputs(&self) -> Vec<PathBuf> {
        match self {
            FmtStep::RemoveDeprecatedManifest { path, .. } => {
                vec![PathBuf::from(format!("{}.removed", path.to_string_lossy()))]
            }
            _ => self.dry_run_outputs(),
        }
    }

    fn can_dirty_on_output(&self) -> bool {
        matches!(
            self,
            FmtStep::RunFormatAndDiff { warn: true, .. }
                | FmtStep::RunWorkspaceFormatter {
                    mode: WorkspaceFormatMode::Warn,
                    ..
                }
        )
    }
}

/// Generate the formatter build graph.
///
/// If `selected_packages` is non-empty, only the specified packages will be formatted.
/// Otherwise, all packages in the current module or workspace will be formatted.
pub fn build_graph_for_fmt(
    resolved: &FmtResolveOutput,
    cfg: &FmtConfig,
    target_dir: &Path,
    selected_packages: &[PackageId],
    project_manifest: &ProjectManifest,
    capture_dry_run: bool,
) -> anyhow::Result<FmtBuildOutput> {
    info!(
        "Building format graph for {} root modules",
        resolved.root_module_ids.len()
    );

    let layout =
        TargetLayout::from_fmt_resolve_output(target_dir.into(), resolved, OptLevel::Release);

    debug!("Layout built for formatting");

    let mut builder = FmtGraphBuilder::new(capture_dry_run);
    let mut user_warnings = Vec::new();
    let mut package_count = 0;
    let selected_packages = (!selected_packages.is_empty())
        .then(|| selected_packages.iter().copied().collect::<HashSet<_>>());
    let has_workspace_manifest = selected_packages.is_none()
        && format_workspace_node(&mut builder, cfg, &layout, project_manifest)?;
    let mut has_module_manifest = false;

    // If no path filter is provided, find and format `moon.mod`/`moon.mod.json`.
    if selected_packages.is_none() {
        for &module_id in &resolved.root_module_ids {
            let module = &resolved.root_modules[module_id];
            match module.source().source() {
                ModuleSourceKind::Local(path) | ModuleSourceKind::Stdlib(path) => {
                    has_module_manifest |= format_moon_mod_node(
                        &mut builder,
                        cfg,
                        &layout,
                        module,
                        path,
                        &mut user_warnings,
                    )?
                }
                ModuleSourceKind::Registry
                | ModuleSourceKind::Git(_)
                | ModuleSourceKind::SingleFile(_) => (),
            };
        }
    }

    for &module_id in &resolved.root_module_ids {
        let Some(packages) = resolved.pkg_dirs.packages_for_module(module_id) else {
            continue;
        };

        for &id in packages.values() {
            if let Some(selected_packages) = &selected_packages
                && !selected_packages.contains(&id)
            {
                continue;
            }

            let pkg = resolved.pkg_dirs.get_package(id);
            info!("Processing package {}", pkg.fqn);
            build_for_package(&mut builder, cfg, &layout, pkg, &mut user_warnings)?;
            package_count += 1;
        }
    }

    if package_count == 0 && !has_workspace_manifest && !has_module_manifest {
        anyhow::bail!("No packages found in workspace to format");
    }

    let (graph, dry_run) = builder.finish();
    Ok(FmtBuildOutput {
        graph,
        dry_run,
        user_warnings,
    })
}

fn format_moon_mod_node(
    sink: &mut impl FmtBuildSink,
    cfg: &FmtConfig,
    layout: &TargetLayout,
    module: &ResolvedModule,
    module_dir: &Path,
    user_warnings: &mut Vec<UserWarning>,
) -> anyhow::Result<bool> {
    let moon_mod = module_dir.join(MOON_MOD);
    let moon_mod_json = module_dir.join(MOON_MOD_JSON);

    let has_dsl = moon_mod.exists();
    let has_json = moon_mod_json.exists();
    if !has_dsl && !has_json {
        return Ok(false);
    }

    let target_moon_mod = layout.format_artifact_path(
        &PackageFQN::new(module.source().clone(), PackagePath::empty()),
        OsStr::new(MOON_MOD),
    );

    if has_dsl {
        format_moon_mod_dsl(sink, cfg, &moon_mod, &target_moon_mod)?;
    } else if cfg.migrate_moon_mod_json {
        user_warnings.push(UserWarning::new(format!(
            "Migrating to {} at module root '{}', deprecated {} is removed.",
            MOON_MOD,
            module_dir.display(),
            MOON_MOD_JSON
        )));
        format_moon_mod_json_migrate(
            sink,
            cfg,
            &moon_mod_json,
            &target_moon_mod,
            &moon_mod,
            module.module_info(),
        )?;
    }

    Ok(true)
}

fn format_moon_mod_dsl(
    sink: &mut impl FmtBuildSink,
    cfg: &FmtConfig,
    moon_mod: &Path,
    target_moon_mod: &Path,
) -> anyhow::Result<()> {
    if cfg.check_only || cfg.warn_only {
        sink.run_format_and_diff(moon_mod, target_moon_mod, cfg.warn_only, &[])?;
    } else {
        sink.run_moonfmt(moon_mod, target_moon_mod, true, &[])?;
    }

    Ok(())
}

fn format_moon_mod_json_migrate(
    sink: &mut impl FmtBuildSink,
    cfg: &FmtConfig,
    moon_mod_json: &Path,
    target_moon_mod: &Path,
    moon_mod: &Path,
    module_info: &moonutil::module::MoonMod,
) -> anyhow::Result<()> {
    // moon.mod `import` cannot represent local dependencies; those must live in moon.work.
    validate_module_dsl_deps(Some(&module_info.deps))?;

    if cfg.check_only || cfg.warn_only {
        sink.run_format_and_diff(moon_mod_json, target_moon_mod, cfg.warn_only, &[])?;
    } else {
        sink.run_moonfmt(moon_mod_json, target_moon_mod, false, &[])?;
        sink.copy_migrated_manifest(target_moon_mod, moon_mod)?;
        sink.remove_deprecated_manifest(moon_mod_json, moon_mod)?;
    }

    Ok(())
}

fn format_workspace_node(
    sink: &mut impl FmtBuildSink,
    cfg: &FmtConfig,
    layout: &TargetLayout,
    project_manifest: &ProjectManifest,
) -> anyhow::Result<bool> {
    let ProjectManifest::Workspace(workspace_manifest_path) = project_manifest else {
        return Ok(false);
    };
    workspace_manifest_path
        .parent()
        .context("workspace manifest path has no parent directory")?;

    let target_moon_work = layout.format_root_artifact_path(std::ffi::OsStr::new(MOON_WORK));
    format_moon_work_dsl(sink, cfg, workspace_manifest_path, &target_moon_work)?;
    Ok(true)
}

fn build_for_package(
    sink: &mut impl FmtBuildSink,
    cfg: &FmtConfig,
    layout: &TargetLayout,
    pkg: &DiscoveredPackage,
    user_warnings: &mut Vec<UserWarning>,
) -> anyhow::Result<()> {
    let ignore_set = &pkg.raw.formatter.ignore;
    let prebuild_outputs = pkg
        .raw
        .pre_build
        .as_ref()
        .iter()
        .flat_map(|prebuild_plans| {
            prebuild_plans
                .iter()
                .flat_map(|plan| plan.output().iter().map(|path| path.as_str()))
        })
        .collect::<HashSet<_>>();

    let mut add_fmt_for_file = |file: &Path| -> anyhow::Result<()> {
        let name = file.file_name().and_then(|name| name.to_str());
        if name.is_some_and(|name| ignore_set.contains(name)) {
            debug!(
                "Skipping formatter input {} due to formatter.ignore",
                file.display()
            );
            return Ok(());
        }
        if name.is_some_and(|name| prebuild_outputs.contains(name)) {
            debug!(
                "Skipping formatter input {} due to pre-build output",
                file.display()
            );
            return Ok(());
        }

        format_node(sink, cfg, layout, pkg, file)?;
        Ok(())
    };

    for file in &pkg.source_files {
        add_fmt_for_file(file)?;
    }
    for file in &pkg.mbt_md_files {
        add_fmt_for_file(file)?;
    }

    // Always format moon.pkg when present; migration from moon.pkg.json is gated.
    format_moon_pkg_node(sink, cfg, layout, pkg, user_warnings)?;

    Ok(())
}

fn format_node(
    sink: &mut impl FmtBuildSink,
    cfg: &FmtConfig,
    layout: &TargetLayout,
    pkg: &DiscoveredPackage,
    file: &Path,
) -> anyhow::Result<()> {
    let out_file = layout
        .format_artifact_path(&pkg.fqn, file.file_name().expect("Should have filename"))
        .to_path_buf();
    if cfg.check_only || cfg.warn_only {
        sink.run_format_and_diff(file, &out_file, cfg.warn_only, &cfg.extra_args)?;
    } else {
        sink.run_moonfmt(file, &out_file, true, &cfg.extra_args)?;
    }
    Ok(())
}

fn format_moon_work_dsl(
    sink: &mut impl FmtBuildSink,
    cfg: &FmtConfig,
    moon_work: &std::path::Path,
    target_moon_work: &std::path::Path,
) -> anyhow::Result<()> {
    if cfg.check_only || cfg.warn_only {
        sink.run_workspace_formatter(
            moon_work,
            target_moon_work,
            if cfg.warn_only {
                WorkspaceFormatMode::Warn
            } else {
                WorkspaceFormatMode::Check
            },
        )?;
    } else {
        sink.run_workspace_formatter(moon_work, target_moon_work, WorkspaceFormatMode::Write)?;
    }

    Ok(())
}

/// Format moon.pkg package configuration files and optionally migrate moon.pkg.json.
///
/// This function handles three scenarios:
/// 1. Both `moon.pkg` and `moon.pkg.json` exist: prefer `moon.pkg`, report error about duplicate
/// 2. Only `moon.pkg.json` exists: migrate to `moon.pkg` format if enabled
/// 3. Only `moon.pkg` exists: format it in place
fn format_moon_pkg_node(
    sink: &mut impl FmtBuildSink,
    cfg: &FmtConfig,
    layout: &TargetLayout,
    pkg: &DiscoveredPackage,
    user_warnings: &mut Vec<UserWarning>,
) -> anyhow::Result<()> {
    use moonutil::constants::{MOON_PKG, MOON_PKG_JSON};

    let moon_pkg_dsl = pkg.root_path.join(MOON_PKG);
    let moon_pkg_json = pkg.root_path.join(MOON_PKG_JSON);

    let has_dsl = moon_pkg_dsl.exists();
    let has_json = moon_pkg_json.exists();

    if !has_dsl && !has_json {
        debug!(
            "Skipping moon.pkg formatting for {} - no config file exists",
            pkg.fqn
        );
        return Ok(());
    }

    // Output to target directory
    let target_moon_pkg = layout.format_artifact_path(&pkg.fqn, std::ffi::OsStr::new("moon.pkg"));

    if has_dsl {
        // Format moon.pkg (new format)
        format_moon_pkg_dsl(sink, cfg, &moon_pkg_dsl, &target_moon_pkg)
    } else if cfg.migrate_moon_pkg_json {
        // Only moon.pkg.json exists: migrate to moon.pkg
        format_moon_pkg_json_migrate(
            sink,
            cfg,
            &moon_pkg_json,
            &target_moon_pkg,
            &moon_pkg_dsl,
            pkg,
            user_warnings,
        )
    } else {
        debug!(
            "Skipping moon.pkg.json migration for {} - feature disabled",
            pkg.fqn
        );
        Ok(())
    }
}

/// Format an existing moon.pkg (DSL format) file.
///
/// - moon_pkg: Path to the source moon.pkg file
/// - target_moon_pkg: Path to the output formatted moon.pkg file
fn format_moon_pkg_dsl(
    sink: &mut impl FmtBuildSink,
    cfg: &FmtConfig,
    moon_pkg: &std::path::Path,
    target_moon_pkg: &std::path::Path,
) -> anyhow::Result<()> {
    if cfg.check_only || cfg.warn_only {
        sink.run_format_and_diff(moon_pkg, target_moon_pkg, cfg.warn_only, &[])?;
    } else {
        sink.run_moonfmt(moon_pkg, target_moon_pkg, true, &[])?;
    }

    Ok(())
}

/// Migrate moon.pkg.json to moon.pkg (DSL format).
///
/// This function generates moon.pkg from moon.pkg.json and warns the user
/// to manually remove the deprecated moon.pkg.json file.
///
/// - moon_pkg_json: Path to the source moon.pkg.json file
/// - target_moon_pkg: Path to the output formatted moon.pkg file in the target directory
/// - moon_pkg: Path to the destination moon.pkg file in the source directory
fn format_moon_pkg_json_migrate(
    sink: &mut impl FmtBuildSink,
    cfg: &FmtConfig,
    moon_pkg_json: &std::path::Path,
    target_moon_pkg: &std::path::Path,
    moon_pkg: &std::path::Path,
    pkg: &DiscoveredPackage,
    user_warnings: &mut Vec<UserWarning>,
) -> anyhow::Result<()> {
    // Warn the user about migration and prompt to remove the old config
    user_warnings.push(UserWarning::new(format!(
        "Migrating to {} in package '{}', deprecated {} is removed.",
        MOON_PKG, pkg.fqn, MOON_PKG_JSON
    )));

    if cfg.check_only || cfg.warn_only {
        sink.run_format_and_diff(moon_pkg_json, target_moon_pkg, cfg.warn_only, &[])?;
    } else {
        sink.run_moonfmt(moon_pkg_json, target_moon_pkg, false, &[])?;
        sink.copy_migrated_manifest(target_moon_pkg, moon_pkg)?;
        sink.remove_deprecated_manifest(moon_pkg_json, moon_pkg)?;
    }

    Ok(())
}
