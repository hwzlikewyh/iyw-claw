use iyw_codex_harness::{
    Capability, CapabilitySet, CodexHarness, HarnessConfig, HarnessError, LifecycleError,
    OwnershipError, SessionAccess, SessionBinding, SessionError, SessionOwner, SessionRegistry,
    TurnBinding,
};

fn binding(generation: u64, runtime: &str) -> SessionBinding {
    let owner = SessionOwner::new("connection", Some(7), generation).unwrap();
    SessionBinding::new(owner, "thread", runtime).unwrap()
}

fn access(generation: u64, runtime: &str) -> SessionAccess<'_> {
    SessionAccess {
        external_id: "thread",
        connection_id: "connection",
        generation,
        runtime_fingerprint: runtime,
    }
}

fn ready_harness(capabilities: CapabilitySet) -> CodexHarness {
    let mut harness = CodexHarness::new(HarnessConfig::default()).unwrap();
    harness.mark_ready(capabilities).unwrap();
    harness
}

#[test]
fn rejects_stale_generation_and_runtime() {
    let mut registry = SessionRegistry::default();
    registry
        .bind(binding(3, "runtime-a"), CapabilitySet::all())
        .unwrap();
    registry
        .begin_turn(
            access(3, "runtime-a"),
            TurnBinding::new("thread", "turn-1").unwrap(),
        )
        .unwrap();
    for stale in [access(4, "runtime-a"), access(3, "runtime-b")] {
        let error = registry.steer(stale, "turn-1").unwrap_err();
        assert!(matches!(
            error,
            SessionError::Ownership(OwnershipError::StaleSession(_))
        ));
    }
}

#[test]
fn cancellation_is_idempotent_and_completion_clears_turn() {
    let mut registry = SessionRegistry::default();
    registry
        .bind(binding(1, "runtime"), CapabilitySet::all())
        .unwrap();
    registry
        .begin_turn(
            access(1, "runtime"),
            TurnBinding::new("thread", "turn-1").unwrap(),
        )
        .unwrap();
    assert!(matches!(
        registry.ensure_no_active_turn(access(1, "runtime")),
        Err(SessionError::TurnAlreadyActive(_))
    ));
    assert!(registry.cancel(access(1, "runtime"), "turn-1").unwrap());
    assert!(!registry.cancel(access(1, "runtime"), "turn-1").unwrap());
    assert!(
        registry
            .complete(access(1, "runtime"), "turn-1")
            .unwrap()
            .cancelling
    );
    assert!(registry.remove(access(1, "runtime")).unwrap());
}

#[test]
fn binding_revalidates_public_owner_fields() {
    let owner = SessionOwner {
        connection_id: " ".to_string(),
        conversation_id: Some(7),
        generation: 1,
    };
    assert!(matches!(
        SessionBinding::new(owner, "thread", "runtime"),
        Err(OwnershipError::EmptyField("connection id"))
    ));
}

#[test]
fn duplicate_binding_cannot_expand_capabilities() {
    let mut registry = SessionRegistry::default();
    let prompt = CapabilitySet::empty().with(Capability::Prompt);
    registry.bind(binding(1, "runtime"), prompt).unwrap();
    let expanded = prompt.with(Capability::Terminal);
    assert!(matches!(
        registry.bind(binding(1, "runtime"), expanded),
        Err(SessionError::CapabilityMismatch(_))
    ));
    assert_eq!(registry.capabilities("thread"), Some(prompt));
}

#[test]
fn preflight_rejects_conflicting_resume_binding() {
    let mut registry = SessionRegistry::default();
    registry
        .bind(binding(1, "runtime-a"), CapabilitySet::all())
        .unwrap();
    assert!(matches!(
        registry.ensure_bindable(&binding(2, "runtime-b"), CapabilitySet::all()),
        Err(SessionError::Ownership(OwnershipError::AlreadyBound(_)))
    ));
}

#[test]
fn runtime_and_session_capabilities_are_both_enforced() {
    let host = CapabilitySet::empty()
        .with(Capability::Prompt)
        .with(Capability::Steering);
    let mut harness = ready_harness(host);
    assert!(matches!(
        harness.bind_session(binding(1, "runtime"), CapabilitySet::all()),
        Err(HarnessError::CapabilityEscalation)
    ));
    harness
        .bind_session(
            binding(1, "runtime"),
            CapabilitySet::empty().with(Capability::Prompt),
        )
        .unwrap();
    harness
        .begin_turn(
            access(1, "runtime"),
            TurnBinding::new("thread", "turn-1").unwrap(),
        )
        .unwrap();
    assert!(matches!(
        harness.steer_turn(access(1, "runtime"), "turn-1"),
        Err(HarnessError::Session(SessionError::CapabilityMismatch(_)))
    ));
    assert!(matches!(
        harness.validate_session_capabilities(CapabilitySet::all()),
        Err(HarnessError::CapabilityEscalation)
    ));
}

#[test]
fn harness_rejects_a_second_turn_before_transport_send() {
    let mut harness = ready_harness(CapabilitySet::all());
    harness
        .bind_session(binding(1, "runtime"), CapabilitySet::all())
        .unwrap();
    harness
        .begin_turn(
            access(1, "runtime"),
            TurnBinding::new("thread", "turn-1").unwrap(),
        )
        .unwrap();
    assert!(matches!(
        harness.ensure_can_begin_turn(access(1, "runtime")),
        Err(HarnessError::Session(SessionError::TurnAlreadyActive(_)))
    ));
}

#[test]
fn revocation_is_immediate_and_shutdown_is_terminal() {
    let mut harness = ready_harness(CapabilitySet::all());
    harness
        .bind_session(binding(1, "runtime"), CapabilitySet::all())
        .unwrap();
    let cancelled = harness.revoke_capability(Capability::Prompt);
    assert_eq!(cancelled.len(), 0);
    assert!(matches!(
        harness.begin_turn(
            access(1, "runtime"),
            TurnBinding::new("thread", "turn-1").unwrap(),
        ),
        Err(HarnessError::CapabilityDenied(Capability::Prompt))
    ));
    harness.begin_shutdown().unwrap();
    harness.finish_shutdown().unwrap();
    assert!(matches!(
        harness.mark_ready(CapabilitySet::all()),
        Err(LifecycleError::InvalidTransition { .. })
    ));
}

#[test]
fn capability_revocation_clears_active_turns() {
    let mut harness = ready_harness(CapabilitySet::all());
    harness
        .bind_session(binding(1, "runtime"), CapabilitySet::all())
        .unwrap();
    harness
        .begin_turn(
            access(1, "runtime"),
            TurnBinding::new("thread", "turn-1").unwrap(),
        )
        .unwrap();
    let cancelled = harness.revoke_capability(Capability::Prompt);
    assert_eq!(cancelled.len(), 1);
    assert!(matches!(
        harness.validate_turn(access(1, "runtime"), "turn-1"),
        Err(HarnessError::Session(SessionError::UnknownTurn(_)))
    ));
}

#[test]
fn capability_set_is_bit_stable() {
    let set = CapabilitySet::empty()
        .with(Capability::Prompt)
        .with(Capability::Mcp);
    assert!(set.contains(Capability::Prompt));
    assert!(set.contains(Capability::Mcp));
    assert!(!set.contains(Capability::Terminal));
    assert_eq!(set.without(Capability::Prompt).bits(), 1 << 4);
}
