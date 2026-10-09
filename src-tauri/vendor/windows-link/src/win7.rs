/// Win7 经典 COM 使用 ole32；可选接口动态解析，缺失时保留失败语义。
#[macro_export]
macro_rules! link {
    ($library:literal $abi:literal fn RoGetActivationFactory $($signature:tt)*) => (
        $crate::link!(@optional "combase.dll" $abi RoGetActivationFactory $($signature)* => core::mem::transmute::<u32, _>(0x80004001));
    );
    ($library:literal $abi:literal fn CoIncrementMTAUsage $($signature:tt)*) => (
        $crate::link!(@optional "ole32.dll" $abi CoIncrementMTAUsage $($signature)* => core::mem::transmute::<u32, _>(0x80004001));
    );
    ($library:literal $abi:literal fn DeriveCapabilitySidsFromName $($signature:tt)*) => (
        $crate::link!(@optional "kernelbase.dll" $abi DeriveCapabilitySidsFromName $($signature)* => core::mem::zeroed());
    );
    ($library:literal $abi:literal fn DeriveAppContainerSidFromAppContainerName $($signature:tt)*) => (
        $crate::link!(@optional "userenv.dll" $abi DeriveAppContainerSidFromAppContainerName $($signature)* => core::mem::transmute::<u32, _>(0x80004001));
    );
    (@optional $library:literal $abi:literal $function:ident ($($arg:ident : $type:ty),*) -> $result:ty => $fallback:expr) => (
        pub unsafe extern $abi fn $function($($arg: $type),*) -> $result {
            let library: &[u16] = &const { const S: &str = $library; let mut a = [0u16; S.len()+1]; let mut i=0; while i<S.len() { a[i]=S.as_bytes()[i] as u16; i+=1; } a };
            let address = unsafe { $crate::resolve_optional(library, concat!(stringify!($function), "\0").as_bytes()) };
            if address.is_null() { return unsafe { $fallback }; }
            let call: unsafe extern $abi fn($($type),*) -> $result = unsafe { core::mem::transmute(address) };
            unsafe { call($($arg),*) }
        }
    );
    ("combase.dll" $abi:literal fn CoTaskMemFree $($signature:tt)*) => (
        $crate::link!("ole32.dll" $abi fn CoTaskMemFree $($signature)*);
    );
    ("combase.dll" $abi:literal fn CoTaskMemAlloc $($signature:tt)*) => (
        $crate::link!("ole32.dll" $abi fn CoTaskMemAlloc $($signature)*);
    );
    ("combase.dll" $abi:literal fn CoTaskMemRealloc $($signature:tt)*) => (
        $crate::link!("ole32.dll" $abi fn CoTaskMemRealloc $($signature)*);
    );
    ("combase.dll" $abi:literal fn CoCreateInstance $($signature:tt)*) => (
        $crate::link!("ole32.dll" $abi fn CoCreateInstance $($signature)*);
    );
    ("combase.dll" $abi:literal fn CoInitializeEx $($signature:tt)*) => (
        $crate::link!("ole32.dll" $abi fn CoInitializeEx $($signature)*);
    );
    ("combase.dll" $abi:literal fn CoUninitialize $($signature:tt)*) => (
        $crate::link!("ole32.dll" $abi fn CoUninitialize $($signature)*);
    );
    ("combase.dll" $abi:literal fn CoCreateGuid $($signature:tt)*) => (
        $crate::link!("ole32.dll" $abi fn CoCreateGuid $($signature)*);
    );
    ("combase.dll" $abi:literal fn CoGetContextToken $($signature:tt)*) => (
        $crate::link!("ole32.dll" $abi fn CoGetContextToken $($signature)*);
    );
    ($library:literal $abi:literal $($link_name:literal)? fn $($function:tt)*) => (
        #[cfg(target_arch = "x86")]
        #[link(name = $library, kind = "raw-dylib", modifiers = "+verbatim", import_name_type = "undecorated")]
        #[cfg(target_arch = "x86")]
        extern $abi {
            $(#[link_name=$link_name])?
            pub fn $($function)*;
        }
        #[cfg(not(target_arch = "x86"))]
        #[link(name = $library, kind = "raw-dylib", modifiers = "+verbatim")]
        #[cfg(not(target_arch = "x86"))]
        extern $abi {
            $(#[link_name=$link_name])?
            pub fn $($function)*;
        }
    );
}
