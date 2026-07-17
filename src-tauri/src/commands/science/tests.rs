use std::collections::HashSet;

use super::bundle::catalog_ids_are_disjoint;
use super::metadata::bundled_metadata;

#[test]
fn bundled_metadata_has_expected_contract() {
    let metadata = bundled_metadata();
    assert_eq!(metadata.len(), 13);
    assert_eq!(
        metadata
            .iter()
            .map(|item| item.id.as_str())
            .collect::<HashSet<_>>()
            .len(),
        metadata.len()
    );
    assert!(metadata.iter().all(|item| {
        !item.featured
            || item
                .accent
                .as_deref()
                .is_some_and(|accent| !accent.is_empty())
    }));
}

#[test]
fn science_ids_do_not_collide_with_other_managed_families() {
    assert!(catalog_ids_are_disjoint());
}

#[tokio::test]
async fn bundled_content_falls_back_without_central_install() {
    let content = super::science_read_content("scientific-brainstorming".to_string())
        .await
        .unwrap();
    assert!(content.contains("Scientific Brainstorming"));
}
