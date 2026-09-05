//! Stable routing policy for Codex App Server methods.

use std::fmt;

use crate::{Capability, RequestClass};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MethodScope {
    Global,
    Session,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TurnScope {
    None,
    Optional,
    Required,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MethodPolicy {
    pub scope: MethodScope,
    pub class: RequestClass,
    pub capability: Option<Capability>,
    pub turn: TurnScope,
}

impl MethodPolicy {
    const fn global(class: RequestClass, capability: Option<Capability>) -> Self {
        Self {
            scope: MethodScope::Global,
            class,
            capability,
            turn: TurnScope::None,
        }
    }

    const fn session(class: RequestClass, capability: Option<Capability>, turn: TurnScope) -> Self {
        Self {
            scope: MethodScope::Session,
            class,
            capability,
            turn,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UnsupportedMethod(pub String);

impl fmt::Display for UnsupportedMethod {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Codex App Server method is not enabled: {}",
            self.0
        )
    }
}

impl std::error::Error for UnsupportedMethod {}

pub fn client_method_policy(method: &str) -> Result<MethodPolicy, UnsupportedMethod> {
    let read_only = MethodPolicy::global(RequestClass::ReadOnly, None);
    let policy = match method {
        "model/list"
        | "modelProvider/capabilities/read"
        | "collaborationMode/list"
        | "permissionProfile/list"
        | "account/read"
        | "config/read"
        | "mcpServerStatus/list" => read_only,
        "skills/list" => global_capability(RequestClass::ReadOnly, Capability::Skills),
        "thread/read" => session_read_only(),
        "thread/goal/get" => session_capability(RequestClass::ReadOnly, Capability::Goals),
        "thread/goal/set" | "thread/goal/clear" => {
            session_capability(RequestClass::Configuration, Capability::Goals)
        }
        "thread/settings/update" => {
            session_capability(RequestClass::Configuration, Capability::Configuration)
        }
        "turn/start" => session_turn(RequestClass::Prompt, Capability::Prompt, TurnScope::None),
        "turn/steer" => session_turn(
            RequestClass::Prompt,
            Capability::Steering,
            TurnScope::Required,
        ),
        "turn/interrupt" => session_turn(
            RequestClass::Cancellation,
            Capability::Cancellation,
            TurnScope::Required,
        ),
        _ => return Err(UnsupportedMethod(method.to_string())),
    };
    Ok(policy)
}

pub fn server_method_policy(method: &str) -> Result<MethodPolicy, UnsupportedMethod> {
    let approval = |turn| {
        MethodPolicy::session(
            RequestClass::PermissionResponse,
            Some(Capability::Permission),
            turn,
        )
    };
    let policy = match method {
        "item/commandExecution/requestApproval"
        | "item/fileChange/requestApproval"
        | "item/tool/requestUserInput"
        | "item/permissions/requestApproval" => approval(TurnScope::Required),
        "mcpServer/elicitation/request" => MethodPolicy::session(
            RequestClass::PermissionResponse,
            Some(Capability::Mcp),
            TurnScope::Optional,
        ),
        "item/tool/call" => MethodPolicy::session(
            RequestClass::Prompt,
            Some(Capability::Mcp),
            TurnScope::Required,
        ),
        "applyPatchApproval" | "execCommandApproval" => approval(TurnScope::None),
        "account/chatgptAuthTokens/refresh" => {
            MethodPolicy::global(RequestClass::Configuration, Some(Capability::Configuration))
        }
        "currentTime/read" => MethodPolicy::global(RequestClass::ReadOnly, None),
        _ => return Err(UnsupportedMethod(method.to_string())),
    };
    Ok(policy)
}

fn global_capability(class: RequestClass, capability: Capability) -> MethodPolicy {
    MethodPolicy::global(class, Some(capability))
}

fn session_read_only() -> MethodPolicy {
    MethodPolicy::session(RequestClass::ReadOnly, None, TurnScope::None)
}

fn session_capability(class: RequestClass, capability: Capability) -> MethodPolicy {
    MethodPolicy::session(class, Some(capability), TurnScope::None)
}

fn session_turn(class: RequestClass, capability: Capability, turn: TurnScope) -> MethodPolicy {
    MethodPolicy::session(class, Some(capability), turn)
}
