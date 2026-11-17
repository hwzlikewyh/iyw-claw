use clap::Args;
use handler_common::{HandlerError, OutputFormat};

/// Swap two elements in the document
#[derive(Args)]
pub struct SwapCommand {
    /// Document file path
    pub file: String,

    /// DOM path of the first element
    pub path1: String,

    /// DOM path of the second element
    pub path2: String,
}

pub fn handle_swap(cmd: SwapCommand, _format: OutputFormat) -> Result<String, HandlerError> {
    let handler = crate::open_handler(&cmd.file, true)?;
    let (p1, p2) = handler.swap(&cmd.path1, &cmd.path2)?;
    handler.save()?;
    Ok(format!("Swapped {} <-> {}", p1, p2))
}
