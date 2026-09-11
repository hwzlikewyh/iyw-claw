/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::environment::parse_bool;
use crate::provider_config::ProviderConfig;
use aws_runtime::env_config::EnvConfigValue;
use aws_smithy_types::error::display::DisplayErrorContext;

mod env {
    pub(super) const USE_DUAL_STACK: &str = "AWS_USE_DUALSTACK_ENDPOINT";
}

mod profile_key {
    pub(super) const USE_DUAL_STACK: &str = "use_dualstack_endpoint";
}

/// Load the value for "use dual-stack"
///
/// This checks the following sources:
/// 1. The environment variable `AWS_USE_DUALSTACK_ENDPOINT=true/false`
/// 2. The profile key `use_dualstack_endpoint=true/false`
///
/// If invalid values are found, the provider will return `None` and an error will be logged.
pub async fn use_dual_stack_provider(provider_config: &ProviderConfig) -> Option<bool> {
    let env = provider_config.env();
    let profiles = provider_config.profile().await;

    EnvConfigValue::new()
        .env(env::USE_DUAL_STACK)
        .profile(profile_key::USE_DUAL_STACK)
        .validate(&env, profiles, parse_bool)
        .map_err(
            |err| tracing::warn!(err = %DisplayErrorContext(&err), "invalid value for dual-stack setting"),
        )
        .unwrap_or(None)
}
