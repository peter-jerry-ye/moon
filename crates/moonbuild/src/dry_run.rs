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

use n2::densemap::Index;
use n2::graph::{BuildId, FileId, Graph};
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    path::Path,
    sync::LazyLock,
};

pub use moonutil::dry_run::PathNormalizer;

const ENV_VAR: &str = "MOON_TEST_DUMP_BUILD_GRAPH";
static DRY_RUN_TEST_OUTPUT: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var(ENV_VAR).ok());

/// Print build commands from a State
pub fn print_build_commands(
    graph: &Graph,
    default: &[FileId],
    source_dir: &Path,
    target_dir: &Path,
) {
    let replacer = PathNormalizer::new_with_target_dir(source_dir, target_dir);

    if !default.is_empty() {
        let mut sorted_default = default.to_vec();
        sorted_default.sort_by_key(|a| a.index());
        let builds: Vec<BuildId> = stable_toposort_graph(graph, &sorted_default);
        for b in builds.iter() {
            let build = &graph.builds[*b];
            if let Some(cmdline) = &build.cmdline {
                let res = replacer.normalize_command(cmdline);
                println!("{}", res);
            }
        }
    }

    try_debug_dump_build_graph_to_file(graph, default, source_dir, target_dir);
}

#[derive(Debug)]
struct BuildGraphDump {
    nodes: Vec<BuildNode>,
}

impl BuildGraphDump {
    fn dump_to(&self, out: impl Write) -> anyhow::Result<()> {
        let mut writer = std::io::BufWriter::new(out);
        for node in &self.nodes {
            serde_json::to_writer(&mut writer, node)?;
            writeln!(&mut writer)?;
        }
        Ok(())
    }
}

#[derive(Debug, serde::Serialize)]
struct BuildNode {
    command: Option<String>,
    inputs: Vec<String>,
    outputs: Vec<String>,
}

fn debug_dump_build_graph(
    graph: &n2::graph::Graph,
    input_files: &[FileId],
    source_dir: &Path,
    target_dir: &Path,
) -> BuildGraphDump {
    let replacer = PathNormalizer::new_with_target_dir(source_dir, target_dir);

    let accessible_nodes = dfs_for_accessible_nodes(graph, input_files);
    generate_from_nodes(graph, accessible_nodes, &replacer)
}

// FIXME: `MOON_TEST_DUMP_BUILD_GRAPH` is integration-test infrastructure kept
// in production-facing dry-run code only so existing snapshot tests can keep
// invoking the compiled `moon` binary. Gate or relocate this once the test
// harness no longer needs the runtime hook.
fn try_debug_dump_build_graph_to_file(
    build_graph: &n2::graph::Graph,
    default_files: &[n2::graph::FileId],
    source_dir: &Path,
    target_dir: &Path,
) {
    let Some(out_file) = DRY_RUN_TEST_OUTPUT.as_deref() else {
        return;
    };

    let file = std::fs::File::create(out_file).expect("Failed to create dry-run dump target");
    let dump = debug_dump_build_graph(build_graph, default_files, source_dir, target_dir);
    dump.dump_to(file).expect("Failed to dump to target output");
}

fn dfs_for_accessible_nodes(graph: &n2::graph::Graph, start_files: &[FileId]) -> Vec<BuildId> {
    let mut stack = Vec::<FileId>::new();
    stack.extend_from_slice(start_files);
    let mut visited_builds = HashSet::new();
    let mut accessible_builds = vec![];

    while let Some(fid) = stack.pop() {
        let file = graph
            .files
            .by_id
            .lookup(fid)
            .expect("Unknown file in graph");
        if let Some(bid) = file.input
            && visited_builds.insert(bid)
        {
            let build = graph.builds.lookup(bid).expect("Unknown build in graph");
            accessible_builds.push(bid);
            // FIXME: This preserves the current graph dump behavior, but raw
            // `ins.ids` collapses explicit, implicit, order-only, and
            // validation/lazy inputs. Follow up by using the n2 accessor that
            // matches the intended dry-run graph snapshot semantics.
            for &in_fid in &build.ins.ids {
                stack.push(in_fid);
            }
        }
    }

    accessible_builds
}

fn generate_from_nodes(
    graph: &n2::graph::Graph,
    accessible_nodes: impl IntoIterator<Item = BuildId>,
    replacer: &PathNormalizer,
) -> BuildGraphDump {
    let mut nodes = vec![];
    for node in accessible_nodes {
        let node = graph.builds.lookup(node).expect("Unknown build in graph");
        let command = node
            .cmdline
            .as_ref()
            .map(|cmd| replacer.normalize_command(cmd));
        let mut inputs = node
            .ins
            .ids
            .iter()
            .map(|&id| {
                let file = graph.files.by_id.lookup(id).expect("Unknown node in graph");
                replacer.normalize_path(&file.name)
            })
            .collect::<Vec<_>>();
        inputs.sort();
        let outputs = node
            .outs
            .ids
            .iter()
            .map(|&id| {
                let file = graph.files.by_id.lookup(id).expect("Unknown node in graph");
                replacer.normalize_path(&file.name)
            })
            .collect::<Vec<_>>();
        nodes.push(BuildNode {
            command,
            inputs,
            outputs,
        });
    }

    nodes.sort_by(|a, b| a.outputs.cmp(&b.outputs));

    BuildGraphDump { nodes }
}

/// Create a filename-based sorting key cache for stable graph traversal.
///
/// The key prioritizes filename over full path to provide deterministic
/// ordering for dry-run output. This handles test sandbox path variations
/// while maintaining stable output across different environments.
///
/// Note: This is specifically for stable dry-run output in tests and CI.
/// Absolute stability across all possible edge cases is not a goal.
fn create_file_sorting_cache(graph: &Graph) -> HashMap<FileId, (String, usize)> {
    let mut key_cache = HashMap::with_capacity(graph.files.all_ids().size_hint().0);
    for id in graph.files.all_ids() {
        let name = &graph.file(id).name;
        let normalized = name.replace('\\', "/");
        let last_slash = normalized.rfind('/').map_or(0, |i| i + 1);
        key_cache.insert(id, (normalized, last_slash));
    }
    key_cache
}

/// Perform an iteration over the build graph to get the total list of build
/// commands that corresponds to the given inputs.
///
/// This function provides stable output order based on file names and
/// the build graph structure, independent of graph insertion order.
fn stable_toposort_graph(graph: &Graph, inputs: &[FileId]) -> Vec<BuildId> {
    let key_cache = create_file_sorting_cache(graph);
    let by_file_name = |k: &FileId| {
        let (name, last_slash) = &key_cache[k];
        (&name[*last_slash..], name)
    };

    // Sort input files by filename for deterministic order
    let mut input_order = Vec::new();
    input_order.extend_from_slice(inputs);
    input_order.sort_unstable_by_key(by_file_name);

    // DFS stack: (file_id, is_pop)
    let mut stack = Vec::<(FileId, bool)>::new();
    stack.extend(input_order.into_iter().map(|x| (x, false)));
    // Result
    let mut res = vec![];
    // Visited builds set
    let mut vis = HashSet::new();
    // Scratch vec for sorting input. Leave empty when unused.
    let mut sort_in_scratch = vec![];

    while let Some((fid, pop)) = stack.pop() {
        let file = graph.file(fid);
        if let Some(bid) = file.input {
            if !pop {
                if vis.insert(bid) {
                    let build = &graph.builds[bid];
                    stack.push((fid, true));

                    // Sort input files for stable traversal order
                    debug_assert!(sort_in_scratch.is_empty());
                    sort_in_scratch.extend_from_slice(build.explicit_ins());
                    sort_in_scratch.sort_unstable_by_key(by_file_name);
                    stack.extend(sort_in_scratch.iter().copied().map(|x| (x, false)));
                    sort_in_scratch.clear();
                }
            } else {
                res.push(bid);
            }
        }
    }

    res
}
