#![doc = include_str!("../readme.md")]
#![no_std]

/// Defines an external function to import.
#[cfg(all(windows, target_arch = "x86", not(target_vendor = "win7")))]
#[macro_export]
macro_rules! link {
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
