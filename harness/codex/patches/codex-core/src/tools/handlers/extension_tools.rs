use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::Weak;

use codex_protocol::items::TurnItem;
use codex_protocol::protocol::Event;
use codex_protocol::protocol::EventMsg;
use codex_tools::ConversationHistory;
use codex_tools::ExtensionTurnItem;
use codex_tools::ResponsesApiNamespaceTool;
use codex_tools::ToolCall as ExtensionToolCall;
use codex_tools::ToolEnvironment;
use codex_tools::ToolName;
use codex_tools::ToolSearchInfo;
use codex_tools::ToolSpec;
use codex_tools::TurnItemEmissionFuture;
use codex_tools::TurnItemEmitter;
use codex_utils_string::to_ascii_json_string;

use crate::sandboxing::SandboxPermissions;
use crate::session::session::Session;
use crate::session::turn_context::TurnContext;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::apply_granted_turn_permissions;
use crate::tools::lifecycle::extension_tool_call_source;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;
use crate::turn_metadata::McpTurnMetadataContext;

pub(crate) struct ExtensionToolAdapter(
    Arc<dyn for<'call> codex_tools::ToolExecutor<ExtensionToolCall<'call>>>,
);

impl ExtensionToolAdapter {
    pub(crate) fn new(
        executor: Arc<dyn for<'call> codex_tools::ToolExecutor<ExtensionToolCall<'call>>>,
    ) -> Self {
        Self(executor)
    }
}

impl ToolExecutor<ToolInvocation> for ExtensionToolAdapter {
    fn tool_name(&self) -> ToolName {
        self.0.tool_name()
    }

    fn spec(&self) -> ToolSpec {
        self.0.spec()
    }

    fn exposure(&self) -> crate::tools::registry::ToolExposure {
        self.0.exposure()
    }

    fn supports_parallel_tool_calls(&self) -> bool {
        self.0.supports_parallel_tool_calls()
    }

    fn search_info(&self) -> Option<ToolSearchInfo> {
        self.0.search_info()
    }

    fn handle<'a>(&'a self, invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'a>
    where
        ToolInvocation: 'a,
    {
        Box::pin(async move { self.0.handle(to_extension_call(&invocation).await).await })
    }
}

impl CoreToolRuntime for ExtensionToolAdapter {
    fn is_builtin_control_tool(&self) -> bool {
        let tool_name = self.0.tool_name();
        if tool_name.is_default_namespace() {
            return matches!(
                tool_name.name.as_str(),
                "get_goal" | "create_goal" | "update_goal"
            );
        }
        matches!(
            (tool_name.namespace.as_deref(), tool_name.name.as_str()),
            (
                Some("notes"),
                "list_files_by_prefix"
                    | "read_file"
                    | "search_contents"
                    | "append_to_file"
                    | "write_file"
            ) | (
                Some("history"),
                "list_windows" | "list_items" | "read_item" | "search_contents"
            )
        )
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        match payload {
            ToolPayload::Function { .. } => true,
            ToolPayload::Custom { .. } => match self.0.spec() {
                ToolSpec::Freeform(_) => true,
                ToolSpec::Namespace(namespace) => namespace.tools.iter().any(|tool| {
                    matches!(
                        tool,
                        ResponsesApiNamespaceTool::Custom(tool)
                            if tool.name == self.0.tool_name().name
                    )
                }),
                ToolSpec::Function(_)
                | ToolSpec::ToolSearch { .. }
                | ToolSpec::WebSearch { .. } => false,
            },
            ToolPayload::ToolSearch { .. } => false,
        }
    }
}

struct CoreTurnItemEmitter {
    session: Weak<Session>,
    turn: Weak<TurnContext>,
}

async fn emit_legacy_events(session: &Session, turn: &TurnContext, legacy_events: Vec<EventMsg>) {
    for msg in legacy_events {
        session
            .send_event_raw(Event {
                id: turn.sub_id.clone(),
                msg,
            })
            .await;
    }
}

impl TurnItemEmitter for CoreTurnItemEmitter {
    fn emit_started<'a>(&'a self, item: ExtensionTurnItem) -> TurnItemEmissionFuture<'a> {
        Box::pin(async move {
            let (Some(session), Some(turn)) = (self.session.upgrade(), self.turn.upgrade()) else {
                return;
            };
            let ExtensionTurnItem {
                item,
                legacy_events,
            } = item;
            let item = TurnItem::Extension(item);
            session.emit_turn_item_started(turn.as_ref(), &item).await;
            emit_legacy_events(session.as_ref(), turn.as_ref(), legacy_events).await;
        })
    }

    fn emit_completed<'a>(&'a self, item: ExtensionTurnItem) -> TurnItemEmissionFuture<'a> {
        Box::pin(async move {
            let (Some(session), Some(turn)) = (self.session.upgrade(), self.turn.upgrade()) else {
                return;
            };
            let ExtensionTurnItem {
                item,
                legacy_events,
            } = item;
            let item = TurnItem::Extension(item);
            session.emit_turn_item_completed(turn.as_ref(), item).await;
            emit_legacy_events(session.as_ref(), turn.as_ref(), legacy_events).await;
        })
    }
}

async fn to_extension_call(invocation: &ToolInvocation) -> ExtensionToolCall<'_> {
    let conversation_history =
        ConversationHistory::new(invocation.session.clone_history().await.into_raw_items());
    let codex_turn_metadata = invocation
        .turn
        .turn_metadata_state
        .current_meta_value_for_mcp_request(McpTurnMetadataContext {
            model: invocation.turn.model_info().slug.as_str(),
            reasoning_effort: invocation.turn.effective_reasoning_effort(),
            node_repl_disabled: invocation.turn.model_info().node_repl_disabled,
        })
        .and_then(|metadata| to_ascii_json_string(&metadata).ok());
    let mut environments = Vec::new();
    for environment in invocation.step_context.environments.turn_environments() {
        // TODO(anp): Migrate extension ToolEnvironment and granted-permission lookup to PathUri
        // so extensions can receive foreign environment cwd values.
        let Ok(native_cwd) = environment.cwd().to_abs_path() else {
            continue;
        };
        let additional_permissions = apply_granted_turn_permissions(
            invocation.session.as_ref(),
            environment,
            environment.cwd(),
            SandboxPermissions::UseDefault,
            /*additional_permissions*/ None,
        )
        .await
        .additional_permissions;
        let file_system_sandbox_context = environment.sandbox_context(additional_permissions);
        environments.push(ToolEnvironment {
            _lifetime: PhantomData,
            environment_id: environment.selection.environment_id.clone(),
            cwd: native_cwd,
            file_system: environment.environment.get_filesystem(),
            file_system_sandbox_context,
        });
    }
    ExtensionToolCall {
        turn_id: invocation.turn.sub_id.clone(),
        call_id: invocation.call_id.clone(),
        tool_name: invocation.tool_name.clone(),
        model: invocation.turn.model_info().slug.clone(),
        codex_turn_metadata,
        truncation_policy: invocation.turn.model_info().truncation_policy.into(),
        source: extension_tool_call_source(invocation.source.clone()),
        conversation_history,
        turn_item_emitter: Arc::new(CoreTurnItemEmitter {
            session: Arc::downgrade(&invocation.session),
            turn: Arc::downgrade(&invocation.turn),
        }),
        environments,
        payload: invocation.payload.clone(),
    }
}
