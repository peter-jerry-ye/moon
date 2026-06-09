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

pub(crate) const MOONBIT_V0_MODULE: &str = "moonbit_v0";
#[cfg(test)]
pub(crate) const NATIVE_ASYNC_PREFIX: &str = "moonbitlang_async_";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncImportStatus {
    Implemented,
    UnsupportedMvp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AsyncImportMapping {
    pub(crate) native_symbol: &'static str,
    pub(crate) wasm_symbol: &'static str,
    pub(crate) status: AsyncImportStatus,
}

#[cfg(test)]
pub(crate) const INTERNAL_IMPORTS: &[&str] =
    &["set_memory", "sleep_ms", "copy_from_guest", "zero_guest"];

pub(crate) const ASYNC_IMPORT_MAPPINGS: &[AsyncImportMapping] = &[
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_get_platform",
        wasm_symbol: "get_platform",
        status: AsyncImportStatus::Implemented,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_get_ms_since_epoch",
        wasm_symbol: "get_ms_since_epoch",
        status: AsyncImportStatus::Implemented,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_get_errno",
        wasm_symbol: "get_errno",
        status: AsyncImportStatus::Implemented,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_is_nonblocking_io_error",
        wasm_symbol: "is_nonblocking_io_error",
        status: AsyncImportStatus::Implemented,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_is_EINTR",
        wasm_symbol: "is_EINTR",
        status: AsyncImportStatus::Implemented,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_is_ENOENT",
        wasm_symbol: "is_ENOENT",
        status: AsyncImportStatus::Implemented,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_is_EEXIST",
        wasm_symbol: "is_EEXIST",
        status: AsyncImportStatus::Implemented,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_is_EACCES",
        wasm_symbol: "is_EACCES",
        status: AsyncImportStatus::Implemented,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_is_ECONNREFUSED",
        wasm_symbol: "is_ECONNREFUSED",
        status: AsyncImportStatus::Implemented,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_is_ERROR_NOTIFY_ENUM_DIR",
        wasm_symbol: "is_ERROR_NOTIFY_ENUM_DIR",
        status: AsyncImportStatus::Implemented,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_get_ENOTDIR",
        wasm_symbol: "get_ENOTDIR",
        status: AsyncImportStatus::Implemented,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_poll_create",
        wasm_symbol: "poll_create",
        status: AsyncImportStatus::UnsupportedMvp,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_poll_register",
        wasm_symbol: "poll_register",
        status: AsyncImportStatus::UnsupportedMvp,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_poll_wait",
        wasm_symbol: "poll_wait",
        status: AsyncImportStatus::UnsupportedMvp,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_init_thread_pool",
        wasm_symbol: "init_thread_pool",
        status: AsyncImportStatus::UnsupportedMvp,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_make_open_job",
        wasm_symbol: "make_open_job",
        status: AsyncImportStatus::UnsupportedMvp,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_make_read_job",
        wasm_symbol: "make_read_job",
        status: AsyncImportStatus::UnsupportedMvp,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_make_write_job",
        wasm_symbol: "make_write_job",
        status: AsyncImportStatus::UnsupportedMvp,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_make_tcp_socket",
        wasm_symbol: "make_tcp_socket",
        status: AsyncImportStatus::UnsupportedMvp,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_make_udp_socket",
        wasm_symbol: "make_udp_socket",
        status: AsyncImportStatus::UnsupportedMvp,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_make_spawn_job",
        wasm_symbol: "make_spawn_job",
        status: AsyncImportStatus::UnsupportedMvp,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_schannel_new",
        wasm_symbol: "schannel_new",
        status: AsyncImportStatus::UnsupportedMvp,
    },
    AsyncImportMapping {
        native_symbol: "moonbitlang_async_tls_client_ctx",
        wasm_symbol: "tls_client_ctx",
        status: AsyncImportStatus::UnsupportedMvp,
    },
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn native_async_symbols_map_by_stripping_namespace_prefix() {
        for mapping in ASYNC_IMPORT_MAPPINGS {
            let suffix = mapping
                .native_symbol
                .strip_prefix(NATIVE_ASYNC_PREFIX)
                .expect("native async mapping must use the async C namespace");
            assert_eq!(mapping.wasm_symbol, suffix);
            assert!(!mapping.wasm_symbol.starts_with("async_"));
        }
    }

    #[test]
    fn wasm_import_names_are_unique() {
        let mut seen = BTreeSet::new();
        for import in INTERNAL_IMPORTS {
            assert!(seen.insert(*import), "duplicate internal import {import}");
        }
        for mapping in ASYNC_IMPORT_MAPPINGS {
            assert!(
                seen.insert(mapping.wasm_symbol),
                "duplicate async import {}",
                mapping.wasm_symbol
            );
        }
    }
}
