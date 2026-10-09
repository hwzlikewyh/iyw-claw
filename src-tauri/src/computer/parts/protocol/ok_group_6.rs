// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl HelperReply {
    pub fn ok(id: u64, value: impl Serialize) -> Self {
        match serde_json::to_value(value) {
            Ok(value) => Self {
                id,
                ok: Some(value),
                error: None,
            },
            Err(e) => Self::error(id, HelperError::failed(format!("encode: {e}"))),
        }
    }

    pub fn error(id: u64, error: HelperError) -> Self {
        Self {
            id,
            ok: None,
            error: Some(error),
        }
    }

    /// The answer as the type the op promises, or the helper's error.
    pub fn decode<T: DeserializeOwned>(self) -> Result<T, HelperError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let value = self.ok.unwrap_or(Value::Null);
        serde_json::from_value(value).map_err(|e| {
            HelperError::failed(format!("the helper answered in an unexpected shape: {e}"))
        })
    }
}
