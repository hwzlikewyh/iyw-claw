//! The driver's accessibility tree, read line by line: which lines start an
//! element, which elements can be acted on, and which are secrets.
//!
//! The driver writes values unescaped, newlines included, so a node is not a
//! line: it runs from a line that starts one (see [`Dialect::head`]) to the
//! next, and a secret's value goes with every line of it.
//!
//! Secrets are judged once, here, for two uses: the value of a secret field
//! never leaves the helper, and nothing is ever typed into one. The two
//! cannot disagree — a field the agent sees as `[redacted]` is the field it
//! cannot type into.

use std::collections::BTreeSet;

#[path = "parts/tree/SECRET_WORDS_group_1.rs"]
mod part_1;
use part_1::*;

#[path = "parts/tree/Redacted_group_2.rs"]
mod part_2;
pub use part_2::*;

#[path = "parts/tree/value_markers_group_3.rs"]
mod part_3;

#[path = "parts/tree/UIA_CONTROL_TYPES_group_4.rs"]
mod part_4;
use part_4::*;

#[path = "parts/tree/redact_tree_group_5.rs"]
mod part_5;
pub use part_5::*;

#[path = "parts/tree/push_node_group_6.rs"]
mod part_6;
use part_6::*;

#[path = "parts/tree/names_a_secret_group_7.rs"]
mod part_7;
pub use part_7::*;

#[path = "parts/tree/after_value_group_8.rs"]
mod part_8;
use part_8::*;
