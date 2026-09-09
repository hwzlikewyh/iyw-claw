use serde_json::{json, Value};
use std::sync::OnceLock;

use crate::acp::interactive_html::{MAX_HTML_BYTES, MAX_TITLE_CHARS};

pub(super) const ASK_TOOL: &str = "ask_user_question";
pub(super) const HTML_TOOL: &str = "show_interactive_html";

pub(super) fn embedded_tool(name: &str) -> Value {
    static EMBEDDED_TOOLS: OnceLock<Value> = OnceLock::new();
    let tools = EMBEDDED_TOOLS.get_or_init(|| {
        serde_json::from_str(crate::acp::delegation::companion::TOOL_SCHEMA_JSON)
            .expect("embedded tool schema must be valid JSON")
    });
    tools
        .as_array()
        .and_then(|items| items.iter().find(|tool| tool["name"] == name))
        .expect("embedded direct tool must exist")
        .clone()
}

pub(super) fn html_tool() -> Value {
    json!({
        "name": HTML_TOOL,
        "description": "Proactively create and automatically display a freely designed interactive HTML page when seeing, manipulating or experimenting helps the user understand, explore, compare, express preferences or decide. You control the HTML, CSS, JavaScript, SVG, Canvas, layout, visual design, interaction logic and returned JSON data; there is no fixed form or component schema. Examples include interactive explanations, simulations, visual comparisons, design previews, drag-and-drop ordering, annotations, configurable charts and custom mini-tools; these are inspiration, not an exhaustive list. The user does not need to explicitly request HTML. For a concise question or a few choices, prefer ask_user_question. Supply a complete self-contained document with inline scripts/styles and embedded assets; it loads automatically in the current conversation. By default return immediately after presenting so you can continue working. Set wait_for_response=true only when you need the user's result before continuing, and call await iyw.submit(data) from an explicit user action in the page. data may be any JSON value (up to 64 KiB); handle submission errors and keep the user's draft. Do not submit on load. Display-only pages cannot submit feedback. The page runs in a sandbox without host credentials, filesystem access or tool calls. At most eight pages may remain open in one session. A presented result means the host accepted the page, not that browser rendering has been verified.",
        "inputSchema": {
            "type": "object",
            "required": ["title", "html"],
            "properties": {
                "title": {"type": "string", "minLength": 1, "maxLength": MAX_TITLE_CHARS},
                "html": {"type": "string", "minLength": 1, "maxLength": MAX_HTML_BYTES,
                    "description": "Complete HTML document, at most 256 KiB UTF-8. Design freely using inline CSS/JS, SVG, Canvas and embedded assets. Include accessible controls and responsive layout. The host injects iyw.submit(data); no SDK or local server is needed."},
                "wait_for_response": {"type": "boolean", "default": false,
                    "description": "False for an explanation, visualization or exploratory mini-tool: present and continue. True for design feedback, selections, configuration or other user data needed to continue: wait for page submission or cancellation."}
            },
            "additionalProperties": false
        }
    })
}
