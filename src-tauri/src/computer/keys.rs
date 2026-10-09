//! Keys an agent may press on a shared window, and which of them a window
//! grant allows.
//!
//! A window grant is permission to work *in* one window. A key reaches the
//! application, though, not the window: ⌘Q quits all of it, ⌘W closes the
//! window, and the platforms' own chords (⌘Tab, Alt+Tab, Win+L, ⌃⌘Q) act on
//! the whole desktop. So what a window grant lets through is a closed list —
//! the editing and navigation keys and the few chords that stay inside a text
//! field (select all, copy, cut, undo, redo, find, moving by word or line) —
//! and everything else is refused as needing more than one window.
//!
//! An application shared as a whole reaches further: every chord it takes —
//! its menu commands, ⌘W and ⌘Q among them — but still not the desktop's
//! own (switching applications, the launcher, the screenshot keys, locking
//! the screen or logging out, forcing applications to quit, moving between
//! desktops), which reach past any one application ([`classify_for_app`]).
//!
//! **Paste is its own case.** ⌘V writes the clipboard into a window the agent
//! can read, and the clipboard is the user's: what they last copied from a
//! password manager is exactly what would come back in the next snapshot. A
//! paste is safe only when the clipboard holds what the agent itself copied
//! out of a window it may read, which this version does not track — so every
//! paste chord is refused.
//!
//! The vocabulary is also closed, and spelled once here: the driver's key
//! names differ by platform (its Windows build even reads an unknown name as
//! its first letter — `printscreen` is P), so an agent's key is parsed into
//! [`Key`] and written back out in the one spelling that platform's driver
//! reads.

use serde::{Deserialize, Serialize};

#[path = "parts/keys/Key_group_1.rs"]
mod part_1;
pub use part_1::*;

#[path = "parts/keys/PUNCTUATION_group_2.rs"]
mod part_2;
use part_2::*;

#[path = "parts/keys/parse_group_3.rs"]
mod part_3;

#[path = "parts/keys/Modifiers_group_4.rs"]
mod part_4;
pub use part_4::*;

#[path = "parts/keys/parse_group_5.rs"]
mod part_5;

#[path = "parts/keys/Chord_group_6.rs"]
mod part_6;
pub use part_6::*;

#[path = "parts/keys/types_text_group_7.rs"]
mod part_7;

#[path = "parts/keys/Platform_group_8.rs"]
mod part_8;
pub use part_8::*;

#[path = "parts/keys/current_group_9.rs"]
mod part_9;

#[path = "parts/keys/ChordClass_group_10.rs"]
mod part_10;
pub use part_10::*;

#[path = "parts/keys/desktop_chord_group_11.rs"]
mod part_11;
use part_11::*;

#[path = "parts/keys/classify_for_screen_group_12.rs"]
mod part_12;
pub use part_12::*;

#[path = "parts/keys/never_chord_group_13.rs"]
mod part_13;
use part_13::*;

#[path = "parts/keys/pointer_modifiers_allowed_for_app_group_14.rs"]
mod part_14;
pub use part_14::*;

#[path = "parts/keys/COPY_WORDS_group_15.rs"]
mod part_15;
use part_15::*;

#[path = "parts/keys/names_copy_group_16.rs"]
mod part_16;
pub use part_16::*;

#[path = "parts/keys/PASTE_WORDS_group_17.rs"]
mod part_17;
use part_17::*;

#[path = "parts/keys/names_paste_group_18.rs"]
mod part_18;
pub use part_18::*;
