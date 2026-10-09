// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

/// The application `executable` is the main executable of — or sits inside,
/// when it is the main executable of a helper application within another.
#[cfg(target_os = "macos")]
pub(super) fn identify_executable(executable: &str) -> Option<AppIdentity> {
    let own = executable_bundle(executable)?;
    let app = outermost_app_bundle(own);
    if is_system_component(app) {
        return None;
    }
    let info = bundle_info(app)?;
    // Standing by itself, an application by its name or its own word; inside
    // another, by its name only (see the module note).
    let own_is_application = if own == app {
        is_application(own, info.package_type.as_deref())
    } else {
        has_app_extension(own)
    };
    if !own_is_application {
        return None;
    }
    Some(AppIdentity {
        path: app.to_string(),
        bundle_id: info.id?,
        plist_names: info.names,
        nested: own != app,
    })
}

/// The executable `pid` runs, as the kernel has it.
#[cfg(target_os = "macos")]
pub(super) fn executable_path(pid: u32) -> Option<String> {
    let pid = libc::c_int::try_from(pid).ok().filter(|p| *p > 0)?;
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: `buf` is a live buffer of exactly the size passed; the call
    // writes at most that many bytes and returns how many it wrote.
    let written = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
    let written = usize::try_from(written).ok().filter(|n| *n > 0)?;
    buf.truncate(written);
    String::from_utf8(buf).ok()
}

/// What a bundle's `Info.plist` says of it.
#[cfg(target_os = "macos")]
#[derive(Debug, Default)]
pub(super) struct BundleInfo {
    /// `CFBundleIdentifier`.
    pub(in crate::computer::appident) id: Option<String>,
    /// `CFBundlePackageType`.
    pub(in crate::computer::appident) package_type: Option<String>,
    /// `CFBundleDisplayName` and `CFBundleName`, where given.
    pub(in crate::computer::appident) names: Vec<String>,
}

/// `bundle`'s `Info.plist`, XML or binary.
#[cfg(target_os = "macos")]
pub(super) fn bundle_info(bundle: &str) -> Option<BundleInfo> {
    use std::io::Read;

    use core_foundation::base::{CFType, TCFType};
    use core_foundation::data::CFData;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::propertylist::{
        create_with_data, kCFPropertyListImmutable, CFPropertyList,
    };
    use core_foundation::string::CFString;

    let file =
        std::fs::File::open(std::path::Path::new(bundle).join("Contents/Info.plist")).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_INFO_PLIST + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_INFO_PLIST {
        return None;
    }
    let (raw, _format) =
        create_with_data(CFData::from_buffer(&bytes), kCFPropertyListImmutable).ok()?;
    // SAFETY: `create_with_data` returned a +1 property list, released by the
    // wrapper.
    let plist = unsafe { CFPropertyList::wrap_under_create_rule(raw) };
    let dict = plist.downcast_into::<CFDictionary>()?;
    // SAFETY: the same dictionary, viewed with the key and value types every
    // property-list dictionary has; retained by the view for its own life.
    let dict: CFDictionary<CFString, CFType> =
        unsafe { CFDictionary::wrap_under_get_rule(dict.as_concrete_TypeRef()) };
    let text = |key: &'static str| {
        dict.find(CFString::from_static_string(key))
            .and_then(|value| value.downcast::<CFString>())
            .map(|value| value.to_string())
            .filter(|value| !value.is_empty())
    };
    Some(BundleInfo {
        id: text("CFBundleIdentifier"),
        package_type: text("CFBundlePackageType"),
        names: ["CFBundleDisplayName", "CFBundleName"]
            .into_iter()
            .filter_map(text)
            .collect(),
    })
}
