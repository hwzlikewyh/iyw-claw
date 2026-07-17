use super::*;

const STATS_UPDATES: &str = concat!(
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"hi"},"_meta":{"modelId":"grok-4.5-fast","promptIndex":0}},"_meta":{"turnStartMs":1000,"totalTokens":100}},"timestamp":1783584019}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hello"}},"_meta":{"totalTokens":500,"agentTimestampMs":3000}},"timestamp":1783584024}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"turn_completed","stop_reason":"end_turn"},"_meta":{"agentTimestampMs":5000}},"timestamp":1783584024}"#,
    "\n",
);

const SPARSE_UPDATES: &str = concat!(
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"hi"},"_meta":{"promptIndex":0}}},"timestamp":1783584019}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hello"}}},"timestamp":1783584024}"#,
    "\n",
    r#"{"method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"turn_completed","stop_reason":"end_turn"}},"timestamp":1783584024}"#,
    "\n",
);

#[test]
fn lists_session_with_metadata() {
    let (_temp, sessions) = fixture(SUMMARY, UPDATES);
    let conversations = GrokParser::with_base_dir(sessions)
        .list_conversations()
        .expect("conversation list");
    assert_eq!(conversations.len(), 1);
    let summary = &conversations[0];
    assert_eq!(summary.id, SESSION_ID);
    assert_eq!(summary.agent_type, AgentType::Grok);
    assert_eq!(summary.title.as_deref(), Some("Build the project"));
    assert_eq!(summary.model.as_deref(), Some("grok-4.5"));
    assert_eq!(summary.folder_path.as_deref(), Some("/Users/me/proj"));
    assert_eq!(summary.git_branch.as_deref(), Some("main"));
    assert_eq!(summary.message_count, 4);
}

#[test]
fn assistant_turn_carries_model_tokens_and_duration() {
    let detail = detail(STATS_UPDATES);
    let assistant = detail.turns.last().expect("assistant turn");
    assert!(matches!(assistant.role, TurnRole::Assistant));
    assert_eq!(assistant.model.as_deref(), Some("grok-4.5-fast"));
    let usage = assistant.usage.as_ref().expect("usage");
    assert_eq!(usage.input_tokens, 500);
    assert_eq!(usage.output_tokens, 0);
    assert_eq!(assistant.duration_ms, Some(4_000));

    let stats = detail.session_stats.expect("session stats");
    assert_eq!(stats.total_usage.as_ref().unwrap().input_tokens, 500);
    assert_eq!(stats.total_duration_ms, 4_000);
    assert_eq!(stats.context_window_used_tokens, Some(500));
    assert_eq!(stats.context_window_max_tokens, Some(500_000));
    let percent = stats.context_window_usage_percent.expect("percentage");
    assert!((percent - 0.1).abs() < 1e-6, "percent = {percent}");
}

#[test]
fn assistant_turn_model_falls_back_to_summary() {
    let detail = detail(SPARSE_UPDATES);
    let assistant = detail.turns.last().expect("assistant turn");
    assert_eq!(assistant.model.as_deref(), Some("grok-4.5"));
    assert!(assistant.usage.is_none());
    assert!(assistant.duration_ms.is_none());
}

#[test]
fn missing_conversation_errors() {
    let (_temp, sessions) = fixture(SUMMARY, UPDATES);
    let parser = GrokParser::with_base_dir(sessions);
    assert!(matches!(
        parser.get_conversation("does-not-exist"),
        Err(ParseError::ConversationNotFound(_))
    ));
}
