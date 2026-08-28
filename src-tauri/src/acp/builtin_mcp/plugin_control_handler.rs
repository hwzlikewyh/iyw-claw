use rmcp::model::CallToolResult;
use rmcp::ErrorData;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use super::authority::SessionContext;
use super::capability::ResolvedCapability;
use super::delivery::RelayDelivery;
use super::handler::BuiltinMcpHandler;
use super::invocation::{ensure_active, execute_invocation, InvocationContext};
use super::plugin_control::{PluginControlRequest, PluginEnableRequest, PluginInstallRequest};
use super::plugin_control_support::{
    approve_scope, approved, control_result, installed_plugin, load_install_candidate,
    permission_summary, plugin_database,
};

struct PluginApproval {
    authority: SessionContext,
    delivery: Option<RelayDelivery>,
    request_id: Value,
    request_cancel: CancellationToken,
    question_id: &'static str,
    header: &'static str,
    question: String,
    approve_label: &'static str,
}

impl BuiltinMcpHandler {
    pub(super) async fn control_plugin(
        &self,
        authority: SessionContext,
        delivery: Option<RelayDelivery>,
        request: PluginControlRequest,
        request_id: Value,
        request_cancel: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        match request {
            PluginControlRequest::Install(request) => {
                self.install_plugin(authority, delivery, request, request_id, request_cancel)
                    .await
            }
            PluginControlRequest::Enable(request) => {
                self.enable_plugin(authority, delivery, request, request_id, request_cancel)
                    .await
            }
        }
    }

    async fn install_plugin(
        &self,
        authority: SessionContext,
        delivery: Option<RelayDelivery>,
        request: PluginInstallRequest,
        request_id: Value,
        request_cancel: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        ensure_active(&authority, &request_cancel)?;
        let database = plugin_database()?;
        let candidate = load_install_candidate(&database, &request).await?;
        if installed_plugin(&candidate.name).is_some() {
            return Err(ErrorData::invalid_params(
                "plugin is already installed; request workspace enable approval instead",
                Some(json!({"code": "plugin_already_installed"})),
            ));
        }
        let prompt = format!(
            "Install official plugin {} version {}? This downloads executable plugin code. Permissions: {}",
            candidate.name,
            candidate.version,
            permission_summary(&candidate.permissions)
        );
        let answer = self
            .ask_plugin_approval(PluginApproval {
                authority: authority.clone(),
                delivery,
                request_id,
                request_cancel: request_cancel.clone(),
                question_id: "plugin-install-approval",
                header: "Install plugin",
                question: prompt,
                approve_label: "Install",
            })
            .await?;
        if !approved(&answer, "Install") {
            return Ok(answer);
        }
        ensure_active(&authority, &request_cancel)?;
        crate::commands::skill_market::install_core(
            &database,
            request.skill_id,
            candidate.version,
            vec![authority.agent_type()],
        )
        .await
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
        Ok(control_result("installed"))
    }

    async fn enable_plugin(
        &self,
        authority: SessionContext,
        delivery: Option<RelayDelivery>,
        request: PluginEnableRequest,
        request_id: Value,
        request_cancel: CancellationToken,
    ) -> Result<CallToolResult, ErrorData> {
        ensure_active(&authority, &request_cancel)?;
        let plugin = installed_plugin(&request.plugin_slug).ok_or_else(|| {
            ErrorData::invalid_params(
                "installed plugin is unavailable",
                Some(json!({"code": "plugin_unavailable"})),
            )
        })?;
        let permissions = plugin
            .manifest
            .permissions
            .as_ref()
            .ok_or_else(|| ErrorData::invalid_params("plugin permissions are missing", None))?;
        let prompt = format!(
            "Enable plugin {} version {} for this workspace and Agent? Permissions: {}",
            plugin.slug,
            plugin.version,
            permission_summary(permissions)
        );
        let answer = self
            .ask_plugin_approval(PluginApproval {
                authority: authority.clone(),
                delivery,
                request_id,
                request_cancel: request_cancel.clone(),
                question_id: "plugin-enable-approval",
                header: "Enable plugin",
                question: prompt,
                approve_label: "Enable",
            })
            .await?;
        if !approved(&answer, "Enable") {
            return Ok(answer);
        }
        ensure_active(&authority, &request_cancel)?;
        let database = plugin_database()?;
        approve_scope(&database, &plugin.slug, &authority).await?;
        Ok(control_result("enabled"))
    }

    async fn ask_plugin_approval(
        &self,
        approval: PluginApproval,
    ) -> Result<CallToolResult, ErrorData> {
        let ask = ResolvedCapability {
            tool_name: "ask_user_question".to_string(),
            arguments: json!({"questions": [{
                "id": approval.question_id,
                "header": approval.header,
                "question": approval.question,
                "options": [
                    {"label": approval.approve_label, "description": "Apply only to this workspace and Agent."},
                    {"label": "Cancel", "description": "Keep the plugin disabled."}
                ]
            }]}),
            delivery_ack: None,
        };
        execute_invocation(
            self.invocation_dependencies(),
            InvocationContext {
                authority: approval.authority,
                delivery: approval.delivery,
                invocation: ask,
                request_id: approval.request_id,
                request_cancel: approval.request_cancel,
            },
        )
        .await
    }
}
