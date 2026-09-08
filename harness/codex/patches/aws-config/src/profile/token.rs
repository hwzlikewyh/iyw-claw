/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! Profile File Based Token Providers

use crate::profile::cell::ErrorTakingOnceCell;
#[allow(deprecated)]
use crate::profile::profile_file::ProfileFiles;
use crate::profile::ProfileSet;
use crate::provider_config::ProviderConfig;
use crate::sso::SsoTokenProvider;
use aws_credential_types::provider::{
    error::TokenError, future, token::ProvideToken, token::Result as TokenResult,
};
use aws_types::{region::Region, SdkConfig};

async fn load_profile_set(provider_config: &ProviderConfig) -> Result<&ProfileSet, TokenError> {
    provider_config
        .try_profile()
        .await
        .map_err(|parse_err| TokenError::invalid_configuration(parse_err.clone()))
}

fn create_token_provider(
    sdk_config: &SdkConfig,
    provider_config: &ProviderConfig,
    profile_set: &ProfileSet,
) -> Result<SsoTokenProvider, TokenError> {
    let repr = crate::profile::credentials::repr::resolve_chain(profile_set)
        .map_err(TokenError::invalid_configuration)?;
    match repr.base {
        crate::profile::credentials::repr::BaseProvider::Sso {
            sso_session_name,
            sso_region,
            sso_start_url,
            ..
        } => {
            let mut builder = SsoTokenProvider::builder().configure(sdk_config);
            builder.set_session_name(sso_session_name.map(|s| s.to_string()));
            Ok(builder
                .region(Region::new(sso_region.to_string()))
                .start_url(sso_start_url)
                .build_with(provider_config.env(), provider_config.fs()))
        }
        _ => Err(TokenError::not_loaded(
            "no sso-session configured in profile file",
        )),
    }
}

/// AWS profile-based access token provider
///
/// This token provider loads SSO session config from `~/.aws/config`,
/// and uses that config to resolve a cached SSO token from `~/.aws/sso/cache`.
/// The AWS CLI can be used to establish the cached SSO token.
///
/// Generally, this provider is constructed via the default provider chain. However,
/// it can also be manually constructed with the builder:
/// ```rust,no_run
/// use aws_config::profile::ProfileFileTokenProvider;
/// let provider = ProfileFileTokenProvider::builder().build();
/// ```
/// _Note: this provider, when called, will load and parse the `~/.aws/config` file
/// only once. Parsed file contents will be cached indefinitely._
///
/// This provider requires a profile with a `sso_session` configured. For example,
/// ```ini
/// [default]
/// sso_session = example
/// region = us-west-2
///
/// [sso-session example]
/// sso_start_url = https://example.awsapps.com/start
/// sso_region = us-west-2
/// ```
#[derive(Debug)]
pub struct ProfileFileTokenProvider {
    sdk_config: SdkConfig,
    provider_config: ProviderConfig,
    inner_provider: ErrorTakingOnceCell<SsoTokenProvider, TokenError>,
}

impl ProfileFileTokenProvider {
    /// Builder for this token provider.
    pub fn builder() -> Builder {
        Builder::default()
    }

    async fn load_token(&self) -> TokenResult {
        let inner_provider = self
            .inner_provider
            .get_or_init(
                {
                    let sdk_config = self.sdk_config.clone();
                    let provider_config = self.provider_config.clone();
                    move || async move {
                        let profile_set = load_profile_set(&provider_config).await?;
                        create_token_provider(&sdk_config, &provider_config, profile_set)
                    }
                },
                TokenError::unhandled(
                    "profile file token provider initialization error already taken",
                ),
            )
            .await?;

        inner_provider.provide_token().await
    }
}

impl ProvideToken for ProfileFileTokenProvider {
    fn provide_token<'a>(&'a self) -> future::ProvideToken<'a>
    where
        Self: 'a,
    {
        future::ProvideToken::new(self.load_token())
    }
}

/// Builder for [`ProfileFileTokenProvider`].
#[derive(Debug, Default)]
pub struct Builder {
    provider_config: Option<ProviderConfig>,
    profile_override: Option<String>,
    #[allow(deprecated)]
    profile_files: Option<ProfileFiles>,
}

impl Builder {
    /// Override the configuration for the [`ProfileFileTokenProvider`]
    pub(crate) fn configure(mut self, provider_config: &ProviderConfig) -> Self {
        self.provider_config = Some(provider_config.clone());
        self
    }

    /// Override the profile name used by the [`ProfileFileTokenProvider`]
    pub fn profile_name(mut self, profile_name: impl Into<String>) -> Self {
        self.profile_override = Some(profile_name.into());
        self
    }

    /// Set the profile file that should be used by the [`ProfileFileTokenProvider`]
    #[allow(deprecated)]
    pub fn profile_files(mut self, profile_files: ProfileFiles) -> Self {
        self.profile_files = Some(profile_files);
        self
    }

    /// Builds a [`ProfileFileTokenProvider`]
    pub fn build(self) -> ProfileFileTokenProvider {
        let build_span = tracing::debug_span!("build_profile_token_provider");
        let _enter = build_span.enter();
        let conf = self
            .provider_config
            .unwrap_or_default()
            .with_profile_config(self.profile_files, self.profile_override);

        ProfileFileTokenProvider {
            sdk_config: conf.client_config(),
            provider_config: conf,
            inner_provider: ErrorTakingOnceCell::new(),
        }
    }
}
