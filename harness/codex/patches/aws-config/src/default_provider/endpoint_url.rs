/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::environment::parse_url;
use crate::provider_config::ProviderConfig;
use aws_runtime::env_config::EnvConfigValue;
use aws_smithy_types::error::display::DisplayErrorContext;
use aws_types::origin::Origin;

mod env {
    pub(super) const ENDPOINT_URL: &str = "AWS_ENDPOINT_URL";
}

mod profile_key {
    pub(super) const ENDPOINT_URL: &str = "endpoint_url";
}

/// Load the value for an endpoint URL
///
/// This checks the following sources:
/// 1. The environment variable `AWS_ENDPOINT_URL=http://localhost`
/// 2. The profile key `endpoint_url=http://localhost`
///
/// If invalid values are found, the provider will return None and an error will be logged.
pub async fn endpoint_url_provider(provider_config: &ProviderConfig) -> Option<String> {
    let env = provider_config.env();
    let profiles = provider_config.profile().await;

    EnvConfigValue::new()
        .env(env::ENDPOINT_URL)
        .profile(profile_key::ENDPOINT_URL)
        .validate(&env, profiles, parse_url)
        .map_err(
            |err| tracing::warn!(err = %DisplayErrorContext(&err), "invalid value for endpoint URL setting"),
        )
        .unwrap_or(None)
}

/// Load the value for an endpoint URL
///
/// This checks the following sources:
/// 1. The environment variable `AWS_ENDPOINT_URL=http://localhost`
/// 2. The profile key `endpoint_url=http://localhost`
///
/// If invalid values are found, the provider will return None and an error will be logged.
pub async fn endpoint_url_provider_with_origin(
    provider_config: &ProviderConfig,
) -> (Option<String>, Origin) {
    let env = provider_config.env();
    let profiles = provider_config.profile().await;

    EnvConfigValue::new()
        .env(env::ENDPOINT_URL)
        .profile(profile_key::ENDPOINT_URL)
        .validate_and_return_origin(&env, profiles, parse_url)
        .map_err(
            |err| tracing::warn!(err = %DisplayErrorContext(&err), "invalid value for endpoint URL setting"),
        )
        .unwrap_or_default()
}
