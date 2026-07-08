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

//! Handles dry-run printing of build commands.

use std::{path::Path, process::Command};

use moonbuild::dry_run::{DryRunCommand, DryRunCommandKind, PathNormalizer};
use moonbuild_rupes_recta::build_lower::LoweredCommandKind;

use crate::rr_build::{BuildInput, DryRunPlan};

/// Print what would be executed in a dry-run.
///
/// RR builds use lowered action steps directly.
pub fn print_dry_run(input: &BuildInput, source_dir: &Path, target_dir: &Path) {
    match &input.dry_run_plan {
        Some(DryRunPlan::Build(lowered_build)) => {
            let commands = lowered_build.commands();
            print_commands(
                commands
                    .iter()
                    .map(|command| DryRunCommand {
                        command: Some(command.commandline()),
                        command_kind: dry_run_command_kind(command.command_kind()),
                        inputs: command.inputs(),
                        outputs: command.outputs(),
                    })
                    .collect(),
                source_dir,
                target_dir,
            );
        }
        Some(DryRunPlan::Fmt(_)) => {
            panic!("fmt dry-run plan cannot be printed as a build dry-run");
        }
        None => {
            panic!("build dry-run plan should be available");
        }
    }
}

/// Print all commands in a dry-run.
pub fn print_dry_run_all(input: &BuildInput, source_dir: &Path, target_dir: &Path) {
    match &input.dry_run_plan {
        Some(DryRunPlan::Build(lowered_build)) => {
            let commands = lowered_build.commands();
            print_commands(
                commands
                    .iter()
                    .map(|command| DryRunCommand {
                        command: Some(command.commandline()),
                        command_kind: dry_run_command_kind(command.command_kind()),
                        inputs: command.inputs(),
                        outputs: command.outputs(),
                    })
                    .collect(),
                source_dir,
                target_dir,
            );
        }
        Some(DryRunPlan::Fmt(fmt_plan)) => {
            let commands = fmt_plan.dry_run_commands();
            print_commands(
                commands
                    .iter()
                    .map(|command| DryRunCommand {
                        command: Some(command.commandline()),
                        command_kind: DryRunCommandKind::Argv,
                        inputs: command.inputs(),
                        outputs: command.outputs(),
                    })
                    .collect(),
                source_dir,
                target_dir,
            );
        }
        None => {
            panic!("dry-run plan should be available");
        }
    }
}

fn print_commands<'a>(commands: Vec<DryRunCommand<'a>>, source_dir: &Path, target_dir: &Path) {
    let replacer = PathNormalizer::new_with_target_dir(source_dir, target_dir);
    for command in &commands {
        if let Some(commandline) = command.normalized_command(&replacer) {
            println!("{}", commandline);
        }
    }

    moonbuild::dry_run::try_debug_dump_commands_to_file(commands, source_dir, target_dir);
}

fn dry_run_command_kind(kind: LoweredCommandKind) -> DryRunCommandKind {
    match kind {
        LoweredCommandKind::Argv => DryRunCommandKind::Argv,
        LoweredCommandKind::Shell => DryRunCommandKind::Shell,
    }
}

/// Print a command as it would be executed, with the proper escaping.
///
/// This also replaces paths like `print_dry_run` does.
///
/// If `stderr` is true, the command is assumed to write to stderr instead of stdout.
pub fn dry_print_command(cmd: &Command, source_dir: &Path, stderr: bool) {
    dry_print_command_with_target_dir(cmd, source_dir, None, stderr);
}

/// Print a command as it would be executed, normalizing both source and target roots.
pub fn dry_print_command_with_target_dir(
    cmd: &Command,
    source_dir: &Path,
    target_dir: Option<&Path>,
    stderr: bool,
) {
    let replacer = match target_dir {
        Some(target_dir) => PathNormalizer::new_with_target_dir(source_dir, target_dir),
        None => PathNormalizer::new(source_dir),
    };

    let args = std::iter::once(cmd.get_program())
        .chain(cmd.get_args())
        .map(|x| x.to_string_lossy())
        .map(|x| replacer.normalize_command_arg(&x))
        .collect::<Vec<_>>();

    let cmd = moonutil::shlex::join_unix(args.iter().map(|x| x.as_ref()));
    if stderr {
        eprintln!("{}", cmd);
    } else {
        println!("{}", cmd);
    }
}
