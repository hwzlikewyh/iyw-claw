use serde::Deserialize;
use serde_json::Value;
use std::sync::{Arc, OnceLock};

use super::capability_metadata::{first_sentence, public_text, search_score};
use super::capability_registry::RegistryError;
use super::capability_schema;
use super::features::FeatureSnapshot;

mod build;

const DEFAULT_SEARCH_LIMIT: usize = 8;
const MAX_SEARCH_LIMIT: usize = 20;
const MAX_SEARCH_QUERY_CHARS: usize = 256;

#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct CapabilitySummary {
    pub capability_id: &'static str,
    pub summary: String,
    pub category: String,
    pub aliases: Vec<String>,
    pub intent_terms: Vec<String>,
    pub when_to_use: String,
    pub required_inputs: Vec<String>,
    pub schema_digest: String,
    pub status: &'static str,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct CapabilityDetail {
    pub capability_id: &'static str,
    pub description: String,
    pub input_schema: Value,
    pub category: String,
    pub aliases: Vec<String>,
    pub intent_terms: Vec<String>,
    pub when_to_use: String,
    pub required_inputs: Vec<String>,
    pub schema_digest: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_policy: Option<MemoryPolicyHint>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(super) struct MemoryPolicyHint {
    pub revision: &'static str,
    pub digest: &'static str,
    pub summary: &'static str,
    pub reference: &'static str,
    pub document: &'static str,
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedCapability {
    pub tool_name: String,
    pub arguments: Value,
    pub delivery_ack: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EmbeddedTool {
    name: String,
    description: String,
    input_schema: Value,
}

#[derive(Debug)]
struct CatalogEntry {
    id: &'static str,
    tool: EmbeddedTool,
    category: String,
    aliases: Vec<String>,
    intent_terms: Vec<String>,
    negative_terms: Vec<String>,
    when_to_use: String,
    required_inputs: Vec<String>,
    schema_digest: String,
}

#[derive(Clone)]
pub(super) struct CapabilityCatalog {
    entries: Arc<[CatalogEntry]>,
    catalog_digest: String,
}

impl CapabilityCatalog {
    pub(super) fn load() -> Result<Self, CatalogError> {
        static EMBEDDED_CATALOG: OnceLock<Result<CapabilityCatalog, String>> = OnceLock::new();
        EMBEDDED_CATALOG
            .get_or_init(|| build::load().map_err(|error| error.to_string()))
            .as_ref()
            .cloned()
            .map_err(|message| CatalogError::Initialization(message.clone()))
    }

    pub(super) fn digest(&self) -> &str {
        &self.catalog_digest
    }

    pub(super) fn search(
        &self,
        features: &FeatureSnapshot,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<CapabilitySummary>, SearchError> {
        let query = query.trim();
        if query.is_empty() {
            return Err(SearchError::EmptyQuery);
        }
        if query.chars().count() > MAX_SEARCH_QUERY_CHARS {
            return Err(SearchError::QueryTooLong);
        }
        let limit = limit.unwrap_or(DEFAULT_SEARCH_LIMIT);
        if !(1..=MAX_SEARCH_LIMIT).contains(&limit) {
            return Err(SearchError::InvalidLimit);
        }
        let mut matches = self
            .available(features)
            .filter_map(|entry| search_match(entry, &query))
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then(left.1.capability_id.cmp(right.1.capability_id))
        });
        Ok(matches
            .into_iter()
            .take(limit)
            .map(|(_, summary)| summary)
            .collect())
    }

    pub(super) fn read(
        &self,
        features: &FeatureSnapshot,
        capability_id: &str,
    ) -> Option<CapabilityDetail> {
        let entry = self.find(capability_id)?;
        let available = features.should_list(&entry.tool.name);
        Some(CapabilityDetail {
            capability_id: entry.id,
            description: public_text(&entry.tool.description),
            input_schema: entry.tool.input_schema.clone(),
            category: entry.category.clone(),
            aliases: entry.aliases.clone(),
            intent_terms: entry.intent_terms.clone(),
            when_to_use: entry.when_to_use.clone(),
            required_inputs: entry.required_inputs.clone(),
            schema_digest: entry.schema_digest.clone(),
            status: if available {
                "available"
            } else {
                "unavailable"
            },
            unavailable_reason: (!available).then_some("disabled_for_session"),
            memory_policy: memory_policy_hint(entry.id),
        })
    }

    pub(super) fn resolve(
        &self,
        features: &FeatureSnapshot,
        capability_id: &str,
        arguments: Value,
    ) -> Result<ResolvedCapability, ResolveError> {
        let entry = self.find(capability_id).ok_or(ResolveError::Unknown)?;
        features
            .authorize_call(&entry.tool.name)
            .map_err(|_| ResolveError::Unavailable)?;
        capability_schema::validate(&entry.tool.input_schema, &arguments).map_err(|error| {
            super::capability_recovery::invalid_arguments(entry.id, &entry.schema_digest, error)
        })?;
        Ok(ResolvedCapability {
            tool_name: entry.tool.name.clone(),
            arguments,
            delivery_ack: None,
        })
    }

    fn available<'a>(
        &'a self,
        features: &'a FeatureSnapshot,
    ) -> impl Iterator<Item = &'a CatalogEntry> {
        self.entries
            .iter()
            .filter(|entry| features.should_list(&entry.tool.name))
    }

    fn find(&self, capability_id: &str) -> Option<&CatalogEntry> {
        self.entries.iter().find(|entry| entry.id == capability_id)
    }
}

fn memory_policy_hint(capability_id: &str) -> Option<MemoryPolicyHint> {
    capability_id
        .eq("iyw.memory.policy.read.v1")
        .then_some(MemoryPolicyHint {
            revision: crate::user_memory::MEMORY_POLICY_REVISION,
            digest: crate::user_memory::memory_policy_digest(),
            summary: crate::user_memory::MEMORY_POLICY_SUMMARY,
            reference: crate::user_memory::MEMORY_POLICY_REFERENCE,
            document: crate::user_memory::MEMORY_POLICY_DOCUMENT,
        })
}

fn search_match(entry: &CatalogEntry, query: &str) -> Option<(usize, CapabilitySummary)> {
    let score = search_score(
        entry.id,
        &entry.category,
        &entry.aliases,
        &entry.intent_terms,
        &entry.negative_terms,
        &entry.tool.description,
        query,
    )?;
    Some((
        score,
        CapabilitySummary {
            capability_id: entry.id,
            summary: public_text(&first_sentence(&entry.tool.description)),
            category: entry.category.clone(),
            aliases: entry.aliases.clone(),
            intent_terms: entry.intent_terms.clone(),
            when_to_use: entry.when_to_use.clone(),
            required_inputs: entry.required_inputs.clone(),
            schema_digest: entry.schema_digest.clone(),
            status: "available",
        },
    ))
}

#[derive(Debug, thiserror::Error)]
pub(super) enum CatalogError {
    #[error("{0}")]
    Initialization(String),
    #[error("invalid embedded companion schema: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("companion tool `{0}` has no stable capability id")]
    MissingStableId(String),
    #[error("missing intent metadata for companion tool `{0}`")]
    MissingIntentMetadata(String),
    #[error("{0}")]
    IntentMetadata(String),
    #[error(transparent)]
    Registry(#[from] RegistryError),
}

#[derive(Debug, thiserror::Error)]
pub(super) enum ResolveError {
    #[error("unknown capability id")]
    Unknown,
    #[error("capability is unavailable for this session")]
    Unavailable,
    #[error("arguments do not match the capability schema: {message}")]
    InvalidArguments { message: String, data: Value },
}

#[derive(Debug, thiserror::Error)]
pub(super) enum SearchError {
    #[error("query must be a non-empty string")]
    EmptyQuery,
    #[error("query must be at most {MAX_SEARCH_QUERY_CHARS} characters")]
    QueryTooLong,
    #[error("limit must be between 1 and {MAX_SEARCH_LIMIT}")]
    InvalidLimit,
}
