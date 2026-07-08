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

use std::path::{Path, PathBuf};

use crate::moon_dir::{home, toolchain_root};

const TARGET_DIR_PLACEHOLDER: &str = "$MOON_TARGET_DIR";

pub fn dry_run_path_order_key(path: &Path) -> (String, String) {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let last_slash = normalized.rfind('/').map_or(0, |idx| idx + 1);
    (normalized[last_slash..].to_owned(), normalized)
}

pub struct PathNormalizer {
    source_root: Option<PathBuf>,
    target_root: Option<PathBuf>,
    replace_table: Vec<(String, String)>,
    binary_file_name_table: Vec<(String, String)>,
    show_toolchain_root: bool,
    toolchain_root: String,
    moon_home: String,
}

impl PathNormalizer {
    pub fn new(source_dir: &Path) -> Self {
        Self::new_inner(source_dir, None)
    }

    pub fn new_with_target_dir(source_dir: &Path, target_dir: &Path) -> Self {
        Self::new_inner(source_dir, Some(target_dir))
    }

    fn new_inner(source_dir: &Path, target_dir: Option<&Path>) -> Self {
        let all_moon_bins = crate::BINARIES.all_moon_bins();
        let replace_table = all_moon_bins
            .iter()
            .map(|(name, path)| (path.to_string_lossy().into_owned(), name.to_string()))
            .collect();
        let binary_file_name_table = all_moon_bins
            .iter()
            .filter_map(|(name, path)| {
                let file_name = path.file_name()?.to_str()?;
                (file_name != *name).then(|| (file_name.to_owned(), (*name).to_owned()))
            })
            .collect();
        let toolchain_root = toolchain_root();
        let moon_home = home();
        let show_toolchain_root = match (
            dunce::canonicalize(&toolchain_root),
            dunce::canonicalize(&moon_home),
        ) {
            (Ok(toolchain_root), Ok(moon_home)) => toolchain_root != moon_home,
            _ => toolchain_root != moon_home,
        };

        let source_root = dunce::canonicalize(source_dir).ok();
        let target_root = target_dir.and_then(|target_dir| dunce::canonicalize(target_dir).ok());
        PathNormalizer {
            source_root,
            target_root,
            replace_table,
            binary_file_name_table,
            show_toolchain_root,
            toolchain_root: toolchain_root.to_string_lossy().into_owned(),
            moon_home: moon_home.to_string_lossy().into_owned(),
        }
    }

    pub fn normalize_command(&self, command: &str) -> String {
        let args = crate::shlex::split_native(command);
        let normalized_args = args
            .iter()
            .map(|s| self.normalize_command_arg(s))
            .collect::<Vec<_>>();
        crate::shlex::join_unix(normalized_args.iter().map(|s| s.as_ref()))
    }

    pub fn normalize_command_arg(&self, s: &str) -> String {
        let mut s = s.to_owned();
        s = self.replace_root_in_arg(s, self.source_root.as_deref(), ".");
        s = self.replace_root_in_arg(s, self.target_root.as_deref(), TARGET_DIR_PLACEHOLDER);

        for (from, to) in &self.replace_table {
            s = s.replace(from, to);
        }
        if self.show_toolchain_root {
            s = s.replace(&self.toolchain_root, "$MOON_TOOLCHAIN_ROOT");
        }
        s = s.replace(&self.moon_home, "$MOON_HOME");
        s = s.replace('\\', "/");
        s = self.normalize_binary_file_name(s);

        s
    }

    pub fn normalize_path(&self, path: &str) -> String {
        let path_obj = Path::new(path);
        if let Some(source_root) = &self.source_root
            && let Ok(stripped) = path_obj.strip_prefix(source_root)
        {
            return Self::relative_from_path(stripped);
        }
        if let Some(target_root) = &self.target_root
            && let Ok(stripped) = path_obj.strip_prefix(target_root)
        {
            return Self::path_from_placeholder(TARGET_DIR_PLACEHOLDER, stripped);
        }
        let mut path = path.to_owned();
        if self.show_toolchain_root {
            path = path.replace(&self.toolchain_root, "$MOON_TOOLCHAIN_ROOT");
        }
        path = path.replace(&self.moon_home, "$MOON_HOME");
        path = path.replace('\\', "/");
        path = self.normalize_binary_file_name(path);

        path
    }

    fn normalize_binary_file_name(&self, s: String) -> String {
        self.binary_file_name_table
            .iter()
            .find_map(|(from, to)| {
                if s == *from {
                    Some(to.clone())
                } else {
                    s.strip_suffix(from)
                        .filter(|prefix| prefix.ends_with('/'))
                        .map(|prefix| format!("{prefix}{to}"))
                }
            })
            .unwrap_or(s)
    }

    fn replace_root_in_arg(&self, s: String, root: Option<&Path>, placeholder: &str) -> String {
        let Some(root) = root else {
            return s;
        };

        let prefix = root.to_string_lossy();
        let prefix_str = prefix.as_ref();
        let mut out = String::with_capacity(s.len());
        let mut cursor = 0;
        while let Some(offset) = s[cursor..].find(prefix_str) {
            let start = cursor + offset;
            let end = start + prefix_str.len();
            let before = s[..start].chars().next_back();
            let after = s[end..].chars().next();
            let before_ok = before.is_none_or(|c| matches!(c, ':' | ';' | '='));
            let after_ok = after.is_none_or(|c| matches!(c, '/' | '\\' | ':' | ';'));

            if before_ok && after_ok {
                out.push_str(&s[cursor..start]);
                out.push_str(placeholder);
                if matches!(after, Some('/') | Some('\\')) {
                    out.push('/');
                    cursor = end + after.map_or(0, char::len_utf8);
                } else {
                    cursor = end;
                }
            } else {
                out.push_str(&s[cursor..end]);
                cursor = end;
            }
        }
        out.push_str(&s[cursor..]);
        out
    }

    fn relative_from_path(stripped: &Path) -> String {
        Self::path_from_placeholder(".", stripped)
    }

    fn path_from_placeholder(placeholder: &str, stripped: &Path) -> String {
        if stripped.as_os_str().is_empty() {
            placeholder.to_owned()
        } else {
            let normalized = stripped.to_string_lossy().replace('\\', "/");
            format!("{}/{}", placeholder, normalized)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{PathNormalizer, dry_run_path_order_key};

    #[test]
    fn dry_run_path_order_key_uses_file_name_before_full_path() {
        assert!(
            dry_run_path_order_key(Path::new("pkg/z/a.mbt"))
                < dry_run_path_order_key(Path::new("pkg/a/z.mbt"))
        );
        assert_eq!(
            dry_run_path_order_key(Path::new(r"pkg\main.mbt")),
            ("main.mbt".to_owned(), "pkg/main.mbt".to_owned())
        );
    }

    #[test]
    fn normalizes_known_tool_exe_suffix_without_touching_native_outputs() {
        let replacer = PathNormalizer {
            source_root: None,
            target_root: None,
            replace_table: vec![],
            binary_file_name_table: vec![("moonc.exe".to_owned(), "moonc".to_owned())],
            show_toolchain_root: true,
            toolchain_root: "$MOON_TOOLCHAIN_ROOT".to_owned(),
            moon_home: "$MOON_HOME".to_owned(),
        };

        assert_eq!(replacer.normalize_command_arg("moonc.exe"), "moonc");
        assert_eq!(
            replacer.normalize_command_arg("$MOON_HOME/bin/moonc.exe"),
            "$MOON_HOME/bin/moonc"
        );
        assert_eq!(
            replacer.normalize_path("./_build/native/debug/build/main/main.exe"),
            "./_build/native/debug/build/main/main.exe"
        );
    }

    #[test]
    fn keeps_moon_home_when_roots_match() {
        let replacer = PathNormalizer {
            source_root: None,
            target_root: None,
            replace_table: vec![],
            binary_file_name_table: vec![],
            show_toolchain_root: false,
            toolchain_root: "/tmp/.moon".to_owned(),
            moon_home: "/tmp/.moon".to_owned(),
        };

        assert_eq!(
            replacer.normalize_command_arg("/tmp/.moon/lib/core/prelude"),
            "$MOON_HOME/lib/core/prelude"
        );
        assert_eq!(
            replacer.normalize_path("/tmp/.moon/bin/moonc"),
            "$MOON_HOME/bin/moonc"
        );
    }

    #[test]
    fn keeps_toolchain_root_distinct_when_needed() {
        let replacer = PathNormalizer {
            source_root: None,
            target_root: None,
            replace_table: vec![],
            binary_file_name_table: vec![],
            show_toolchain_root: true,
            toolchain_root: "/tmp/toolchain".to_owned(),
            moon_home: "/tmp/home".to_owned(),
        };

        assert_eq!(
            replacer.normalize_command_arg("/tmp/toolchain/lib/core/prelude"),
            "$MOON_TOOLCHAIN_ROOT/lib/core/prelude"
        );
        assert_eq!(
            replacer.normalize_path("/tmp/toolchain/bin/moonc"),
            "$MOON_TOOLCHAIN_ROOT/bin/moonc"
        );
    }

    #[test]
    fn normalizes_external_target_dir_without_changing_in_project_build_dir() {
        let replacer = PathNormalizer {
            source_root: Some("/tmp/project".into()),
            target_root: Some("/tmp/project-target".into()),
            replace_table: vec![],
            binary_file_name_table: vec![],
            show_toolchain_root: false,
            toolchain_root: "/tmp/.moon".to_owned(),
            moon_home: "/tmp/.moon".to_owned(),
        };

        assert_eq!(
            replacer.normalize_command_arg("/tmp/project/_build/native/main.wasm"),
            "./_build/native/main.wasm"
        );
        assert_eq!(
            replacer.normalize_command_arg("/tmp/project-target/native/main.wasm"),
            "$MOON_TARGET_DIR/native/main.wasm"
        );
        assert_eq!(
            replacer.normalize_command_arg("pkg:/tmp/project-target/native/main.mi:main"),
            "pkg:$MOON_TARGET_DIR/native/main.mi:main"
        );
        assert_eq!(
            replacer.normalize_command_arg("/tmp/project-targeted/native/main.wasm"),
            "/tmp/project-targeted/native/main.wasm"
        );
        assert_eq!(
            replacer.normalize_path("/tmp/project-target/native/main.wasm"),
            "$MOON_TARGET_DIR/native/main.wasm"
        );
    }
}
