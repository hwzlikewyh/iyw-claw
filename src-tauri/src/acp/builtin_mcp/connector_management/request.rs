use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Request {
    InstalledList,
    MarketSearch {
        query: Option<String>,
        provider_id: Option<String>,
        limit: Option<u32>,
        offset: Option<u32>,
    },
    MarketDetail {
        provider_id: String,
        server_id: String,
    },
    InstallMarket {
        provider_id: String,
        server_id: String,
        option_id: String,
        protocol: String,
        parameter_values: Option<Value>,
    },
    Uninstall {
        server_id: String,
    },
    SetCurrentConversation {
        server_id: String,
        enabled: bool,
    },
    SetGlobalEnabled {
        server_id: String,
        enabled: bool,
    },
    PublishPlugin {
        request: crate::commands::skill_market::SkillMarketPublishRequest,
    },
    PublishPluginVersion {
        request: crate::commands::skill_market::SkillMarketAddVersionRequest,
    },
    UpdateMarketMetadata {
        request: crate::commands::skill_market::SkillMarketMetadataRequest,
    },
    DeleteMarketItem {
        market_id: String,
    },
}

impl Request {
    pub(super) fn is_mutation(&self) -> bool {
        !matches!(
            self,
            Self::InstalledList | Self::MarketSearch { .. } | Self::MarketDetail { .. }
        )
    }

    pub(super) fn is_publish(&self) -> bool {
        matches!(
            self,
            Self::PublishPlugin { .. }
                | Self::PublishPluginVersion { .. }
                | Self::UpdateMarketMetadata { .. }
                | Self::DeleteMarketItem { .. }
        )
    }

    pub(super) fn is_read(&self) -> bool {
        matches!(
            self,
            Self::InstalledList | Self::MarketSearch { .. } | Self::MarketDetail { .. }
        )
    }
}
