use chrono::{DateTime, FixedOffset, SecondsFormat, Utc};
use tracing_subscriber::fmt::{format::Writer, time::FormatTime};

pub(crate) const OFFSET_SECONDS: i32 = 8 * 60 * 60;

pub(crate) fn offset() -> FixedOffset {
    FixedOffset::east_opt(OFFSET_SECONDS).expect("Beijing UTC offset is valid")
}

pub(crate) fn now() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&offset())
}

pub(super) struct BeijingTime;

impl FormatTime for BeijingTime {
    fn format_time(&self, writer: &mut Writer<'_>) -> std::fmt::Result {
        write!(
            writer,
            "{}",
            now().to_rfc3339_opts(SecondsFormat::Micros, false)
        )
    }
}
