use super::{CapabilityCatalog, CatalogEntry, CatalogError, EmbeddedTool};
use crate::acp::builtin_mcp::capability_intents::intent_metadata;
use crate::acp::builtin_mcp::capability_metadata::{
    capability_aliases, capability_category, digest, intent_terms, negative_terms, public_value,
    required_inputs, validate_intent_metadata, when_to_use,
};
use crate::acp::builtin_mcp::capability_registry::{stable_capability_id, validate_bindings};

pub(super) fn load() -> Result<CapabilityCatalog, CatalogError> {
    let tools = serde_json::from_str::<Vec<EmbeddedTool>>(
        crate::acp::delegation::companion::TOOL_SCHEMA_JSON,
    )?;
    validate_bindings(tools.iter().map(|tool| tool.name.as_str()))?;
    validate_intent_metadata(tools.iter().map(|tool| tool.name.as_str()))
        .map_err(CatalogError::IntentMetadata)?;
    let entries = tools
        .into_iter()
        .map(build_entry)
        .collect::<Result<Vec<_>, CatalogError>>()?;
    let catalog_digest = digest(
        &entries
            .iter()
            .map(|entry| {
                (
                    &entry.id,
                    &entry.schema_digest,
                    &entry.aliases,
                    &entry.intent_terms,
                    &entry.when_to_use,
                )
            })
            .collect::<Vec<_>>(),
    )?;
    tracing::debug!(
        target: "builtin_mcp",
        capability_count = entries.len(),
        catalog_digest,
        "embedded capability metadata initialized"
    );
    Ok(CapabilityCatalog {
        entries: entries.into(),
        catalog_digest,
    })
}

fn build_entry(mut tool: EmbeddedTool) -> Result<CatalogEntry, CatalogError> {
    let id = stable_capability_id(&tool.name)
        .ok_or_else(|| CatalogError::MissingStableId(tool.name.clone()))?;
    let metadata = intent_metadata(&tool.name)
        .ok_or_else(|| CatalogError::MissingIntentMetadata(tool.name.clone()))?;
    tool.input_schema = public_value(tool.input_schema);
    Ok(CatalogEntry {
        id,
        category: capability_category(id),
        aliases: capability_aliases(id, &tool.name),
        intent_terms: intent_terms(metadata),
        negative_terms: negative_terms(metadata),
        when_to_use: when_to_use(metadata),
        required_inputs: required_inputs(&tool.input_schema),
        schema_digest: digest(&tool.input_schema)?,
        tool,
    })
}
