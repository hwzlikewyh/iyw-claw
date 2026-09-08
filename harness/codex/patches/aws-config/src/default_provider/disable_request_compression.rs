/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::environment::parse_bool;
use crate::provider_config::ProviderConfig;
use aws_runtime::env_config::EnvConfigValue;
use aws_smithy_types::error::display::DisplayErrorContext;

mod env {
    pub(super) const DISABLE_REQUEST_COMPRESSION: &str = "AWS_DISABLE_REQUEST_COMPRESSION";
}

mod profile_key {
    pub(super) const DISABLE_REQUEST_COMPRESSION: &str = "disable_request_compression";
}

/// Load the value for "disable request compression".
///
/// This checks the following sources:
/// 1. The environment variable `AWS_DISABLE_REQUEST_COMPRESSION=true/false`
/// 2. The profile key `disable_request_compression=true/false`
///
/// If invalid values are found, the provider will return None and an error will be logged.
pub(crate) async fn disable_request_compression_provider(
    provider_config: &ProviderConfig,
) -> Option<bool> {
    let env = provider_config.env();
    let profiles = provider_config.profile().await;

    EnvConfigValue::new()
        .env(env::DISABLE_REQUEST_COMPRESSION)
        .profile(profile_key::DISABLE_REQUEST_COMPRESSION)
        .validate(&env, profiles, parse_bool)
        .map_err(
            |err| tracing::warn!(err = %DisplayErrorContext(&err), "invalid value for `disable request compression` setting"),
        )
        .unwrap_or(None)
}
