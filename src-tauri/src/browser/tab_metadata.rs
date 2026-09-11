use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::command_runner::AgentBrowserCli;
use super::error::{BrowserError, BrowserErrorCode};
use super::runtime::unavailable_error;

const COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub(super) async fn page_metadata(
    cli: &AgentBrowserCli,
    session: &str,
    cdp_url: &str,
    response: &Value,
    cancellation: CancellationToken,
) -> Result<(String, String), BrowserError> {
    ensure_not_cancelled(&cancellation)?;
    let data = response_data(response);
    let title = data.get("title").and_then(Value::as_str);
    let url = data.get("url").and_then(Value::as_str);
    let fetch = |field| {
        let cancellation = cancellation.clone();
        async move {
            cli.run_pinned(
                session,
                cdp_url,
                &["get", field],
                COMMAND_TIMEOUT,
                cancellation,
            )
            .await
        }
    };
    let url = match url {
        Some(url) => url.to_string(),
        None => {
            let response = fetch("url").await?;
            response_data(&response)
                .get("url")
                .and_then(Value::as_str)
                .ok_or_else(unavailable_error)?
                .to_string()
        }
    };
    let title = match title {
        Some(title) => bounded_title(title),
        None => {
            let response = fetch("title").await?;
            bounded_title(
                response_data(&response)
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
        }
    };
    Ok((title, url))
}

fn ensure_not_cancelled(cancellation: &CancellationToken) -> Result<(), BrowserError> {
    if cancellation.is_cancelled() {
        return Err(BrowserError::new(
            BrowserErrorCode::BrowserCancelled,
            "The browser operation was cancelled",
        ));
    }
    Ok(())
}

pub(super) fn response_data(response: &Value) -> &Value {
    response.get("data").unwrap_or(response)
}

fn bounded_title(title: &str) -> String {
    title.chars().take(512).collect()
}
