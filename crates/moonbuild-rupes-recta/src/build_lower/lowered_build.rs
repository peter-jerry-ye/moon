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

use std::path::PathBuf;

/// Commands captured while lowering the selected build plan.
///
/// `n2` remains the executor representation. This type is the semantic build
/// surface for callers that need command information without reparsing the
/// executor graph.
#[derive(Debug, Clone, Default)]
pub struct LoweredBuild {
    commands: Vec<LoweredBuildCommand>,
}

#[derive(Debug, Clone)]
struct LoweredBuildCommand {
    commandline: String,
    inputs: Vec<PathBuf>,
    outputs: Vec<PathBuf>,
}

/// A command produced by action-plan lowering.
#[derive(Debug, Clone, Copy)]
pub struct LoweredCommand<'a> {
    commandline: &'a str,
    inputs: &'a [PathBuf],
    outputs: &'a [PathBuf],
}

impl<'a> LoweredCommand<'a> {
    pub fn commandline(&self) -> &'a str {
        self.commandline
    }

    pub fn inputs(&self) -> &'a [PathBuf] {
        self.inputs
    }

    pub fn outputs(&self) -> &'a [PathBuf] {
        self.outputs
    }
}

impl LoweredBuildCommand {
    fn as_command(&self) -> LoweredCommand<'_> {
        LoweredCommand {
            commandline: &self.commandline,
            inputs: &self.inputs,
            outputs: &self.outputs,
        }
    }
}

impl LoweredBuild {
    pub(crate) fn push_command(
        &mut self,
        commandline: String,
        inputs: Vec<PathBuf>,
        outputs: Vec<PathBuf>,
    ) {
        self.commands.push(LoweredBuildCommand {
            commandline,
            inputs,
            outputs,
        });
    }

    pub fn commands(&self) -> Vec<LoweredCommand<'_>> {
        self.commands
            .iter()
            .map(LoweredBuildCommand::as_command)
            .collect()
    }
}
