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

use std::any::Any;
use std::sync::OnceLock;

use crate::async_host::{AsyncHost, AsyncHostError, AsyncHostResult};
use crate::async_manifest::{ASYNC_IMPORT_MAPPINGS, AsyncImportStatus};
use crate::v8_builder::ObjectExt;

const ASYNC_ERRNO_SUCCESS: i32 = 0;
#[cfg(test)]
const IMPLEMENTED_IMPORTS: &[&str] = &[
    "get_platform",
    "get_ms_since_epoch",
    "get_errno",
    "is_nonblocking_io_error",
    "is_EINTR",
    "is_ENOENT",
    "is_EEXIST",
    "is_EACCES",
    "is_ECONNREFUSED",
    "is_ERROR_NOTIFY_ENUM_DIR",
    "get_ENOTDIR",
];

struct AsyncContext {
    host: AsyncHost,
    memory: OnceLock<v8::Global<v8::WasmMemoryObject>>,
}

fn callback_context<'s>(args: &v8::FunctionCallbackArguments<'s>) -> &'s AsyncContext {
    let data = args.data();
    assert!(data.is_external());
    let data: v8::Local<v8::Data> = data.into();
    let ptr = v8::Local::<v8::External>::try_from(data).unwrap().value();
    unsafe { &*(ptr as *const AsyncContext) }
}

fn read_i32_arg(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    index: i32,
) -> AsyncHostResult<i32> {
    args.get(index)
        .int32_value(scope)
        .ok_or(AsyncHostError::Inval)
}

fn cached_memory<'s>(
    scope: &mut v8::HandleScope<'s>,
    context: &AsyncContext,
) -> AsyncHostResult<v8::Local<'s, v8::WasmMemoryObject>> {
    context
        .memory
        .get()
        .map(|memory| v8::Local::new(scope, memory))
        .ok_or(AsyncHostError::Fault)
}

fn with_memory_mut<T>(
    scope: &mut v8::HandleScope,
    context: &AsyncContext,
    f: impl FnOnce(&mut [u8]) -> AsyncHostResult<T>,
) -> AsyncHostResult<T> {
    let memory_object = cached_memory(scope, context)?;
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

fn finish_errno(context: &AsyncContext, ret: &mut v8::ReturnValue, result: AsyncHostResult<()>) {
    let errno = match result {
        Ok(()) => ASYNC_ERRNO_SUCCESS,
        Err(error) => context.host.record_error(error),
    };
    ret.set_int32(errno);
}

fn set_memory(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| -> AsyncHostResult<()> {
        let memory_value = args.get(0);
        let memory = v8::Local::<v8::WasmMemoryObject>::try_from(memory_value)
            .map_err(|_| AsyncHostError::Inval)?;
        let _ = context.memory.set(v8::Global::new(scope, memory));
        Ok(())
    })();
    finish_errno(context, &mut ret, result);
}

fn get_platform(
    _scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    ret.set_int32(context.host.platform());
}

fn get_ms_since_epoch(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let value = v8::BigInt::new_from_i64(scope, context.host.ms_since_epoch());
    ret.set(value.into());
}

fn sleep_ms(
    scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    let result = (|| -> AsyncHostResult<()> {
        let duration_ms = read_i32_arg(scope, &args, 0)?;
        context.host.sleep_ms(duration_ms);
        Ok(())
    })();
    finish_errno(context, &mut ret, result);
}

fn copy_from_guest(
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

fn zero_guest(
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

fn get_errno(
    _scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    ret.set_int32(context.host.get_errno());
}

fn is_errno_predicate(
    scope: &mut v8::HandleScope,
    args: &v8::FunctionCallbackArguments,
    predicate: impl FnOnce(&AsyncHost, i32) -> bool,
) -> AsyncHostResult<bool> {
    let errno = read_i32_arg(scope, args, 0)?;
    let context = callback_context(args);
    Ok(predicate(&context.host, errno))
}

fn finish_bool(ret: &mut v8::ReturnValue, value: bool) {
    ret.set_int32(if value { 1 } else { 0 });
}

macro_rules! errno_predicate {
    ($callback:ident, $method:ident) => {
        fn $callback(
            scope: &mut v8::HandleScope,
            args: v8::FunctionCallbackArguments,
            mut ret: v8::ReturnValue,
        ) {
            let context = callback_context(&args);
            match is_errno_predicate(scope, &args, |host, errno| host.$method(errno)) {
                Ok(value) => finish_bool(&mut ret, value),
                Err(error) => {
                    context.host.record_error(error);
                    finish_bool(&mut ret, false);
                }
            }
        }
    };
}

errno_predicate!(is_nonblocking_io_error, is_nonblocking_io_error);
errno_predicate!(is_eintr, is_eintr);
errno_predicate!(is_enoent, is_enoent);
errno_predicate!(is_eexist, is_eexist);
errno_predicate!(is_eacces, is_eacces);
errno_predicate!(is_econnrefused, is_econnrefused);
errno_predicate!(is_error_notify_enum_dir, is_error_notify_enum_dir);

fn get_enotdir(
    _scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    ret.set_int32(context.host.enotdir());
}

fn unsupported_i32(
    _scope: &mut v8::HandleScope,
    args: v8::FunctionCallbackArguments,
    mut ret: v8::ReturnValue,
) {
    let context = callback_context(&args);
    ret.set_int32(context.host.unsupported_return());
}

fn set_func_impl<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    name: &str,
    callback: impl v8::MapFnTo<v8::FunctionCallback>,
    context_ptr: *const AsyncContext,
) {
    let data = v8::External::new(scope, context_ptr as *mut std::ffi::c_void);
    let function = v8::Function::builder(callback)
        .data(data.into())
        .build(scope)
        .unwrap();
    obj.set_value(scope, name, function.into());
}

macro_rules! set_func {
    ($obj:expr, $scope:expr, $context_ptr:expr, $name:literal, $callback:ident) => {
        set_func_impl($obj, $scope, $name, $callback, $context_ptr);
    };
}

pub(crate) fn init_env<'s>(
    obj: v8::Local<'s, v8::Object>,
    scope: &mut v8::HandleScope<'s>,
    dtors: &mut Vec<Box<dyn Any>>,
) {
    let context = Box::new(AsyncContext {
        host: AsyncHost::default(),
        memory: OnceLock::new(),
    });
    let context_ptr = &*context as *const AsyncContext;
    dtors.push(context);

    set_func!(obj, scope, context_ptr, "set_memory", set_memory);
    set_func!(obj, scope, context_ptr, "sleep_ms", sleep_ms);
    set_func!(obj, scope, context_ptr, "copy_from_guest", copy_from_guest);
    set_func!(obj, scope, context_ptr, "zero_guest", zero_guest);

    set_func!(obj, scope, context_ptr, "get_platform", get_platform);
    set_func!(
        obj,
        scope,
        context_ptr,
        "get_ms_since_epoch",
        get_ms_since_epoch
    );
    set_func!(obj, scope, context_ptr, "get_errno", get_errno);
    set_func!(
        obj,
        scope,
        context_ptr,
        "is_nonblocking_io_error",
        is_nonblocking_io_error
    );
    set_func!(obj, scope, context_ptr, "is_EINTR", is_eintr);
    set_func!(obj, scope, context_ptr, "is_ENOENT", is_enoent);
    set_func!(obj, scope, context_ptr, "is_EEXIST", is_eexist);
    set_func!(obj, scope, context_ptr, "is_EACCES", is_eacces);
    set_func!(obj, scope, context_ptr, "is_ECONNREFUSED", is_econnrefused);
    set_func!(
        obj,
        scope,
        context_ptr,
        "is_ERROR_NOTIFY_ENUM_DIR",
        is_error_notify_enum_dir
    );
    set_func!(obj, scope, context_ptr, "get_ENOTDIR", get_enotdir);

    for mapping in ASYNC_IMPORT_MAPPINGS {
        if mapping.status == AsyncImportStatus::UnsupportedMvp {
            set_func_impl(
                obj,
                scope,
                mapping.wasm_symbol,
                unsupported_i32,
                context_ptr,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::async_manifest::{ASYNC_IMPORT_MAPPINGS, AsyncImportStatus, INTERNAL_IMPORTS};

    use super::IMPLEMENTED_IMPORTS;

    #[test]
    fn adapter_covers_manifest_implemented_imports() {
        let manifest_implemented = ASYNC_IMPORT_MAPPINGS
            .iter()
            .filter(|mapping| mapping.status == AsyncImportStatus::Implemented)
            .map(|mapping| mapping.wasm_symbol)
            .collect::<BTreeSet<_>>();
        let adapter_implemented = IMPLEMENTED_IMPORTS.iter().copied().collect::<BTreeSet<_>>();

        assert_eq!(adapter_implemented, manifest_implemented);
    }

    #[test]
    fn internal_imports_are_adapter_owned_names() {
        assert_eq!(
            INTERNAL_IMPORTS,
            &["set_memory", "sleep_ms", "copy_from_guest", "zero_guest"]
        );
    }
}
