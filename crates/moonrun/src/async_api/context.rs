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

use crate::async_host::{AsyncHost, AsyncHostError, AsyncHostResult};

const ASYNC_ERRNO_SUCCESS: i32 = 0;

pub(super) struct AsyncContext {
    pub(super) host: AsyncHost,
    imports: v8::Global<v8::Object>,
}

impl AsyncContext {
    pub(super) fn new<'s>(
        scope: &mut v8::HandleScope<'s>,
        imports: v8::Local<'s, v8::Object>,
        host: AsyncHost,
    ) -> Self {
        Self {
            host,
            imports: v8::Global::new(scope, imports),
        }
    }
}

pub(super) fn callback_context<'s>(args: &v8::FunctionCallbackArguments<'s>) -> &'s AsyncContext {
    let data = args.data();
    assert!(data.is_external());
    let data: v8::Local<v8::Data> = data.into();
    let ptr = v8::Local::<v8::External>::try_from(data).unwrap().value();
    unsafe { &*(ptr as *const AsyncContext) }
}

pub(super) fn read_i32_arg(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    index: i32,
) -> AsyncHostResult<i32> {
    args.get(index)
        .int32_value(scope)
        .ok_or(AsyncHostError::Inval)
}

fn memory_object<'s>(
    scope: &mut v8::HandleScope<'s>,
    context: &AsyncContext,
) -> AsyncHostResult<v8::Local<'s, v8::WasmMemoryObject>> {
    let imports = v8::Local::new(scope, &context.imports);
    let key = v8::String::new(scope, "memory").ok_or(AsyncHostError::Fault)?;
    let memory = imports
        .get(scope, key.into())
        .ok_or(AsyncHostError::Fault)?;
    v8::Local::<v8::WasmMemoryObject>::try_from(memory).map_err(|_| AsyncHostError::Fault)
}

pub(super) fn with_memory_mut<T>(
    scope: &mut v8::HandleScope,
    context: &AsyncContext,
    f: impl FnOnce(&mut [u8]) -> AsyncHostResult<T>,
) -> AsyncHostResult<T> {
    let memory_object = memory_object(scope, context)?;
    let buffer = memory_object.buffer();
    let len = buffer.byte_length();

    let Some(ptr) = buffer.data() else {
        if len == 0 {
            let mut empty = [];
            return f(&mut empty);
        }
        return Err(AsyncHostError::Fault);
    };

    let memory = unsafe { std::slice::from_raw_parts_mut(ptr.as_ptr() as *mut u8, len) };
    f(memory)
}

pub(super) fn finish_errno(
    context: &AsyncContext,
    ret: &mut v8::ReturnValue,
    result: AsyncHostResult<()>,
) {
    let errno = match result {
        Ok(()) => ASYNC_ERRNO_SUCCESS,
        Err(error) => context.host.record_error(error),
    };
    ret.set_int32(errno);
}

pub(super) fn finish_bool(ret: &mut v8::ReturnValue, value: bool) {
    ret.set_int32(if value { 1 } else { 0 });
}
