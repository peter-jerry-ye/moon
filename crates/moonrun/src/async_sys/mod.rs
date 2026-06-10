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

pub(crate) mod fs;
pub(crate) mod internal;
pub(crate) mod os_error;

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PortedSymbol {
    pub(crate) rust_module: &'static str,
    pub(crate) rust_symbol: &'static str,
    pub(crate) native_symbol: &'static str,
    pub(crate) source: &'static str,
}

macro_rules! ported_fns {
    ($(
        #[ported(source = $source:literal, original = $original:literal)]
        $(#[$meta:meta])*
        $vis:vis fn $name:ident($($args:tt)*) $(-> $ret:ty)? $body:block
    )+) => {
        #[cfg(test)]
        pub(crate) const PORTED_SYMBOLS: &[crate::async_sys::PortedSymbol] = &[
            $(
                crate::async_sys::PortedSymbol {
                    rust_module: module_path!(),
                    rust_symbol: stringify!($name),
                    native_symbol: $original,
                    source: $source,
                },
            )+
        ];

        $(
            $(#[$meta])*
            $vis fn $name($($args)*) $(-> $ret)? $body
        )*
    };
}

pub(crate) use ported_fns;

#[cfg(test)]
pub(crate) fn ported_symbols() -> Vec<PortedSymbol> {
    let mut symbols = Vec::new();
    symbols.extend_from_slice(internal::fd_util::stub::PORTED_SYMBOLS);
    symbols.extend_from_slice(internal::event_loop::thread_pool::PORTED_SYMBOLS);
    symbols.extend_from_slice(internal::time::time::PORTED_SYMBOLS);
    symbols.extend_from_slice(fs::stub::PORTED_SYMBOLS);
    symbols.extend_from_slice(os_error::stub::PORTED_SYMBOLS);
    symbols
}
