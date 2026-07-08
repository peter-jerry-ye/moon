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

use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::LazyLock,
};

pub use moonutil::dry_run::PathNormalizer;

// Historical test hook name. The payload is now produced from typed dry-run
// command records, not by reparsing an n2 graph.
const ENV_VAR: &str = "MOON_TEST_DUMP_BUILD_GRAPH";
static DRY_RUN_TEST_OUTPUT: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var(ENV_VAR).ok());

pub struct DryRunCommand<'a> {
    pub command: Option<&'a str>,
    pub inputs: &'a [PathBuf],
    pub outputs: &'a [PathBuf],
}

pub fn try_debug_dump_commands_to_file<'a>(
    commands: impl IntoIterator<Item = DryRunCommand<'a>>,
    source_dir: &Path,
    target_dir: &Path,
) {
    let Some(out_file) = DRY_RUN_TEST_OUTPUT.as_deref() else {
        return;
    };

    let replacer = PathNormalizer::new_with_target_dir(source_dir, target_dir);
    let mut nodes = commands
        .into_iter()
        .map(|command| {
            let mut inputs = command
                .inputs
                .iter()
                .map(|path| replacer.normalize_path(&path.to_string_lossy()))
                .collect::<Vec<_>>();
            inputs.sort();
            let outputs = command
                .outputs
                .iter()
                .map(|path| replacer.normalize_path(&path.to_string_lossy()))
                .collect::<Vec<_>>();
            DryRunNode {
                command: command
                    .command
                    .map(|command| replacer.normalize_command(command)),
                inputs,
                outputs,
            }
        })
        .collect::<Vec<_>>();

    nodes.sort_by(|a, b| a.outputs.cmp(&b.outputs));
    let file = std::fs::File::create(out_file).expect("Failed to create dry-run dump target");
    let mut writer = std::io::BufWriter::new(file);
    for node in &nodes {
        serde_json::to_writer(&mut writer, node).expect("Failed to dump to target output");
        writeln!(&mut writer).expect("Failed to dump to target output");
    }
}

#[derive(Debug, serde::Serialize)]
struct DryRunNode {
    command: Option<String>,
    inputs: Vec<String>,
    outputs: Vec<String>,
}
