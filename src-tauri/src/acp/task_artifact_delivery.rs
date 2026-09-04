use std::io;

mod path;
use path::{create_managed_directory, turn_directory, TurnIdentity};

/// Creates and returns the host-owned directory used to materialize local MCP
/// artifacts for one assistant turn.
pub(crate) async fn ensure_managed_turn_directory(
    connection_id: &str,
    conversation_id: i32,
    turn_generation: i64,
) -> io::Result<std::path::PathBuf> {
    let identity = TurnIdentity::new(connection_id, conversation_id, turn_generation);
    create_managed_directory(&identity).await?;
    turn_directory(&identity)
}
