#[path = "source_scopes.rs"]
mod scopes;
use crate::acp::runtime_task_receipts::{self, TaskReceipt};
use crate::db::service::{conversation_service, conversation_session_segment_service};
use crate::models::{AgentType, DbConversationDetail};
use sea_orm::DatabaseConnection;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct LedgerSource {
    pub requests: Vec<TaskReceipt>,
    pub messages: HashMap<String, String>,
    pub own: HashSet<String>,
    pub scopes: HashMap<String, HashSet<String>>,
    pub incomplete: bool,
    pub unscoped_descendants: bool,
    goal_turns: HashMap<String, String>,
    tool_goals: HashMap<String, String>,
    tool_parents: HashMap<String, String>,
}

#[derive(Clone)]
struct Actor {
    id: String,
    agent: AgentType,
    own: bool,
    scopes: Vec<String>,
}

pub(super) async fn collect(
    conn: &DatabaseConnection,
    detail: &DbConversationDetail,
) -> LedgerSource {
    let mut source = LedgerSource::default();
    let mut actors = root_actors(conn, detail, &mut source).await;
    add_delegations((conn, detail.summary.id), &mut actors, &mut source).await;
    expand_native(conn, &mut actors, &mut source).await;
    let ids: HashSet<_> = actors
        .iter()
        .filter(|actor| actor.agent == AgentType::Codex)
        .map(|actor| actor.id.clone())
        .collect();
    let files = tokio::task::spawn_blocking(move || {
        crate::parsers::codex::CodexParser::new().billing_receipts_batch(&ids)
    })
    .await
    .unwrap_or_default();
    source.incomplete |= files.values().any(|receipts| receipts.incomplete);
    source.goal_turns.extend(
        files
            .values()
            .flat_map(|receipts| receipts.turn_goals.clone()),
    );
    for receipts in files.values() {
        for (tool, turn) in &receipts.tool_turns {
            if let Some(goal) = source.goal_turns.get(turn) {
                source.tool_goals.insert(tool.clone(), goal.clone());
            }
        }
    }
    load_receipts(conn, (actors, files), &mut source).await;
    source
}

async fn root_actors(
    conn: &DatabaseConnection,
    detail: &DbConversationDetail,
    source: &mut LedgerSource,
) -> Vec<Actor> {
    let mut ids: HashSet<_> = detail.summary.external_id.iter().cloned().collect();
    match conversation_session_segment_service::list_for_conversation(conn, detail.summary.id).await
    {
        Ok(segments) => ids.extend(
            segments
                .into_iter()
                .filter_map(|segment| segment.external_id),
        ),
        Err(_) => source.incomplete = true,
    }
    ids.into_iter()
        .map(|id| Actor {
            id,
            agent: detail.summary.agent_type,
            own: true,
            scopes: Vec::new(),
        })
        .collect()
}

async fn add_delegations(
    context: (&DatabaseConnection, i32),
    actors: &mut Vec<Actor>,
    source: &mut LedgerSource,
) {
    let (conn, root) = context;
    let mut pending = vec![(root, Vec::<String>::new())];
    let mut seen = HashSet::new();
    while let Some((parent, scopes)) = pending.pop() {
        if !seen.insert(parent) {
            continue;
        }
        let children = match conversation_service::list_children(conn, parent).await {
            Ok(children) => children,
            Err(_) => {
                source.incomplete = true;
                continue;
            }
        };
        for child in children {
            let mut scopes = scopes.clone();
            if let Some(tool) = &child.parent_tool_use_id {
                scopes.push(format!("delegation:{tool}"));
            }
            pending.push((child.id, scopes.clone()));
            actors.extend(delegated_actors(conn, (child, scopes), source).await);
        }
    }
}

async fn delegated_actors(
    conn: &DatabaseConnection,
    input: (crate::models::DbConversationSummary, Vec<String>),
    source: &mut LedgerSource,
) -> Vec<Actor> {
    let (child, scopes) = input;
    let mut ids: HashSet<_> = child.external_id.into_iter().collect();
    match conversation_session_segment_service::list_for_conversation(conn, child.id).await {
        Ok(segments) => ids.extend(
            segments
                .into_iter()
                .filter_map(|segment| segment.external_id),
        ),
        Err(_) => source.incomplete = true,
    }
    source.incomplete |= ids.is_empty();
    ids.into_iter()
        .map(|id| Actor {
            id,
            agent: child.agent_type,
            own: false,
            scopes: scopes.clone(),
        })
        .collect()
}

async fn expand_native(
    conn: &DatabaseConnection,
    actors: &mut Vec<Actor>,
    source: &mut LedgerSource,
) {
    let mut seen = HashSet::new();
    let mut index = 0;
    while index < actors.len() {
        let actor = actors[index].clone();
        index += 1;
        if !seen.insert((actor.agent, actor.id.clone())) {
            continue;
        }
        let Some(snapshot) = runtime_task_receipts::load_tasks(conn, actor.agent, &actor.id).await
        else {
            source.incomplete = true;
            continue;
        };
        source.incomplete |= snapshot.billing_incomplete;
        if actor.agent != AgentType::Codex {
            continue;
        }
        for id in snapshot.billing_children.keys() {
            if *id == actor.id || seen.contains(&(actor.agent, id.clone())) {
                continue;
            }
            let mut scopes = actor.scopes.clone();
            scopes.extend(native_scopes(id, &snapshot.billing_children));
            actors.push(Actor {
                id: id.clone(),
                agent: actor.agent,
                own: false,
                scopes,
            });
        }
    }
}

fn native_scopes(
    id: &str,
    parents: &std::collections::BTreeMap<String, Option<String>>,
) -> Vec<String> {
    let mut scopes = vec![format!("subagent:{id}")];
    let mut parent = parents.get(id).and_then(|parent| parent.as_deref());
    let mut seen = HashSet::new();
    while let Some(id) = parent {
        if !seen.insert(id) {
            break;
        }
        if !parents.contains_key(id) {
            break;
        }
        scopes.push(format!("subagent:{id}"));
        parent = parents.get(id).and_then(|parent| parent.as_deref());
    }
    scopes
}

fn read_codex(
    actor: &Actor,
    parsed: Option<&crate::parsers::codex::billing::Receipts>,
    source: &mut LedgerSource,
) -> Vec<TaskReceipt> {
    let Some(parsed) = parsed else {
        return Vec::new();
    };
    if actor.own {
        source.messages.extend(
            parsed
                .message_turns
                .iter()
                .map(|(message, turn)| (message.clone(), format!("{}:{turn}", actor.id))),
        );
    }
    parsed
        .requests
        .iter()
        .map(|receipt| TaskReceipt {
            thread_id: receipt.thread_id.clone(),
            turn_id: receipt.turn_id.clone(),
            request_id: receipt.request_id.clone(),
            purpose: receipt.purpose.clone(),
            task_id: receipt.task_id.clone(),
            parent_tool_use_id: None,
            message_id: String::new(),
            root_turn_id: receipt.root_turn_id.clone(),
            tool_ids: Vec::new(),
            goal_identity_known: receipt.goal_identity_known,
        })
        .collect()
}

async fn load_receipts(
    conn: &DatabaseConnection,
    input: (
        Vec<Actor>,
        HashMap<String, crate::parsers::codex::billing::Receipts>,
    ),
    source: &mut LedgerSource,
) {
    let (actors, files) = input;
    let mut associated = Vec::new();
    for actor in actors {
        let receipts = match actor.agent {
            AgentType::Codex => read_codex(&actor, files.get(&actor.id), source),
            AgentType::ClaudeCode => runtime_task_receipts::load(conn, actor.agent, &actor.id)
                .await
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        source.incomplete |= receipts.is_empty();
        for receipt in receipts {
            if let Some(parent) = &receipt.parent_tool_use_id {
                source.tool_parents.extend(
                    receipt
                        .tool_ids
                        .iter()
                        .map(|tool| (tool.clone(), parent.clone())),
                );
            }
            if let Some(goal) = &receipt.task_id {
                source.tool_goals.extend(
                    receipt
                        .tool_ids
                        .iter()
                        .map(|tool| (tool.clone(), goal.clone())),
                );
            }
            associated.push((actor.clone(), receipt));
        }
    }
    for (actor, receipt) in associated {
        source.append(&actor, receipt);
    }
}
