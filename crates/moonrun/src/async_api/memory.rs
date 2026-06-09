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

use crate::async_host::AsyncHostResult;

use super::context::{callback_context, finish_errno, read_i32_arg, with_memory_mut};

pub(super) fn copy_from_guest(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| -> AsyncHostResult<i32> {
        let ptr = read_i32_arg(scope, &args, 0)?;
        let len = read_i32_arg(scope, &args, 1)?;
        with_memory_mut(scope, context, |memory| {
            context.host.copy_from_guest_len(memory, ptr, len)
        })
    })();
    match result {
        Ok(len) => ret.set_int32(len),
        Err(error) => {
            context.host.record_error(error);
            ret.set_int32(-1);
        }
    }
}

pub(super) fn zero_guest(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| -> AsyncHostResult<()> {
        let ptr = read_i32_arg(scope, &args, 0)?;
        let len = read_i32_arg(scope, &args, 1)?;
        with_memory_mut(scope, context, |memory| {
            context.host.zero_guest(memory, ptr, len)
        })
    })();
    finish_errno(context, &mut ret, result);
}
