use super::*;

#[test]
fn held_turn_settlement_is_visible_but_its_overlay_copy_is_suppressed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.jsonl");
    std::fs::File::create(&path).unwrap();
    let ledger = PromptLedger::new();
    ledger.record_text("start agent");
    let mut watcher = WatchState::with_file_for_test("session-1", path.clone());

    append_records(
        &path,
        &[
            user_record("user-1", "start agent"),
            async_launch("agent-1"),
        ],
    );
    let event = watcher
        .tick(&ledger, "D:/work", "connection-1", true, false)
        .expect("launch changes accounting");
    assert_eq!(
        unpack_background(event),
        (0, 1, 0, std::fs::metadata(&path).unwrap().len())
    );

    append_records(
        &path,
        &[
            notification("agent-1"),
            assistant_record("assistant-2", "held result"),
        ],
    );
    let event = watcher
        .tick(&ledger, "D:/work", "connection-1", false, false)
        .expect("settlement is surfaced");
    match event {
        AcpEvent::BackgroundActivity {
            turns,
            outstanding,
            settled,
            ..
        } => {
            assert!(turns.is_empty());
            assert_eq!(outstanding, 0);
            assert_eq!(settled.len(), 1);
            assert!(settled[0].wire_visible);
        }
        other => panic!("expected background activity, got {other:?}"),
    }
}
