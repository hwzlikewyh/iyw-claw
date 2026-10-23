#![doc = include_str!("../readme.md")]
#![no_std]

#[cfg(windows)]
mod api_query;
#[cfg(windows)]
#[doc(hidden)]
pub use api_query::query_api_set;

/// Defines an external function to import.
#[cfg(all(windows, target_arch = "x86", not(target_vendor = "win7")))]
#[macro_export]
macro_rules! link {
    ("api-ms-win-core-apiquery-l2-1-0.dll" $abi:literal fn IsApiSetImplemented($contract:ident : $input:ty) -> $result:ty) => (
        pub unsafe extern $abi fn IsApiSetImplemented($contract: $input) -> $result {
            let supported = unsafe { $crate::query_api_set(core::mem::transmute($contract)) };
            unsafe { core::mem::transmute::<i32, $result>(supported) }
        }
    );
    ($library:literal $abi:literal $($link_name:literal)? fn $($function:tt)*) => (
        #[link(name = $library, kind = "raw-dylib", modifiers = "+verbatim", import_name_type = "undecorated")]
        extern $abi {
            $(#[link_name=$link_name])?
            pub fn $($function)*;
        }
    )
}

/// Defines an external function to import.
#[cfg(all(windows, not(target_arch = "x86"), not(target_vendor = "win7")))]
#[macro_export]
macro_rules! link {
    ("api-ms-win-core-apiquery-l2-1-0.dll" $abi:literal fn IsApiSetImplemented($contract:ident : $input:ty) -> $result:ty) => (
        pub unsafe extern $abi fn IsApiSetImplemented($contract: $input) -> $result {
            let supported = unsafe { $crate::query_api_set(core::mem::transmute($contract)) };
            unsafe { core::mem::transmute::<i32, $result>(supported) }
        }
    );
    ($library:literal $abi:literal $($link_name:literal)? fn $($function:tt)*) => (
        #[link(name = $library, kind = "raw-dylib", modifiers = "+verbatim")]
        extern $abi {
            $(#[link_name=$link_name])?
            pub fn $($function)*;
        }
    )
}

#[cfg(all(windows, target_vendor = "win7", any(target_arch = "x86", target_arch = "x86_64")))]
mod win7;
#[cfg(all(windows, target_vendor = "win7", any(target_arch = "x86", target_arch = "x86_64")))]
#[path = "optional.rs"]
mod optional;
#[cfg(all(windows, target_vendor = "win7", any(target_arch = "x86", target_arch = "x86_64")))]
pub use optional::resolve_optional;

/// Defines an external function to import.
#[cfg(not(windows))]
#[macro_export]
macro_rules! link {
    ($library:literal $abi:literal $($link_name:literal)? fn $($function:tt)*) => (
        extern $abi {
            pub fn $($function)*;
        }
    )
}
