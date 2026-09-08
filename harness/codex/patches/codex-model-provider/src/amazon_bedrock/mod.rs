mod auth;
mod auth_refresh;
mod catalog;
mod error;
mod mantle;
mod runtime;
mod runtime_catalog;

use std::path::PathBuf;
use std::sync::Arc;

use codex_api::ApiError;
use codex_api::Provider;
use codex_api::SharedAuthProvider;
use codex_api::TransportError;
use codex_login::AuthManager;
use codex_login::CodexAuth;
use codex_model_provider_info::AMAZON_BEDROCK_GPT_5_6_LUNA_MODEL_ID;
use codex_model_provider_info::AMAZON_BEDROCK_GPT_5_6_TERRA_MODEL_ID;
use codex_model_provider_info::AMAZON_BEDROCK_RUNTIME_GLOBAL_GPT_5_6_LUNA_MODEL_ID;
use codex_model_provider_info::AMAZON_BEDROCK_RUNTIME_GLOBAL_GPT_5_6_TERRA_MODEL_ID;
use codex_model_provider_info::ModelProviderAwsAuthInfo;
use codex_model_provider_info::ModelProviderInfo;
use codex_models_manager::manager::SharedModelsManager;
use codex_models_manager::manager::StaticModelsManager;
use codex_protocol::account::ProviderAccount;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result;
use codex_protocol::openai_models::ModelsResponse;

use crate::auth::auth_manager_for_provider;
use crate::auth::resolve_provider_auth as resolve_configured_provider_auth;
use crate::provider::ModelProvider;
use crate::provider::ModelProviderFuture;
use crate::provider::ProviderAccountResult;
use crate::provider::ProviderAccountState;
use crate::provider::ProviderAuthRecoveryMessages;
use crate::provider::ProviderCapabilities;
use crate::provider::ProviderUnauthorizedRecovery;
use crate::provider::RemoteCompactionSupport;
use crate::shared_state::process_shared_state;
use auth::resolve_provider_auth as resolve_bedrock_provider_auth;
pub(crate) use auth_refresh::AwsAuthRecovery;
use catalog::normalize_bedrock_catalog;
pub(crate) use catalog::static_model_catalog;
use mantle::bedrock_mantle_runtime_base_url;
pub use mantle::is_supported_amazon_bedrock_region;
use runtime::bedrock_runtime_base_url;
use runtime_catalog::static_runtime_model_catalog;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BedrockEndpoint {
    Mantle,
    Runtime,
}

/// Runtime provider for Amazon Bedrock's OpenAI-compatible endpoints.
#[derive(Clone, Debug)]
pub(crate) struct AmazonBedrockModelProvider {
    pub(crate) info: ModelProviderInfo,
    aws: ModelProviderAwsAuthInfo,
    endpoint: BedrockEndpoint,
    auth_manager: Option<Arc<AuthManager>>,
    auth_recovery: Option<Arc<AwsAuthRecovery>>,
}

impl AmazonBedrockModelProvider {
    pub(crate) fn new(
        provider_info: ModelProviderInfo,
        auth_manager: Option<Arc<AuthManager>>,
    ) -> Self {
        let endpoint = if provider_info.is_amazon_bedrock_runtime() {
            BedrockEndpoint::Runtime
        } else {
            BedrockEndpoint::Mantle
        };
        let aws = provider_info
            .aws
            .clone()
            .unwrap_or(ModelProviderAwsAuthInfo {
                profile: None,
                region: None,
                auth_refresh: None,
            });
        let uses_aws_sdk_auth = matches!(
            auth::auth_source(&provider_info, auth_manager.as_deref(), std::env::var),
            auth::BedrockAuthSource::ConfiguredAwsProfile | auth::BedrockAuthSource::AwsSdk
        );
        let auth_recovery = if uses_aws_sdk_auth && aws.auth_refresh.is_some() {
            process_shared_state().aws_auth_recovery(&aws)
        } else {
            None
        };
        let auth_manager = auth_manager_for_provider(auth_manager, &provider_info);
        Self {
            info: provider_info,
            aws,
            endpoint,
            auth_manager,
            auth_recovery,
        }
    }

    fn auth_source(&self) -> auth::BedrockAuthSource {
        auth::auth_source(&self.info, self.auth_manager.as_deref(), std::env::var)
    }

    fn managed_auth(&self) -> Option<CodexAuth> {
        let source = self.auth_source();
        self.auth_manager
            .as_deref()
            .and_then(AuthManager::auth_cached)
            .filter(|auth| {
                matches!(
                    (source, auth),
                    (
                        auth::BedrockAuthSource::ManagedBearerToken,
                        CodexAuth::BedrockApiKey(_)
                    ) | (
                        auth::BedrockAuthSource::ManagedAccessKeys,
                        CodexAuth::BedrockAccessKeys(_)
                    )
                )
            })
    }

    fn uses_aws_auth_recovery(&self) -> bool {
        self.auth_recovery.is_some()
            && matches!(
                self.auth_source(),
                auth::BedrockAuthSource::ConfiguredAwsProfile | auth::BedrockAuthSource::AwsSdk
            )
    }

    async fn auth(&self) -> Option<CodexAuth> {
        match self.auth_source() {
            auth::BedrockAuthSource::CommandBearerToken => match self.auth_manager.as_ref() {
                Some(auth_manager) => auth_manager.auth().await,
                None => None,
            },
            auth::BedrockAuthSource::ManagedBearerToken
            | auth::BedrockAuthSource::ManagedAccessKeys => self.managed_auth(),
            auth::BedrockAuthSource::ConfiguredAwsProfile
            | auth::BedrockAuthSource::EnvBearerToken
            | auth::BedrockAuthSource::EnvAwsCredentials
            | auth::BedrockAuthSource::AwsSdk => None,
        }
    }

    async fn api_provider(&self) -> Result<Provider> {
        let mut api_provider_info = self.info.clone();
        api_provider_info.base_url = self.runtime_base_url().await?;
        api_provider_info.to_api_provider(/*auth_mode*/ None)
    }

    async fn runtime_base_url(&self) -> Result<Option<String>> {
        if let Some(base_url) = self.info.base_url.clone() {
            return Ok(Some(base_url));
        }
        let auth_source = self.auth_source();
        let managed_auth = self.managed_auth();
        let base_url = match self.endpoint {
            BedrockEndpoint::Mantle => {
                bedrock_mantle_runtime_base_url(auth_source, managed_auth.as_ref(), &self.aws)
                    .await?
            }
            BedrockEndpoint::Runtime => {
                bedrock_runtime_base_url(auth_source, managed_auth.as_ref(), &self.aws).await?
            }
        };
        Ok(Some(base_url))
    }

    async fn api_auth(&self) -> Result<SharedAuthProvider> {
        let source = self.auth_source();
        if source == auth::BedrockAuthSource::CommandBearerToken {
            let auth = self.auth().await;
            return resolve_configured_provider_auth(auth.as_ref(), &self.info);
        }

        let managed_auth = self.managed_auth();
        resolve_bedrock_provider_auth(source, managed_auth.as_ref(), &self.aws, self.endpoint).await
    }

    fn default_model_catalog(&self) -> ModelsResponse {
        match self.endpoint {
            BedrockEndpoint::Mantle => static_model_catalog(),
            BedrockEndpoint::Runtime => static_runtime_model_catalog(),
        }
    }
}

impl ModelProvider for AmazonBedrockModelProvider {
    fn info(&self) -> &ModelProviderInfo {
        &self.info
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            namespace_tools: true,
            image_generation: false,
            web_search: self.endpoint == BedrockEndpoint::Mantle,
            external_web_access: false,
            remote_compaction: RemoteCompactionSupport::V2,
        }
    }

    fn approval_review_preferred_model(&self) -> &'static str {
        match self.endpoint {
            BedrockEndpoint::Mantle => AMAZON_BEDROCK_GPT_5_6_LUNA_MODEL_ID,
            BedrockEndpoint::Runtime => AMAZON_BEDROCK_RUNTIME_GLOBAL_GPT_5_6_LUNA_MODEL_ID,
        }
    }

    fn memory_extraction_preferred_model(&self) -> &'static str {
        match self.endpoint {
            BedrockEndpoint::Mantle => AMAZON_BEDROCK_GPT_5_6_LUNA_MODEL_ID,
            BedrockEndpoint::Runtime => AMAZON_BEDROCK_RUNTIME_GLOBAL_GPT_5_6_LUNA_MODEL_ID,
        }
    }

    fn memory_consolidation_preferred_model(&self) -> &'static str {
        match self.endpoint {
            BedrockEndpoint::Mantle => AMAZON_BEDROCK_GPT_5_6_TERRA_MODEL_ID,
            BedrockEndpoint::Runtime => AMAZON_BEDROCK_RUNTIME_GLOBAL_GPT_5_6_TERRA_MODEL_ID,
        }
    }

    fn auth_manager(&self) -> Option<Arc<AuthManager>> {
        match self.auth_source() {
            auth::BedrockAuthSource::CommandBearerToken
            | auth::BedrockAuthSource::ManagedBearerToken
            | auth::BedrockAuthSource::ManagedAccessKeys => self.auth_manager.clone(),
            auth::BedrockAuthSource::ConfiguredAwsProfile
            | auth::BedrockAuthSource::EnvBearerToken
            | auth::BedrockAuthSource::EnvAwsCredentials
            | auth::BedrockAuthSource::AwsSdk => None,
        }
    }

    fn is_recoverable_auth_error(&self, error: &TransportError) -> bool {
        matches!(
            error,
            TransportError::Http { status, .. } if *status == http::StatusCode::UNAUTHORIZED
        ) || (self.uses_aws_auth_recovery() && error::is_refreshable_auth_error(error))
    }

    fn auth_recovery_messages(&self) -> Option<ProviderAuthRecoveryMessages> {
        self.uses_aws_auth_recovery()
            .then_some(ProviderAuthRecoveryMessages {
                started: "AWS session has expired. Reauthenticating...",
                succeeded: "Signed in with AWS.",
            })
    }

    fn recover_from_unauthorized(
        &self,
    ) -> ModelProviderFuture<'_, Result<ProviderUnauthorizedRecovery>> {
        Box::pin(async move {
            let Some(recovery) = self
                .auth_recovery
                .as_ref()
                .filter(|_| self.uses_aws_auth_recovery())
            else {
                return Ok(ProviderUnauthorizedRecovery::NotConfigured);
            };

            recovery.refresh().await.map_err(|error| {
                if error.kind() == std::io::ErrorKind::InvalidInput {
                    CodexErr::InvalidRequest(error.to_string())
                } else {
                    CodexErr::Io(error)
                }
            })?;
            Ok(ProviderUnauthorizedRecovery::Recovered)
        })
    }

    fn auth(&self) -> ModelProviderFuture<'_, Option<CodexAuth>> {
        Box::pin(AmazonBedrockModelProvider::auth(self))
    }

    fn account_state(&self) -> ProviderAccountResult {
        Ok(ProviderAccountState {
            account: Some(ProviderAccount::AmazonBedrock {
                uses_codex_managed_credentials: matches!(
                    self.auth_source(),
                    auth::BedrockAuthSource::ManagedBearerToken
                        | auth::BedrockAuthSource::ManagedAccessKeys
                ),
            }),
            requires_openai_auth: false,
        })
    }

    fn map_api_error(&self, error: ApiError) -> CodexErr {
        error::map_api_error(error)
    }

    fn api_provider(&self) -> ModelProviderFuture<'_, Result<Provider>> {
        Box::pin(AmazonBedrockModelProvider::api_provider(self))
    }

    fn runtime_base_url(&self) -> ModelProviderFuture<'_, Result<Option<String>>> {
        Box::pin(AmazonBedrockModelProvider::runtime_base_url(self))
    }

    fn api_auth(&self) -> ModelProviderFuture<'_, Result<SharedAuthProvider>> {
        Box::pin(AmazonBedrockModelProvider::api_auth(self))
    }

    fn models_manager(
        &self,
        _codex_home: PathBuf,
        config_model_catalog: Option<ModelsResponse>,
    ) -> SharedModelsManager {
        Arc::new(StaticModelsManager::new(
            /*auth_manager*/ None,
            config_model_catalog
                .map_or_else(|| self.default_model_catalog(), normalize_bedrock_catalog),
        ))
    }

    fn models_manager_without_cache(
        &self,
        config_model_catalog: Option<ModelsResponse>,
    ) -> SharedModelsManager {
        Arc::new(StaticModelsManager::new(
            /*auth_manager*/ None,
            config_model_catalog
                .map_or_else(|| self.default_model_catalog(), normalize_bedrock_catalog),
        ))
    }
}
