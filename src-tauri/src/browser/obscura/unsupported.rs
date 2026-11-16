use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub(super) const SUPPORTED: bool = false;

pub(super) fn run(_: mpsc::Receiver<super::worker::Request>, _: CancellationToken) {}
