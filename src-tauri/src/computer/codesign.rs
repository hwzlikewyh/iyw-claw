//! macOS code-signature checks: who is on the other end of a socket, and what
//! a suspended child is about to run.
//!
//! Both questions are answered by the kernel's view of a *running* process,
//! never by a path: a path names a file, and the files in question sit in
//! places another process of the same user can rewrite. The socket peer is
//! named by the audit token the kernel attaches to the connection
//! (`LOCAL_PEERTOKEN`), which carries the pid *and* its version, so a pid that
//! has been recycled does not resolve to the new process. The child is named
//! by its pid while it is suspended and unreaped — our own child, which no
//! one else can have been given its pid.
//!
//! Security.framework and `csops` are not governed by TCC, so these checks
//! are as safe to run inside iyw-claw as inside the helper.

use std::ffi::{c_int, c_void};
use std::os::fd::RawFd;

use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::data::CFData;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_foundation_sys::base::{CFRelease, CFTypeRef};
use core_foundation_sys::dictionary::CFDictionaryRef;
use core_foundation_sys::string::CFStringRef;

use super::driver::DENIED_ENTITLEMENTS;

#[path = "parts/codesign/SecCodeRef_group_1.rs"]
mod part_1;
use part_1::*;

#[path = "parts/codesign/CS_VALID_group_2.rs"]
mod part_2;
pub use part_2::*;

#[link(name = "Security", kind = "framework")]
extern "C" {
    static kSecGuestAttributeAudit: CFStringRef;
    static kSecGuestAttributePid: CFStringRef;
    static kSecCodeInfoIdentifier: CFStringRef;
    static kSecCodeInfoTeamIdentifier: CFStringRef;
    static kSecCodeInfoUnique: CFStringRef;
    static kSecCodeInfoFlags: CFStringRef;
    static kSecCodeInfoEntitlementsDict: CFStringRef;

    fn SecCodeCopyGuestWithAttributes(
        host: SecCodeRef,
        attributes: CFDictionaryRef,
        flags: SecCSFlags,
        guest: *mut SecCodeRef,
    ) -> OSStatus;
    fn SecCodeCheckValidity(
        code: SecCodeRef,
        flags: SecCSFlags,
        requirement: SecRequirementRef,
    ) -> OSStatus;
    fn SecRequirementCreateWithString(
        text: CFStringRef,
        flags: SecCSFlags,
        requirement: *mut SecRequirementRef,
    ) -> OSStatus;
    fn SecCodeCopySigningInformation(
        code: SecCodeRef,
        flags: SecCSFlags,
        information: *mut CFDictionaryRef,
    ) -> OSStatus;
    fn SecCodeCopySelf(flags: SecCSFlags, code: *mut SecCodeRef) -> OSStatus;
}

extern "C" {
    /// libsystem_kernel; not in the `libc` crate.
    fn csops(pid: libc::pid_t, ops: u32, useraddr: *mut c_void, usersize: libc::size_t) -> c_int;
}

#[path = "parts/codesign/Owned_group_3.rs"]
mod part_3;
use part_3::*;

#[path = "parts/codesign/drop_group_4.rs"]
mod part_4;

#[path = "parts/codesign/AuditToken_group_5.rs"]
mod part_5;
pub use part_5::*;

#[path = "parts/codesign/fmt_group_6.rs"]
mod part_6;

#[path = "parts/codesign/peer_audit_token_group_7.rs"]
mod part_7;
pub use part_7::*;

#[path = "parts/codesign/entitlements_clean_group_8.rs"]
mod part_8;

#[path = "parts/codesign/status_error_group_9.rs"]
mod part_9;
use part_9::*;

#[path = "parts/codesign/check_guest_group_10.rs"]
mod part_10;
pub use part_10::*;

#[path = "parts/codesign/hex_group_11.rs"]
mod part_11;
use part_11::*;
