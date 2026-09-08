/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::provider_config::ProviderConfig;
use aws_runtime::env_config::EnvConfigValue;
use aws_smithy_types::error::display::DisplayErrorContext;
use aws_types::endpoint_config::AccountIdEndpointMode;
use std::str::FromStr;

mod env {
    pub(super) const ACCOUNT_ID_ENDPOINT_MODE: &str = "AWS_ACCOUNT_ID_ENDPOINT_MODE";
}

mod profile_key {
    pub(super) const ACCOUNT_ID_ENDPOINT_MODE: &str = "account_id_endpoint_mode";
}

/// Load the value for the Account-based endpoint mode
///
/// This checks the following sources:
/// 1. The environment variable `AWS_ACCOUNT_ID_ENDPOINT_MODE=preferred/disabled/required`
/// 2. The profile key `account_id_endpoint_mode=preferred/disabled/required`
///
/// If invalid values are found, the provider will return `None` and an error will be logged.
pub(crate) async fn account_id_endpoint_mode_provider(
    provider_config: &ProviderConfig,
) -> Option<AccountIdEndpointMode> {
    let env = provider_config.env();
    let profiles = provider_config.profile().await;

    EnvConfigValue::new()
        .env(env::ACCOUNT_ID_ENDPOINT_MODE)
        .profile(profile_key::ACCOUNT_ID_ENDPOINT_MODE)
        .validate(&env, profiles, AccountIdEndpointMode::from_str)
        .map_err(|err| tracing::warn!(err = %DisplayErrorContext(&err), "invalid value for `AccountIdEndpointMode`"))
        .unwrap_or(None)
}
