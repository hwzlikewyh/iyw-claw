use std::time::Duration;

use serde_json::Value;

use super::{ChatChannelError, Frame, FrameHeader, FRAME_METHOD_CONTROL, WS_PING_INTERVAL};

pub(super) struct Endpoint {
    pub url: String,
    pub ping: Duration,
    pub service: i32,
}

impl Endpoint {
    pub fn new(url: String, config: &Value) -> Result<Self, ChatChannelError> {
        let parsed = reqwest::Url::parse(&url)
            .map_err(|_| ChatChannelError::ConnectionFailed("Invalid Lark endpoint".into()))?;
        let service = parsed
            .query_pairs()
            .find(|(key, _)| key == "service_id")
            .and_then(|(_, value)| value.parse().ok())
            .ok_or_else(|| {
                ChatChannelError::ConnectionFailed("Lark endpoint omitted service_id".into())
            })?;
        Ok(Self {
            url,
            service,
            ping: interval(config).unwrap_or(WS_PING_INTERVAL),
        })
    }

    pub fn idle_timeout(&self) -> Duration {
        self.ping.saturating_mul(2) + Duration::from_secs(5)
    }

    pub fn ping_frame(&self) -> Frame {
        Frame {
            seq_id: 0,
            log_id: 0,
            service: self.service,
            method: FRAME_METHOD_CONTROL,
            headers: vec![FrameHeader {
                key: "type".into(),
                value: "ping".into(),
            }],
            payload_encoding: String::new(),
            payload_type: String::new(),
            payload: Vec::new(),
            log_id_new: String::new(),
        }
    }
}

pub(super) fn interval(value: &Value) -> Option<Duration> {
    let seconds = value["PingInterval"].as_u64()?;
    (seconds > 0 && seconds <= 3600).then(|| Duration::from_secs(seconds))
}
