//! 与向导一致，通过 Windows 原生信任链核验发布者，不依赖 PowerShell 提供程序。
use std::{
    fs::File,
    os::windows::{ffi::OsStrExt, io::AsRawHandle},
    path::Path,
};
use windows_sys::Win32::Security::{Cryptography::*, WinTrust::*};

pub(super) fn verify(path: &Path, file: &File) -> Result<(), String> {
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // 路径和文件句柄在整个验证及状态释放期间都保持有效。
    unsafe {
        let mut input: WINTRUST_FILE_INFO = std::mem::zeroed();
        input.cbStruct = std::mem::size_of_val(&input) as u32;
        input.pcwszFilePath = name.as_ptr();
        input.hFile = file.as_raw_handle();
        let mut trust: WINTRUST_DATA = std::mem::zeroed();
        trust.cbStruct = std::mem::size_of_val(&trust) as u32;
        trust.dwUIChoice = WTD_UI_NONE;
        trust.fdwRevocationChecks = WTD_REVOKE_WHOLECHAIN;
        trust.dwUnionChoice = WTD_CHOICE_FILE;
        trust.Anonymous.pFile = &mut input;
        trust.dwStateAction = WTD_STATEACTION_VERIFY;
        trust.dwProvFlags = WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT;
        let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        let status = WinVerifyTrust(
            std::ptr::null_mut(),
            &mut action,
            (&mut trust as *mut WINTRUST_DATA).cast(),
        );
        let publisher = status == 0 && matches_publisher(&trust);
        trust.dwStateAction = WTD_STATEACTION_CLOSE;
        WinVerifyTrust(
            std::ptr::null_mut(),
            &mut action,
            (&mut trust as *mut WINTRUST_DATA).cast(),
        );
        if status != 0 {
            return Err(format!(
                "Installer Authenticode verification failed: {status:#x}"
            ));
        }
        if !publisher {
            return Err("Installer Authenticode publisher mismatch".into());
        }
    }
    Ok(())
}

unsafe fn matches_publisher(trust: &WINTRUST_DATA) -> bool {
    let provider = WTHelperProvDataFromStateData(trust.hWVTStateData);
    if provider.is_null() {
        return false;
    }
    let signer = WTHelperGetProvSignerFromChain(provider, 0, 0, 0);
    if signer.is_null() || (*signer).csCertChain == 0 || (*signer).pasCertChain.is_null() {
        return false;
    }
    let mut name = [0u16; 256];
    let count = CertGetNameStringW(
        (*(*signer).pasCertChain).pCert,
        CERT_NAME_SIMPLE_DISPLAY_TYPE,
        0,
        std::ptr::null(),
        name.as_mut_ptr(),
        name.len() as u32,
    );
    count > 1
        && String::from_utf16_lossy(&name[..count as usize - 1]) == "厦门市虹约产品设计有限公司"
}
