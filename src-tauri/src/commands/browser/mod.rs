use std::future::Future;
use std::pin::Pin;

use crate::browser::BrowserError;

mod cdp;
mod control;
mod runtime;
mod streams;
mod tabs;
mod views;
mod window_close;

pub(super) type BrowserCommandFuture<T> =
    Pin<Box<dyn Future<Output = Result<T, BrowserError>> + Send + 'static>>;

pub(super) fn browser_command<T>(
    future: impl Future<Output = Result<T, BrowserError>> + Send + 'static,
) -> BrowserCommandFuture<T> {
    Box::pin(future)
}

pub use cdp::*;
pub use control::*;
pub use runtime::*;
pub use streams::*;
pub use tabs::*;
pub use views::*;
