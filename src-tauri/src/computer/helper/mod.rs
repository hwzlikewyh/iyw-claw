//! `iyw-computer-helper`: the process that holds the OS permissions for
//! computer use, and runs the driver as its child.
//!
//! On macOS the helper is launched by iyw-claw as its own TCC responsible
//! process, so Accessibility and Screen Recording are granted to it and not to
//! iyw-claw — where every agent's shell would inherit them. That makes the helper
//! the thing an agent would most like to drive itself: any process can launch
//! a copy of it the same way iyw-claw does. So **before it reads a byte, the
//! helper checks that the process on the other end of its stdin is iyw-claw** —
//! by the audit token the kernel attached to the socket, against iyw-claw's
//! designated requirement compiled into this binary — and exits, having done
//! nothing, if it is not.
//!
//! The requirement is compiled in (`IYW_CLAW_COMPUTER_PEER_REQUIREMENT`, set by
//! the release build) rather than read from anywhere at run time: the app
//! bundle both binaries ship in is owned by the user and writable by any of
//! their processes. A build without it is a development build. Development
//! builds skip the peer check and say so in their first frame — tolerable only
//! because such a build is ad-hoc signed, so the permissions granted to it are
//! keyed to that one build's cdhash. **A helper that carries a Team ID and no
//! requirement refuses to start**: that would be a helper matching the release
//! signing identity with no check in front of it, a standing key to whatever
//! the user granted.
//!
//! On Windows and Linux there is no TCC to guard and no code signature to
//! check; the helper serves its stdin, which is the pipe iyw-claw gave it.

pub mod act;
#[cfg(target_os = "macos")]
pub mod axwin;
pub mod clipboard;
pub mod driver_proc;
#[cfg(windows)]
pub mod hwnd;
pub mod keystate;
pub mod mcp;
pub mod ops;
pub mod screen;
pub mod session;
pub mod tree;
#[cfg(all(target_os = "linux", feature = "computer-helper"))]
pub mod x11win;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, Mutex};

use self::act::SnapshotBook;
use self::driver_proc::DriverProc;
use self::ops::AppCache;
use super::driver;
use super::protocol::{
    read_frame, HelperError, HelperErrorCode, HelperMessage, HelperOp, HelperReady, HelperReply,
    HelperRequest, OsPermission, PeerCheck, PermissionReport, ProcessRun, RawAct, RawApp,
    RawWindow, MAX_FRAME_BYTES, PROTOCOL_VERSION, SOURCE_FINGERPRINT, STOP_ALL,
};

#[path = "parts/mod/EXIT_OK_group_1.rs"]
mod part_1;
pub use part_1::*;

#[path = "parts/mod/serve_on_own_runtime_group_2.rs"]
mod part_2;
use part_2::*;

#[path = "parts/mod/into_tokio_group_3.rs"]
mod part_3;

#[path = "parts/mod/Channel_group_4.rs"]
mod part_4;
use part_4::*;

#[path = "parts/mod/PeerGuard_group_5.rs"]
mod part_5;
pub use part_5::*;

#[path = "parts/mod/still_peer_group_6.rs"]
mod part_6;

#[cfg(target_os = "macos")]
#[path = "parts/mod/open_channel_group_7.rs"]
mod part_7;
#[cfg(target_os = "macos")]
use part_7::*;

#[cfg(not(target_os = "macos"))]
#[path = "parts/mod/open_channel_group_8.rs"]
mod part_8;
#[cfg(not(target_os = "macos"))]
use part_8::*;

#[cfg(target_os = "macos")]
#[path = "parts/mod/fstat_group_9.rs"]
mod part_9;
#[cfg(target_os = "macos")]
use part_9::*;

#[cfg(any(test, target_os = "macos"))]
#[path = "parts/mod/RECHECK_MISSING_group_10.rs"]
mod part_10;
#[cfg(any(test, target_os = "macos"))]
use part_10::*;

#[path = "parts/mod/RunningDriver_group_11.rs"]
mod part_11;
use part_11::*;

#[path = "parts/mod/snapshots_group_12.rs"]
mod part_12;
#[path = "parts/mod/list_windows_group_13.rs"]
mod part_13;

#[path = "parts/mod/Delivery_group_14.rs"]
mod part_14;
pub use part_14::*;

#[path = "parts/mod/check_group_15.rs"]
mod part_15;

#[path = "parts/mod/screen_offered_group_16.rs"]
mod part_16;
use part_16::*;

#[cfg(any(test, target_os = "macos"))]
#[path = "parts/mod/answer_stands_group_17.rs"]
mod part_17;
#[cfg(any(test, target_os = "macos"))]
use part_17::*;

#[path = "parts/mod/gained_group_18.rs"]
mod part_18;
use part_18::*;

#[path = "parts/mod/handle_op_group_19.rs"]
mod part_19;
use part_19::*;

#[path = "parts/mod/encode_group_20.rs"]
mod part_20;
use part_20::*;

#[path = "parts/mod/serve_group_21.rs"]
mod part_21;
pub use part_21::*;

#[path = "parts/mod/control_ops.rs"]
mod control_ops;
use control_ops::*;
#[path = "parts/mod/reads_ops.rs"]
mod reads_ops;
use reads_ops::*;
#[path = "parts/mod/actions_ops.rs"]
mod actions_ops;
use actions_ops::*;
