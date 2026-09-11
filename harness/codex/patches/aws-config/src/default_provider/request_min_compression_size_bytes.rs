/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::environment::parse_uint;
use crate::provider_config::ProviderConfig;
use aws_runtime::env_config::EnvConfigValue;
use aws_smithy_types::error::display::DisplayErrorContext;

mod env {
    pub(super) const REQUEST_MIN_COMPRESSION_SIZE_BYTES: &str =
        "AWS_REQUEST_MIN_COMPRESSION_SIZE_BYTES";
}

mod profile_key {
    pub(super) const REQUEST_MIN_COMPRESSION_SIZE_BYTES: &str =
        "request_min_compression_size_bytes";
}

/// Load the value for "request minimum compression size bytes".
///
/// This checks the following sources:
/// 1. The environment variable `AWS_REQUEST_MIN_COMPRESSION_SIZE_BYTES=10240`
/// 2. The profile key `request_min_compression_size_bytes=10240`
///
/// If invalid values are found, the provider will return None and an error will be logged.
pub(crate) async fn request_min_compression_size_bytes_provider(
    provider_config: &ProviderConfig,
) -> Option<u32> {
    let env = provider_config.env();
    let profiles = provider_config.profile().await;

    EnvConfigValue::new()
        .env(env::REQUEST_MIN_COMPRESSION_SIZE_BYTES)
        .profile(profile_key::REQUEST_MIN_COMPRESSION_SIZE_BYTES)
        .validate(&env, profiles, parse_uint)
        .map_err(
            |err| tracing::warn!(err = %DisplayErrorContext(&err), "invalid value for `request minimum compression size bytes` setting"),
        )
        .unwrap_or(None)
}
