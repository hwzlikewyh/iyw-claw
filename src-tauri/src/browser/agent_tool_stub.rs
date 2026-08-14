use super::manager::BrowserSessionManager;
use super::types::BrowserAgentToolCall;

impl BrowserSessionManager {
    pub async fn execute_agent_tool(&self, _call: BrowserAgentToolCall) -> serde_json::Value {
        serde_json::json!({
            "error": {
                "code": "BROWSER_UNSUPPORTED_RUNTIME",
                "message": "The shared browser requires the desktop runtime.",
                "retryable": false,
                "effectMayHaveOccurred": false,
            }
        })
    }
}
