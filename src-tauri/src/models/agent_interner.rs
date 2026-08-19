/// Resolve only client-reviewed registry ids to their compile-time storage.
pub(super) fn intern(value: &str) -> Option<&'static str> {
    crate::acp::trusted_agents::definition_for(value).map(|definition| definition.registry_id)
}
