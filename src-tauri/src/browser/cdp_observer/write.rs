use std::time::Duration;

use futures_util::SinkExt;
use tokio_tungstenite::tungstenite::Message;

pub(super) async fn send_with_timeout<S>(
    sink: &mut S,
    message: Message,
    timeout: Duration,
) -> Result<(), ()>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    tokio::time::timeout(timeout, sink.send(message))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())
}
