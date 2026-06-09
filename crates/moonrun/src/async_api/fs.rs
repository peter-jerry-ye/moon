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

use std::ffi::OsStr;

use crate::async_host::{AsyncHostError, AsyncHostResult, GuestMemory, GuestRange};
use crate::async_sys::fs::stub;

use super::context::{callback_context, finish_bool, read_i32_arg, with_memory_mut};

pub(super) fn get_tmp_path_len(
    _scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| -> AsyncHostResult<i32> {
        let bytes = encoded_tmp_path()?;
        i32::try_from(bytes.len()).map_err(|_| AsyncHostError::Fault)
    })();
    match result {
        Ok(len) => ret.set_int32(len),
        Err(error) => {
            context.host.record_error(error);
            ret.set_int32(-1);
        }
    }
}

pub(super) fn get_tmp_path(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| -> AsyncHostResult<()> {
        let dst = read_i32_arg(scope, &args, 0)?;
        let len = read_i32_arg(scope, &args, 1)?;
        let bytes = encoded_tmp_path()?;
        let required_len = i32::try_from(bytes.len()).map_err(|_| AsyncHostError::Fault)?;

        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        if len < bytes.len() {
            return Err(AsyncHostError::Inval);
        }

        with_memory_mut(scope, context, |memory| {
            memory.write(GuestRange::new(dst, required_len)?, &bytes)
        })
    })();
    match result {
        Ok(()) => ret.set_int32(0),
        Err(error) => {
            context.host.record_error(error);
            ret.set_int32(-1);
        }
    }
}

fn encoded_tmp_path() -> AsyncHostResult<Vec<u8>> {
    // Native async returns a process-owned C buffer here. Wasm cannot receive
    // host pointers, so the V8 adapter exposes the same OS string value by
    // length-first copy-out in the representation async uses on this platform.
    let path = stub::get_tmp_path()?;
    Ok(encode_os_string_for_wasm(path.as_os_str()))
}

#[cfg(unix)]
fn encode_os_string_for_wasm(path: &OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;

    path.as_bytes().to_vec()
}

#[cfg(windows)]
fn encode_os_string_for_wasm(path: &OsStr) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;

    path.encode_wide()
        .flat_map(|code_unit| code_unit.to_le_bytes())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn os_string_wasm_encoding_uses_unix_bytes() {
        use std::os::unix::ffi::OsStrExt;

        let path = OsStr::from_bytes(b"/tmp/\xff");

        assert_eq!(encode_os_string_for_wasm(path), b"/tmp/\xff");
    }

    #[cfg(windows)]
    #[test]
    fn os_string_wasm_encoding_uses_utf16_code_units() {
        let path = OsStr::new("A\u{10000}");

        assert_eq!(
            encode_os_string_for_wasm(path),
            [0x41, 0x00, 0x00, 0xd8, 0x00, 0xdc]
        );
    }
}

pub(super) fn errno_is_lock_violation(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    match read_i32_arg(scope, &args, 0) {
        Ok(errno) => finish_bool(&mut ret, stub::errno_is_lock_violation(errno)),
        Err(error) => {
            context.host.record_error(error);
            finish_bool(&mut ret, false);
        }
    }
}
