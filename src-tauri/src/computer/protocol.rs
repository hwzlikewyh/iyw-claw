//! Frames between iyw-claw and `iyw-computer-helper`.
//!
//! One private channel per helper: the socketpair iyw-claw created and handed to
//! the helper as its stdin/stdout on macOS, plain pipes elsewhere. Frames are
//! the broker's length-prefixed JSON ([`write_frame`] / [`read_frame`]), with
//! the broker's 16 MiB cap — which is also the most the broker can hand an
//! agent in one answer, so a capture too large for this channel could not
//! have been delivered anyway.
//!
//! **The helper speaks first**, with [`HelperMessage::Ready`]. That ordering is
//! load-bearing on macOS: iyw-claw created both ends of the socketpair, so until
//! the helper has written to its end the kernel still reports iyw-claw itself as
//! the peer, and a signature check of "the peer" would check iyw-claw. iyw-claw
//! verifies the helper only after the first frame arrives.
//!
//! The ops are a closed list. The driver behind the helper advertises dozens
//! of tools — launching and killing applications, rewriting its own
//! configuration, replaying recorded input — and none of them is reachable
//! from here: the helper translates each op below into fixed driver calls,
//! and there is no op that carries a tool name. The one op that changes a
//! window, [`HelperOp::Act`], carries a closed [`WindowAction`] whose every
//! field the helper rebuilds into the driver's arguments itself — save
//! [`WindowAction::Restore`], which on macOS and Windows the helper carries
//! out itself, on that one window, since the driver has no call that leaves
//! it in the background.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use crate::acp::delegation::transport::{read_frame, write_frame, MAX_FRAME_BYTES};

#[path = "parts/protocol/no_modifiers_group_1.rs"]
mod part_1;
use part_1::*;

use super::keys::{Chord, Modifiers};
use super::types::{
    ActDelivery, ActEffect, ActRoute, PointerButton, PredicateResult, Rect, ScrollDirection,
    ScrollUnit, VerifyRequest, VerifyStatus,
};

#[path = "parts/protocol/PROTOCOL_VERSION_group_2.rs"]
mod part_2;
pub use part_2::*;

#[path = "parts/protocol/WindowAction_group_3.rs"]
mod part_3;
pub use part_3::*;

#[path = "parts/protocol/element_group_4.rs"]
mod part_4;

#[path = "parts/protocol/RawAct_group_5.rs"]
mod part_5;
pub use part_5::*;

#[path = "parts/protocol/ok_group_6.rs"]
mod part_6;

#[path = "parts/protocol/OsPermission_group_7.rs"]
mod part_7;
pub use part_7::*;

#[path = "parts/protocol/arg_group_8.rs"]
mod part_8;

#[path = "parts/protocol/PermissionReport_group_9.rs"]
mod part_9;
pub use part_9::*;

#[path = "parts/protocol/has_group_10.rs"]
mod part_10;

#[path = "parts/protocol/RawApp_group_11.rs"]
mod part_11;
pub use part_11::*;

#[path = "parts/protocol/key_group_12.rs"]
mod part_12;

#[path = "parts/protocol/InstalledApp_group_13.rs"]
mod part_13;
pub use part_13::*;

#[path = "parts/protocol/new_group_14.rs"]
mod part_14;
