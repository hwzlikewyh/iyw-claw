// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

pub(super) fn status_error(what: &str, status: OSStatus) -> String {
    let name = match status {
        -67050 => " (errSecCSReqFailed: does not satisfy the requirement)",
        -67062 => " (errSecCSUnsigned)",
        -67061 => " (errSecCSSignatureFailed)",
        -67030 => " (errSecCSStaticCodeChanged: the file on disk no longer matches)",
        -67065 => " (errSecCSNoSuchCode)",
        -67052 => " (errSecCSReqInvalid)",
        -67068 => " (errSecCSGuestInvalid)",
        _ => "",
    };
    format!("{what} failed: OSStatus {status}{name}")
}

pub(super) fn copy_guest(guest: Guest) -> Result<Owned, String> {
    // SAFETY: the extern statics are valid CFStrings for the life of the
    // process; wrapping under the get rule retains nothing we must release.
    let dict = unsafe {
        match guest {
            Guest::Audit(token) => {
                let bytes: Vec<u8> = token.0.iter().flat_map(|w| w.to_ne_bytes()).collect();
                CFDictionary::from_CFType_pairs(&[(
                    CFString::wrap_under_get_rule(kSecGuestAttributeAudit).as_CFType(),
                    CFData::from_buffer(&bytes).as_CFType(),
                )])
            }
            Guest::ChildPid(pid) => {
                let pid = i32::try_from(pid).map_err(|_| "pid out of range".to_string())?;
                CFDictionary::from_CFType_pairs(&[(
                    CFString::wrap_under_get_rule(kSecGuestAttributePid).as_CFType(),
                    CFNumber::from(pid).as_CFType(),
                )])
            }
        }
    };
    let mut code: SecCodeRef = std::ptr::null();
    // SAFETY: a null host means "the system's guest registry"; `dict` is a
    // live dictionary and `code` receives a +1 reference on success.
    let status = unsafe {
        SecCodeCopyGuestWithAttributes(
            std::ptr::null(),
            dict.as_concrete_TypeRef(),
            K_SEC_CS_DEFAULT_FLAGS,
            &mut code,
        )
    };
    if status != 0 {
        return Err(status_error("SecCodeCopyGuestWithAttributes", status));
    }
    Ok(Owned(code))
}

pub(super) fn check_requirement(code: &Owned, requirement: &str) -> Result<(), String> {
    let text = CFString::new(requirement);
    let mut req: SecRequirementRef = std::ptr::null();
    // SAFETY: `text` is live; `req` receives a +1 reference on success.
    let status = unsafe {
        SecRequirementCreateWithString(text.as_concrete_TypeRef(), K_SEC_CS_DEFAULT_FLAGS, &mut req)
    };
    if status != 0 {
        return Err(status_error("SecRequirementCreateWithString", status));
    }
    let req = Owned(req);
    // SAFETY: both references are live for the call.
    let status = unsafe { SecCodeCheckValidity(code.0, K_SEC_CS_DEFAULT_FLAGS, req.0) };
    if status != 0 {
        return Err(status_error("SecCodeCheckValidity", status));
    }
    Ok(())
}

pub(super) fn signing_info(code: &Owned) -> Result<CodeInfo, String> {
    let mut raw: CFDictionaryRef = std::ptr::null();
    // SAFETY: `code` is live; `raw` receives a +1 dictionary on success.
    let status = unsafe {
        SecCodeCopySigningInformation(
            code.0,
            K_SEC_CS_SIGNING_INFORMATION | K_SEC_CS_REQUIREMENT_INFORMATION,
            &mut raw,
        )
    };
    if status != 0 {
        return Err(status_error("SecCodeCopySigningInformation", status));
    }
    // SAFETY: a +1 CFDictionary from the call above, released by the wrapper.
    let dict: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_create_rule(raw) };
    // SAFETY: extern CFString keys, valid for the life of the process.
    let key = |k: CFStringRef| unsafe { CFString::wrap_under_get_rule(k) };
    let string = |k: CFStringRef| {
        dict.find(key(k))
            .and_then(|v| v.downcast::<CFString>())
            .map(|s| s.to_string())
    };
    let mut info = CodeInfo {
        // SAFETY for the statics in this block: see `key`.
        identifier: string(unsafe { kSecCodeInfoIdentifier }),
        team_id: string(unsafe { kSecCodeInfoTeamIdentifier }),
        cdhash: dict
            .find(key(unsafe { kSecCodeInfoUnique }))
            .and_then(|v| v.downcast::<CFData>())
            .map(|d| hex(d.bytes())),
        flags: dict
            .find(key(unsafe { kSecCodeInfoFlags }))
            .and_then(|v| v.downcast::<CFNumber>())
            .and_then(|n| n.to_i64())
            .and_then(|n| u32::try_from(n).ok()),
        entitlements: Vec::new(),
    };
    if let Some(ents) = dict
        .find(key(unsafe { kSecCodeInfoEntitlementsDict }))
        .and_then(|v| v.downcast::<CFDictionary>())
    {
        // SAFETY: the dictionary is live; keys come back as borrowed CF refs.
        let ents: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule(ents.as_concrete_TypeRef()) };
        let (keys, values) = ents.get_keys_and_values();
        for (k, v) in keys.into_iter().zip(values) {
            // SAFETY: entries of a live dictionary, borrowed.
            let name = unsafe { CFString::wrap_under_get_rule(k as CFStringRef) }.to_string();
            let value = unsafe { CFType::wrap_under_get_rule(v) };
            let off = value
                .downcast::<CFBoolean>()
                .is_some_and(|b| !bool::from(b));
            if !off {
                info.entitlements.push(name);
            }
        }
        info.entitlements.sort();
    }
    Ok(info)
}
