/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use std::borrow::Cow;

use aws_credential_types::provider::{self, future, ProvideCredentials};
use aws_credential_types::Credentials;
use tracing::Instrument;

use crate::environment::credentials::EnvironmentVariableCredentialsProvider;
use crate::meta::credentials::CredentialsProviderChain;
use crate::meta::region::ProvideRegion;
use crate::provider_config::ProviderConfig;

#[cfg(any(feature = "default-https-client", feature = "rustls"))]
/// Default Credentials Provider chain
///
/// The region from the default region provider will be used
pub async fn default_provider() -> impl ProvideCredentials {
    DefaultCredentialsChain::builder().build().await
}

/// Default AWS Credential Provider Chain
///
/// Resolution order:
/// 1. Environment variables: [`EnvironmentVariableCredentialsProvider`]
/// 2. Shared config (`~/.aws/config`, `~/.aws/credentials`): [`SharedConfigCredentialsProvider`](crate::profile::ProfileFileCredentialsProvider)
/// 3. [Web Identity Tokens](crate::web_identity_token)
/// 4. ECS (IAM Roles for Tasks) & General HTTP credentials: [`ecs`](crate::ecs)
/// 5. [EC2 IMDSv2](crate::imds)
///
/// The outer provider is wrapped in a refreshing cache.
///
/// More providers are a work in progress.
///
/// # Examples
/// Create a default chain with a custom region:
/// ```no_run
/// use aws_types::region::Region;
/// use aws_config::default_provider::credentials::DefaultCredentialsChain;
/// let credentials_provider = DefaultCredentialsChain::builder()
///     .region(Region::new("us-west-1"))
///     .build();
/// ```
///
/// Create a default chain with no overrides:
/// ```no_run
/// use aws_config::default_provider::credentials::DefaultCredentialsChain;
/// let credentials_provider = DefaultCredentialsChain::builder().build();
/// ```
///
/// Create a default chain that uses a different profile:
/// ```no_run
/// use aws_config::default_provider::credentials::DefaultCredentialsChain;
/// let credentials_provider = DefaultCredentialsChain::builder()
///     .profile_name("otherprofile")
///     .build();
/// ```
#[derive(Debug)]
pub struct DefaultCredentialsChain {
    provider_chain: CredentialsProviderChain,
}

impl DefaultCredentialsChain {
    /// Builder for `DefaultCredentialsChain`
    pub fn builder() -> Builder {
        Builder::default()
    }

    async fn credentials(&self) -> provider::Result {
        self.provider_chain
            .provide_credentials()
            .instrument(tracing::debug_span!("default_credentials_chain"))
            .await
    }
}

impl ProvideCredentials for DefaultCredentialsChain {
    fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        future::ProvideCredentials::new(self.credentials())
    }

    fn fallback_on_interrupt(&self) -> Option<Credentials> {
        self.provider_chain.fallback_on_interrupt()
    }
}

/// Builder for [`DefaultCredentialsChain`].
#[derive(Debug, Default)]
pub struct Builder {
    profile_file_builder: crate::profile::credentials::Builder,
    web_identity_builder: crate::web_identity_token::Builder,
    imds_builder: crate::imds::credentials::Builder,
    ecs_builder: crate::ecs::Builder,
    region_override: Option<Box<dyn ProvideRegion>>,
    region_chain: crate::default_provider::region::Builder,
    conf: Option<ProviderConfig>,
}

impl Builder {
    /// Sets the region used when making requests to AWS services
    ///
    /// When unset, the default region resolver chain will be used.
    pub fn region(mut self, region: impl ProvideRegion + 'static) -> Self {
        self.set_region(Some(region));
        self
    }

    /// Sets the region used when making requests to AWS services
    ///
    /// When unset, the default region resolver chain will be used.
    pub fn set_region(&mut self, region: Option<impl ProvideRegion + 'static>) -> &mut Self {
        self.region_override = region.map(|provider| Box::new(provider) as _);
        self
    }

    /// Add an additional credential source for the ProfileProvider
    ///
    /// Assume role profiles may specify named credential sources:
    /// ```ini
    /// [default]
    /// role_arn = arn:aws:iam::123456789:role/RoleA
    /// credential_source = MyCustomProvider
    /// ```
    ///
    /// Typically, these are built-in providers like `Environment`, however, custom sources may
    /// also be used.
    ///
    /// See [`with_custom_provider`](crate::profile::credentials::Builder::with_custom_provider)
    pub fn with_custom_credential_source(
        mut self,
        name: impl Into<Cow<'static, str>>,
        provider: impl ProvideCredentials + 'static,
    ) -> Self {
        self.profile_file_builder = self
            .profile_file_builder
            .with_custom_provider(name, provider);
        self
    }

    /// Override the profile name used by this provider
    ///
    /// When unset, the value of the `AWS_PROFILE` environment variable will be used.
    pub fn profile_name(mut self, name: &str) -> Self {
        self.profile_file_builder = self.profile_file_builder.profile_name(name);
        self.region_chain = self.region_chain.profile_name(name);
        self
    }

    /// Override the IMDS client used for this provider
    ///
    /// When unset, the default IMDS client will be used.
    pub fn imds_client(mut self, client: crate::imds::Client) -> Self {
        self.imds_builder = self.imds_builder.imds_client(client);
        self
    }

    /// Override the configuration used for this provider
    pub fn configure(mut self, config: ProviderConfig) -> Self {
        self.region_chain = self.region_chain.configure(&config);
        self.conf = Some(config);
        self
    }

    /// Creates a `DefaultCredentialsChain`
    ///
    /// ## Panics
    /// This function will panic if no connector has been set or the `default-https-client`
    /// feature has been disabled.
    pub async fn build(self) -> DefaultCredentialsChain {
        let region = match self.region_override {
            Some(provider) => provider.region().await,
            None => self.region_chain.build().region().await,
        };

        let conf = self.conf.unwrap_or_default().with_region(region);

        let env_provider = EnvironmentVariableCredentialsProvider::new_with_env(conf.env());
        let profile_provider = self.profile_file_builder.configure(&conf).build();
        let web_identity_token_provider = self.web_identity_builder.configure(&conf).build();
        let imds_provider = self.imds_builder.configure(&conf).build();
        let ecs_provider = self.ecs_builder.configure(&conf).build();

        let provider_chain = CredentialsProviderChain::first_try("Environment", env_provider)
            .or_else("Profile", profile_provider)
            .or_else("WebIdentityToken", web_identity_token_provider)
            .or_else("EcsContainer", ecs_provider)
            .or_else("Ec2InstanceMetadata", imds_provider);

        DefaultCredentialsChain { provider_chain }
    }
}
