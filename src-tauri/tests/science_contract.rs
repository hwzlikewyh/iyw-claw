use std::collections::HashSet;

use iyw_claw_lib::commands::{experts, science};

#[tokio::test]
async fn bundled_science_catalog_is_complete_and_disjoint() {
    let science = science::science_list().await.expect("science catalog");
    let experts = experts::experts_list().await.expect("expert catalog");

    assert_eq!(science.len(), 13);
    let science_ids = science
        .iter()
        .map(|item| item.metadata.id.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(science_ids.len(), science.len());
    assert!(science
        .iter()
        .filter(|item| item.metadata.featured)
        .all(|item| item
            .metadata
            .accent
            .as_deref()
            .is_some_and(|value| !value.is_empty())));
    assert!(experts
        .iter()
        .all(|item| !science_ids.contains(item.metadata.id.as_str())));
}

#[tokio::test]
async fn bundled_science_content_is_readable_before_installation() {
    let content = science::science_read_content("scientific-brainstorming".to_string())
        .await
        .expect("bundled SKILL.md fallback");

    assert!(content.contains("Scientific Brainstorming"));
}
