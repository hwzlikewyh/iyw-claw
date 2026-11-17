//! Response item ID generation shared by sessions and their consumers.

use crate::ResponseItemId;
use std::sync::Arc;

/// Generates response item IDs from a type prefix for an owning session. Clones share the same callable.
pub type ItemIdGenerator = Arc<dyn Fn(&str) -> ResponseItemId + Send + Sync>;
