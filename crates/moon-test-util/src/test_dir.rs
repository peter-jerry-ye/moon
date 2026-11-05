// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::{
    fs::File,
    io::Write,
    path::{Component, Path, PathBuf},
    time::Instant,
};

#[derive(Default, serde::Serialize)]
struct CopyStats {
    files: u64,
    bytes: u64,
}

pub fn copy_tree(src: &Path, dest: &Path, exclude_target: bool) -> anyhow::Result<()> {
    copy_tree_counted(src, dest, exclude_target, &mut CopyStats::default())
}

fn copy_tree_counted(
    src: &Path,
    dest: &Path,
    exclude_target: bool,
    stats: &mut CopyStats,
) -> anyhow::Result<()> {
    if src.is_dir() {
        if !dest.exists() {
            std::fs::create_dir_all(dest)?;
        }
        let mut walker = ignore::WalkBuilder::new(src);
        walker.hidden(false);
        walker.git_global(false);
        if exclude_target {
            walker.filter_entry(|x| x.file_name() != "target");
        }
        for entry in walker.build() {
            let entry = entry?;
            let path = entry.path();
            let relative_path = path.strip_prefix(src)?;
            let dest_path = dest.join(relative_path);
            if path.is_dir() {
                if !dest_path.exists() {
                    std::fs::create_dir_all(dest_path)?;
                }
            } else {
                stats.bytes += std::fs::copy(path, dest_path)?;
                stats.files += 1;
            }
        }
    } else {
        stats.bytes += std::fs::copy(src, dest)?;
        stats.files += 1;
    }
    Ok(())
}

pub struct TestDir {
    // Taken on drop so cleanup can be timed, or explicitly disabled for inspection.
    path: Option<tempfile::TempDir>,
    trace: Option<Trace>,
    retain: bool,
    ready_at: Option<Instant>,
}

impl TestDir {
    pub fn from_case_root(
        case_root: impl AsRef<Path>,
        sub: impl AsRef<Path>,
        exclude_target: bool,
    ) -> Self {
        Self::from_case_root_subset(case_root, sub, ["."], exclude_target)
    }

    /// Copy only the selected case-relative paths, preserving their layout.
    /// Include sibling dependencies explicitly when a case uses relative imports.
    pub fn from_case_root_subset(
        case_root: impl AsRef<Path>,
        sub: impl AsRef<Path>,
        paths: impl IntoIterator<Item = impl AsRef<Path>>,
        exclude_target: bool,
    ) -> Self {
        let paths: Vec<_> = paths.into_iter().map(|p| p.as_ref().to_owned()).collect();
        assert!(
            paths.iter().all(|p| p
                .components()
                .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))),
            "fixture paths must stay within the case root"
        );
        Self::prepare(
            &case_root.as_ref().join(sub),
            &paths,
            exclude_target,
            Options::from_env(),
        )
    }

    pub fn new_empty() -> Self {
        Self::prepare(Path::new(""), &[], false, Options::from_env())
    }

    fn prepare(source: &Path, paths: &[PathBuf], exclude_target: bool, options: Options) -> Self {
        let mut trace = options
            .trace_dir
            .as_deref()
            .and_then(|root| Trace::open(root, source));
        if let Some(trace) = &mut trace {
            trace.record(serde_json::json!({"phase": "create", "state": "started"}));
        }
        let started = Instant::now();
        let tmp_dir = match &options.keep_dir {
            Some(root) => std::fs::create_dir_all(root).and_then(|()| {
                let root = dunce::canonicalize(root)?;
                tempfile::Builder::new()
                    .prefix("moon-test-")
                    .tempdir_in(root)
            }),
            None => tempfile::TempDir::new(),
        };
        let elapsed = started.elapsed();
        if let Some(trace) = &mut trace {
            trace.workspace = tmp_dir.as_ref().ok().map(|dir| dir.path().to_owned());
            trace.record(serde_json::json!({
                "phase": "create", "state": if tmp_dir.is_ok() { "ok" } else { "error" },
                "elapsed_us": elapsed.as_micros(),
                "error": tmp_dir.as_ref().err().map(ToString::to_string),
            }));
        }
        // Construct the owner before copying so a setup panic also cleans up or
        // retains the partial workspace according to the requested policy.
        let mut dir = Self {
            path: Some(tmp_dir.expect("create temp dir for tests")),
            trace,
            retain: options.keep_dir.is_some(),
            ready_at: None,
        };
        if let Some(trace) = &mut dir.trace {
            let paths: Vec<_> = paths.iter().map(|path| path.to_string_lossy()).collect();
            trace.record(serde_json::json!({
                "phase": "copy", "state": "started", "paths": paths,
            }));
        }
        let started = Instant::now();
        let mut stats = CopyStats::default();
        let copied: anyhow::Result<()> = paths.iter().try_for_each(|path| {
            let dest = dir.join(path);
            // A selected file may have parents not otherwise selected.
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            copy_tree_counted(&source.join(path), &dest, exclude_target, &mut stats)
        });
        let elapsed = started.elapsed();
        if let Some(trace) = &mut dir.trace {
            trace.record(serde_json::json!({
                "phase": "copy", "state": if copied.is_ok() { "ok" } else { "error" },
                "elapsed_us": elapsed.as_micros(), "copied": stats,
                "error": copied.as_ref().err().map(ToString::to_string),
            }));
        }
        copied.expect("copy test case to temp dir");
        if let Some(trace) = &mut dir.trace {
            trace.record(serde_json::json!({"phase": "active", "state": "started"}));
        }
        dir.ready_at = Some(Instant::now());
        dir
    }

    pub fn join(&self, sub: impl AsRef<Path>) -> PathBuf {
        self.as_ref().join(sub.as_ref())
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        self.path.as_ref().expect("live test directory").path()
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let Some(dir) = self.path.take() else {
            return;
        };
        if let Some(trace) = &mut self.trace {
            if let Some(started) = self.ready_at {
                // This is workspace lifetime, including commands AND assertions;
                // it is not a measurement of compiler or child-process CPU time.
                trace.record(serde_json::json!({
                    "phase": "active", "state": "finished",
                    "elapsed_us": started.elapsed().as_micros(),
                    "unwinding": std::thread::panicking(),
                }));
            }
            trace.record(serde_json::json!({"phase": "cleanup", "state": "started"}));
        }
        let started = Instant::now();
        let cleaned = if self.retain {
            let _retained_path = dir.into_path();
            Ok(())
        } else {
            dir.close()
        };
        let elapsed = started.elapsed();
        if let Some(trace) = &mut self.trace {
            trace.record(serde_json::json!({
                "phase": "cleanup",
                "state": if self.retain { "retained" } else if cleaned.is_ok() { "ok" } else { "error" },
                "elapsed_us": elapsed.as_micros(),
                "error": cleaned.as_ref().err().map(ToString::to_string),
            }));
        }
        // Like TempDir's destructor, cleanup must not turn a test failure into a
        // second panic. The trace exposes errors that Drop previously discarded.
        if let Err(error) = cleaned {
            tracing::warn!(%error, "failed to clean up test directory");
        }
    }
}

#[derive(Default)]
struct Options {
    trace_dir: Option<PathBuf>,
    keep_dir: Option<PathBuf>,
}

impl Options {
    fn from_env() -> Self {
        Self {
            trace_dir: std::env::var_os("MOON_TEST_TRACE_DIR")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            keep_dir: std::env::var_os("MOON_TEST_KEEP_DIR")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
        }
    }
}

struct Trace {
    file: File,
    test: String,
    fixture: PathBuf,
    workspace: Option<PathBuf>,
}

impl Trace {
    fn open(root: &Path, fixture: &Path) -> Option<Self> {
        let opened = (|| -> anyhow::Result<File> {
            std::fs::create_dir_all(root)?;
            let (file, _) = tempfile::Builder::new()
                .prefix("test-dir-")
                .suffix(".jsonl")
                .tempfile_in(root)?
                .keep()?;
            Ok(file)
        })();
        match opened {
            Ok(file) => Some(Self {
                file,
                test: std::thread::current()
                    .name()
                    .unwrap_or("<unnamed>")
                    .to_owned(),
                fixture: fixture.to_owned(),
                workspace: None,
            }),
            Err(error) => {
                tracing::warn!(%error, "failed to create TestDir trace");
                None
            }
        }
    }

    fn record(&mut self, event: serde_json::Value) {
        let record = serde_json::json!({
            "pid": std::process::id(), "test": self.test,
            "fixture": self.fixture.to_string_lossy(),
            "workspace": self.workspace.as_deref().map(Path::to_string_lossy), "event": event,
        });
        // One write per event, outside the measured phase. Avoid a shared log
        // lock and retain completed events if a later phase hangs or aborts.
        let written = serde_json::to_vec(&record)
            .map_err(std::io::Error::other)
            .and_then(|mut line| {
                line.push(b'\n');
                self.file.write_all(&line)
            });
        if let Err(error) = written {
            tracing::warn!(%error, "failed to write TestDir trace");
        }
    }
}

#[cfg(test)]
mod tests;
