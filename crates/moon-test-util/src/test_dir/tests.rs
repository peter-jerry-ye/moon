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

use super::*;

fn records(root: &Path) -> Vec<serde_json::Value> {
    let files: Vec<_> = std::fs::read_dir(root).unwrap().collect();
    assert_eq!(files.len(), 1);
    std::fs::read_to_string(files[0].as_ref().unwrap().path())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn subset_preserves_sibling_layout_and_isolates_writes() {
    let source = tempfile::tempdir().unwrap();
    for name in ["app", "dependency", "unrelated"] {
        std::fs::create_dir(source.path().join(name)).unwrap();
        std::fs::write(source.path().join(name).join("input"), name).unwrap();
    }
    let workspace;
    {
        let dir = TestDir::prepare(
            source.path(),
            &["app".into(), "dependency/input".into()],
            true,
            Options::default(),
        );
        workspace = dir.as_ref().to_owned();
        assert_eq!(
            std::fs::read_to_string(dir.join("app/input")).unwrap(),
            "app"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("dependency/input")).unwrap(),
            "dependency"
        );
        assert!(!dir.join("unrelated").exists());
        std::fs::write(dir.join("app/input"), "changed").unwrap();
        assert_eq!(
            std::fs::read_to_string(source.path().join("app/input")).unwrap(),
            "app"
        );
        std::fs::create_dir(dir.join("_build")).unwrap();
        std::fs::write(dir.join("_build/artifact"), "generated").unwrap();
    }
    assert!(
        !workspace.exists(),
        "default cleanup includes generated artifacts"
    );
}

#[test]
fn trace_records_phase_boundaries_and_copied_data() {
    let source = tempfile::tempdir().unwrap();
    let traces = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("input"), "abc").unwrap();
    let dir = TestDir::prepare(
        source.path(),
        &["input".into()],
        true,
        Options {
            trace_dir: Some(traces.path().to_owned()),
            keep_dir: None,
        },
    );
    let workspace = dir.as_ref().to_owned();
    // Completed preparation is available while the test is still running.
    let active = records(traces.path());
    assert_eq!(active.last().unwrap()["event"]["phase"], "active");
    assert_eq!(active.last().unwrap()["event"]["state"], "started");
    drop(dir);
    let data = records(traces.path());
    let phases: Vec<_> = data
        .iter()
        .map(|row| {
            (
                row["event"]["phase"].as_str().unwrap(),
                row["event"]["state"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        phases,
        [
            ("create", "started"),
            ("create", "ok"),
            ("copy", "started"),
            ("copy", "ok"),
            ("active", "started"),
            ("active", "finished"),
            ("cleanup", "started"),
            ("cleanup", "ok"),
        ]
    );
    let copied = &data[3]["event"];
    assert_eq!(copied["copied"]["files"], 1);
    assert_eq!(copied["copied"]["bytes"], 3);
    for row in data.iter().filter(|row| row["event"]["state"] != "started") {
        assert!(row["event"]["elapsed_us"].is_u64());
    }
    assert!(!workspace.exists());
}

#[test]
fn retention_keeps_generated_files_and_reports_no_cleanup() {
    let kept = tempfile::tempdir().unwrap();
    let traces = tempfile::tempdir().unwrap();
    let dir = TestDir::prepare(
        Path::new(""),
        &[],
        false,
        Options {
            trace_dir: Some(traces.path().to_owned()),
            keep_dir: Some(kept.path().to_owned()),
        },
    );
    let workspace = dir.as_ref().to_owned();
    std::fs::write(dir.join("result"), "inspect me").unwrap();
    drop(dir);
    assert_eq!(
        std::fs::read_to_string(workspace.join("result")).unwrap(),
        "inspect me"
    );
    assert_eq!(
        records(traces.path()).last().unwrap()["event"]["state"],
        "retained"
    );
}

#[test]
fn failed_setup_keeps_partial_copy_when_requested() {
    let source = tempfile::tempdir().unwrap();
    let kept = tempfile::tempdir().unwrap();
    let traces = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("input"), "partial").unwrap();
    let result = std::panic::catch_unwind(|| {
        TestDir::prepare(
            source.path(),
            &["input".into(), "missing".into()],
            true,
            Options {
                trace_dir: Some(traces.path().to_owned()),
                keep_dir: Some(kept.path().to_owned()),
            },
        )
    });
    assert!(result.is_err());
    let data = records(traces.path());
    let copy = data
        .iter()
        .find(|row| row["event"]["phase"] == "copy" && row["event"]["state"] == "error")
        .unwrap();
    assert_eq!(copy["event"]["copied"]["files"], 1);
    let workspace = Path::new(copy["workspace"].as_str().unwrap());
    assert_eq!(
        std::fs::read_to_string(workspace.join("input")).unwrap(),
        "partial"
    );
    assert!(!data.iter().any(|row| row["event"]["phase"] == "active"));
    assert_eq!(data.last().unwrap()["event"]["state"], "retained");
}

#[test]
fn unwinding_still_cleans_up_by_default() {
    let traces = tempfile::tempdir().unwrap();
    let result = std::panic::catch_unwind(|| {
        let _dir = TestDir::prepare(
            Path::new(""),
            &[],
            false,
            Options {
                trace_dir: Some(traces.path().to_owned()),
                keep_dir: None,
            },
        );
        panic!("simulated test failure");
    });
    assert!(result.is_err());
    let data = records(traces.path());
    let active = data
        .iter()
        .find(|row| row["event"]["phase"] == "active" && row["event"]["state"] == "finished")
        .unwrap();
    assert_eq!(active["event"]["unwinding"], true);
    assert!(!Path::new(active["workspace"].as_str().unwrap()).exists());
    assert_eq!(data.last().unwrap()["event"]["state"], "ok");
}

#[test]
fn unavailable_trace_destination_does_not_break_a_test() {
    let root = tempfile::tempdir().unwrap();
    let blocker = root.path().join("file");
    std::fs::write(&blocker, "not a directory").unwrap();
    let dir = TestDir::prepare(
        Path::new(""),
        &[],
        false,
        Options {
            trace_dir: Some(blocker),
            keep_dir: None,
        },
    );
    let workspace = dir.as_ref().to_owned();
    std::fs::write(dir.join("input"), "ok").unwrap();
    drop(dir);
    assert!(!workspace.exists());
}

#[test]
#[should_panic(expected = "fixture paths must stay within the case root")]
fn subset_rejects_parent_traversal() {
    TestDir::from_case_root_subset("unused", ".", ["../outside"], true);
}
