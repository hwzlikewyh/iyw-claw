/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::environment::parse_bool;
use crate::provider_config::ProviderConfig;
use aws_runtime::env_config::EnvConfigValue;
use aws_smithy_types::error::display::DisplayErrorContext;

mod env {
    pub(super) const USE_FIPS: &str = "AWS_USE_FIPS_ENDPOINT";
}

mod profile_key {
    pub(super) const USE_FIPS: &str = "use_fips_endpoint";
}

/// Load the value for "use FIPS"
///
/// This checks the following sources:
/// 1. The environment variable `AWS_USE_FIPS_ENDPOINT=true/false`
/// 2. The profile key `use_fips_endpoint=true/false`
///
/// If invalid values are found, the provider will return None and an error will be logged.
pub async fn use_fips_provider(provider_config: &ProviderConfig) -> Option<bool> {
    let env = provider_config.env();
    let profiles = provider_config.profile().await;

    EnvConfigValue::new()
        .env(env::USE_FIPS)
        .profile(profile_key::USE_FIPS)
        .validate(&env, profiles, parse_bool)
        .map_err(
            |err| tracing::warn!(err = %DisplayErrorContext(&err), "invalid value for FIPS setting"),
        )
        .unwrap_or(None)
}
