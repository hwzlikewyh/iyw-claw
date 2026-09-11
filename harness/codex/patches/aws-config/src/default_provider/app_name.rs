/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::provider_config::ProviderConfig;
use aws_runtime::env_config::{EnvConfigError, EnvConfigValue};
use aws_smithy_types::error::display::DisplayErrorContext;
use aws_types::app_name::{AppName, InvalidAppName};

/// Default App Name Provider chain
///
/// This provider will check the following sources in order:
/// 1. Environment variables: `AWS_SDK_UA_APP_ID`
/// 2. Profile files from the key `sdk_ua_app_id`
///
#[doc = include_str!("../profile/location_of_profile_files.md")]
///
/// # Examples
///
/// **Loads "my-app" as the app name**
/// ```ini
/// [default]
/// sdk_ua_app_id = my-app
/// ```
///
/// **Loads "my-app" as the app name _if and only if_ the `AWS_PROFILE` environment variable
/// is set to `other`.**
/// ```ini
/// [profile other]
/// sdk_ua_app_id = my-app
/// ```
pub fn default_provider() -> Builder {
    Builder::default()
}

/// Default provider builder for [`AppName`]
#[derive(Debug, Default)]
pub struct Builder {
    provider_config: ProviderConfig,
}

impl Builder {
    /// Configure the default chain
    ///
    /// Exposed for overriding the environment when unit-testing providers
    pub(crate) fn configure(self, configuration: &ProviderConfig) -> Self {
        Self {
            provider_config: configuration.clone(),
        }
    }

    /// Override the profile name used by this provider
    pub fn profile_name(mut self, name: &str) -> Self {
        self.provider_config = self.provider_config.with_profile_name(name.to_string());
        self
    }

    async fn fallback_app_name(&self) -> Result<Option<AppName>, EnvConfigError<InvalidAppName>> {
        let env = self.provider_config.env();
        let profiles = self.provider_config.profile().await;

        EnvConfigValue::new()
            .profile("sdk-ua-app-id")
            .validate(&env, profiles, |name| AppName::new(name.to_string()))
    }

    /// Build an [`AppName`] from the default chain
    pub async fn app_name(self) -> Option<AppName> {
        let env = self.provider_config.env();
        let profiles = self.provider_config.profile().await;

        let standard = EnvConfigValue::new()
            .env("AWS_SDK_UA_APP_ID")
            .profile("sdk_ua_app_id")
            .validate(&env, profiles, |name| AppName::new(name.to_string()));
        let with_fallback = match standard {
            Ok(None) => self.fallback_app_name().await,
            other => other,
        };

        with_fallback.map_err(
                |err| tracing::warn!(err = %DisplayErrorContext(&err), "invalid value for App Name setting"),
            )
            .unwrap_or(None)
    }
}
