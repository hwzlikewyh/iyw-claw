/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::provider_config::ProviderConfig;
use crate::retry::error::{RetryConfigError, RetryConfigErrorKind};
use aws_runtime::env_config::{EnvConfigError, EnvConfigValue};
use aws_smithy_types::error::display::DisplayErrorContext;
use aws_smithy_types::retry::{RetryConfig, RetryMode, RetrySpec};
use std::str::FromStr;

/// Default RetryConfig Provider chain
///
/// Unlike other "providers" `RetryConfig` has no related `RetryConfigProvider` trait. Instead,
/// a builder struct is returned which has a similar API.
///
/// This provider will check the following sources in order:
/// 1. Environment variables: `AWS_MAX_ATTEMPTS` & `AWS_RETRY_MODE`
/// 2. Profile file: `max_attempts` and `retry_mode`
///
/// # Example
///
/// When running [`aws_config::from_env()`](crate::from_env()), a [`ConfigLoader`](crate::ConfigLoader)
/// is created that will then create a [`RetryConfig`] from the default_provider. There is no
/// need to call `default_provider` and the example below is only for illustration purposes.
///
/// ```no_run
/// # use std::error::Error;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn Error>> {
/// use aws_config::default_provider::retry_config;
///
/// // Load a retry config from a specific profile
/// let retry_config = retry_config::default_provider()
///     .profile_name("other_profile")
///     .retry_config()
///     .await;
/// let config = aws_config::from_env()
///     // Override the retry config set by the default profile
///     .retry_config(retry_config)
///     .load()
///     .await;
/// // instantiate a service client:
/// // <my_aws_service>::Client::new(&config);
/// #     Ok(())
/// # }
/// ```
pub fn default_provider() -> Builder {
    Builder::default()
}

mod env {
    pub(super) const MAX_ATTEMPTS: &str = "AWS_MAX_ATTEMPTS";
    pub(super) const RETRY_MODE: &str = "AWS_RETRY_MODE";
    pub(super) const NEW_RETRIES_2026: &str = "AWS_NEW_RETRIES_2026";
}

mod profile_keys {
    pub(super) const MAX_ATTEMPTS: &str = "max_attempts";
    pub(super) const RETRY_MODE: &str = "retry_mode";
}

/// Builder for RetryConfig that checks the environment and aws profile for configuration
#[derive(Debug, Default)]
pub struct Builder {
    provider_config: ProviderConfig,
}

impl Builder {
    /// Configure the default chain
    ///
    /// Exposed for overriding the environment when unit-testing providers
    pub fn configure(mut self, configuration: &ProviderConfig) -> Self {
        self.provider_config = configuration.clone();
        self
    }

    /// Override the profile name used by this provider
    pub fn profile_name(mut self, name: &str) -> Self {
        self.provider_config = self.provider_config.with_profile_name(name.to_string());
        self
    }

    /// Attempt to create a [`RetryConfig`] from following sources in order:
    /// 1. Environment variables: `AWS_MAX_ATTEMPTS` & `AWS_RETRY_MODE`
    /// 2. Profile file: `max_attempts` and `retry_mode`
    /// 3. [RetryConfig::standard()](aws_smithy_types::retry::RetryConfig::standard)
    ///
    /// Precedence is considered on a per-field basis
    ///
    /// # Panics
    ///
    /// - Panics if the `AWS_MAX_ATTEMPTS` env var or `max_attempts` profile var is set to 0
    /// - Panics if the `AWS_RETRY_MODE` env var or `retry_mode` profile var is set to "adaptive" (it's not yet supported)
    pub async fn retry_config(self) -> RetryConfig {
        match self.try_retry_config().await {
            Ok(conf) => conf,
            Err(e) => panic!("{}", DisplayErrorContext(e)),
        }
    }

    pub(crate) async fn try_retry_config(
        self,
    ) -> Result<RetryConfig, EnvConfigError<RetryConfigError>> {
        let env = self.provider_config.env();
        let profiles = self.provider_config.profile().await;
        // Both of these can return errors due to invalid config settings, and we want to surface those as early as possible
        // hence, we'll panic if any config values are invalid (missing values are OK though)
        // We match this instead of unwrapping, so we can print the error with the `Display` impl instead of the `Debug` impl that unwrap uses
        let mut retry_config = RetryConfig::standard();
        let max_attempts = EnvConfigValue::new()
            .env(env::MAX_ATTEMPTS)
            .profile(profile_keys::MAX_ATTEMPTS)
            .validate(&env, profiles, validate_max_attempts);

        let retry_mode = EnvConfigValue::new()
            .env(env::RETRY_MODE)
            .profile(profile_keys::RETRY_MODE)
            .validate(&env, profiles, |s| {
                RetryMode::from_str(s)
                    .map_err(|err| RetryConfigErrorKind::InvalidRetryMode { source: err }.into())
            });

        if let Some(max_attempts) = max_attempts? {
            retry_config = retry_config.with_max_attempts(max_attempts);
        }

        if let Some(retry_mode) = retry_mode? {
            retry_config = retry_config.with_retry_mode(retry_mode);
        }

        // Enable Retry Behavior 2.1 when AWS_NEW_RETRIES_2026=true
        let new_retries =
            EnvConfigValue::new()
                .env(env::NEW_RETRIES_2026)
                .validate(&env, profiles, |s| Ok::<_, RetryConfigError>(s.to_owned()));
        if let Some(val) = new_retries? {
            if val.eq_ignore_ascii_case("true") {
                retry_config = retry_config.with_retry_spec(RetrySpec::v2_1());
            }
        }

        Ok(retry_config)
    }
}

fn validate_max_attempts(max_attempts: &str) -> Result<u32, RetryConfigError> {
    match max_attempts.parse::<u32>() {
        Ok(0) => Err(RetryConfigErrorKind::MaxAttemptsMustNotBeZero.into()),
        Ok(max_attempts) => Ok(max_attempts),
        Err(source) => Err(RetryConfigErrorKind::FailedToParseMaxAttempts { source }.into()),
    }
}
