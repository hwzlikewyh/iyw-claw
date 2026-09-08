/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::environment::parse_bool;
use crate::provider_config::ProviderConfig;
use aws_runtime::env_config::EnvConfigValue;
use aws_smithy_types::error::display::DisplayErrorContext;

mod env {
    pub(super) const IGNORE_CONFIGURED_ENDPOINT_URLS: &str = "AWS_IGNORE_CONFIGURED_ENDPOINT_URLS";
}

mod profile_key {
    pub(super) const IGNORE_CONFIGURED_ENDPOINT_URLS: &str = "ignore_configured_endpoint_urls";
}

/// Load the value for "ignore configured endpoint URLs"
///
/// This checks the following sources:
/// 1. The environment variable `AWS_IGNORE_CONFIGURED_ENDPOINT_URLS_ENDPOINT=true/false`
/// 2. The profile key `ignore_configured_endpoint_urls=true/false`
///
/// If invalid values are found, the provider will return None and an error will be logged.
pub async fn ignore_configured_endpoint_urls_provider(
    provider_config: &ProviderConfig,
) -> Option<bool> {
    let env = provider_config.env();
    let profiles = provider_config.profile().await;

    EnvConfigValue::new()
        .env(env::IGNORE_CONFIGURED_ENDPOINT_URLS)
        .profile(profile_key::IGNORE_CONFIGURED_ENDPOINT_URLS)
        .validate(&env, profiles, parse_bool)
        .map_err(
            |err| tracing::warn!(err = %DisplayErrorContext(&err), "invalid value for 'ignore configured endpoint URLs' setting"),
        )
        .unwrap_or(None)
}
