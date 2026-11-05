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

//! Measure fixture preparation independently of MoonBit installation/execution.
//! Use MOON_TEST_TRACE_DIR to collect events. This probe has no time threshold.

use moon_test_util::test_dir::TestDir;
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../moon/tests/test_cases");
    let cases: &[(&str, &str, &[&str])] = &[
        ("small", "test_filter/test_filter", &["."]),
        ("whole-suite", "moon_test", &["."]),
        (
            "subset",
            "moon_test",
            &["async_test_inline", "dummy_async_impl"],
        ),
    ];
    let jobs: Vec<_> = (0..5)
        .flat_map(|iteration| cases.iter().map(move |case| (iteration, case)))
        .collect();
    for workers in [1, 4] {
        for batch in jobs.chunks(workers) {
            std::thread::scope(|scope| {
                for &(iteration, &(label, case, paths)) in batch {
                    let root = &root;
                    std::thread::Builder::new()
                        .name(format!("profile::{workers}-workers::{label}::{iteration}"))
                        .spawn_scoped(scope, move || {
                            drop(TestDir::from_case_root_subset(root, case, paths, true));
                        })
                        .expect("start fixture profiling worker");
                }
            });
        }
    }
    println!(
        "Prepared and cleaned 30 workspaces with 1 and 4 workers; inspect MOON_TEST_TRACE_DIR for phase timings."
    );
}
