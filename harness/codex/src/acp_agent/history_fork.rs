use std::collections::BTreeSet;

use serde_json::{json, Value};

use crate::{UpstreamClient, UpstreamError};

const FORK_POINT_VERSION: u64 = 1;
const HISTORY_PAGE_SIZE: u32 = 100;
const HISTORY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

pub(super) async fn request(
    upstream: &UpstreamClient,
    source: &str,
    params: &Value,
) -> Result<Value, UpstreamError> {
    let mut request = json!({ "method": "thread/fork", "params": {
        "threadId": source, "excludeTurns": true, "deferGoalContinuation": true,
    } });
    let Some(point) = params.pointer("/_meta/jetbrains/air/fork") else {
        return Ok(request);
    };
    let target = Target::parse(point)?;
    eprintln!("[星河][worker] resolving history fork: source={source}");
    let result = tokio::time::timeout(HISTORY_TIMEOUT, resolve(upstream, source, target)).await;
    let turn = match result {
        Ok(Ok(turn)) => turn,
        Ok(Err(error)) => {
            eprintln!(
                "[星河][worker] history fork resolution failed: source={source} error={error}"
            );
            return Err(error);
        }
        Err(_) => {
            eprintln!("[星河][worker] history fork resolution timed out: source={source}");
            return Err(invalid("分叉历史读取超时，请重试"));
        }
    };
    eprintln!("[星河][worker] history fork resolved: source={source} last_turn={turn}");
    request["params"]["lastTurnId"] = json!(turn);
    Ok(request)
}

struct Target<'a> {
    message_id: &'a str,
    legacy_text: Option<&'a str>,
    occurrence: u64,
    seen: u64,
}

impl<'a> Target<'a> {
    fn parse(point: &'a Value) -> Result<Self, UpstreamError> {
        if point["version"].as_u64() != Some(FORK_POINT_VERSION) {
            return Err(invalid("不支持的历史分叉定位格式，请更新应用"));
        }
        let message_id = point["messageId"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| invalid("分叉消息缺少消息 ID"))?;
        // 旧解析器生成 assistant-N，无法与上游重建的 item-N 直接比较。
        let legacy = message_id
            .strip_prefix("assistant-")
            .is_some_and(|index| index.parse::<usize>().is_ok());
        let legacy_text = legacy.then(|| point["messageText"].as_str()).flatten();
        let occurrence = point["messageOccurrence"].as_u64().unwrap_or_default();
        if legacy && (legacy_text.is_none_or(str::is_empty) || occurrence == 0) {
            return Err(invalid(
                "旧历史分叉缺少消息正文或出现序号，请刷新会话后重试",
            ));
        }
        Ok(Self {
            message_id,
            legacy_text,
            occurrence,
            seen: 0,
        })
    }

    fn matches(&mut self, item: &Value) -> bool {
        if item["type"] != "agentMessage" {
            return false;
        }
        let Some(text) = self.legacy_text else {
            return item["id"] == self.message_id;
        };
        if item["text"].as_str() != Some(text) {
            return false;
        }
        self.seen += 1;
        self.seen == self.occurrence
    }
}

async fn resolve(
    upstream: &UpstreamClient,
    source: &str,
    mut target: Target<'_>,
) -> Result<String, UpstreamError> {
    let mut cursor = Value::Null;
    let mut visited = BTreeSet::new();
    loop {
        let page = upstream
            .history_page(
                source,
                json!({
                    "method": "thread/turns/list", "params": {
                        "threadId": source, "limit": HISTORY_PAGE_SIZE,
                        "cursor": cursor, "sortDirection": "asc", "itemsView": "full",
                    },
                }),
            )
            .await?;
        let turns = page["data"]
            .as_array()
            .ok_or_else(|| invalid("分叉历史缺少回合列表"))?;
        for turn in turns {
            if let Some(id) = matching_turn(&mut target, turn)? {
                return Ok(id);
            }
        }
        let Some(next) = page["nextCursor"].as_str() else {
            return Err(invalid("分叉消息已变化或尚未保存，请刷新会话后重试"));
        };
        if !visited.insert(next.to_string()) {
            return Err(invalid("分叉历史分页游标重复"));
        }
        cursor = json!(next);
    }
}

fn matching_turn(target: &mut Target<'_>, turn: &Value) -> Result<Option<String>, UpstreamError> {
    let items = turn["items"]
        .as_array()
        .ok_or_else(|| invalid("分叉历史缺少回合消息"))?;
    // 宿主历史将同轮多次 final_answer 合为最后一版，旧正文序号也按此计数。
    let legacy = target.legacy_text.is_some();
    let last_final = items
        .iter()
        .rposition(|item| item["type"] == "agentMessage" && item["phase"] == "final_answer");
    let matched = items.iter().enumerate().any(|(index, item)| {
        if legacy && item["phase"] == "final_answer" && Some(index) != last_final {
            return false;
        }
        target.matches(item)
    });
    if !matched {
        return Ok(None);
    }
    if !matches!(
        turn["status"].as_str(),
        Some("completed" | "interrupted" | "failed")
    ) {
        return Err(invalid("分叉消息所在回合尚未结束，请稍后重试"));
    }
    turn["id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .map(|id| Some(id.to_string()))
        .ok_or_else(|| invalid("分叉历史缺少回合 ID"))
}

fn invalid(message: &str) -> UpstreamError {
    UpstreamError::InvalidRequest(message.into())
}
