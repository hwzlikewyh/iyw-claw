/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use crate::provider_config::ProviderConfig;
use aws_runtime::env_config::EnvConfigValue;
use aws_smithy_types::error::display::DisplayErrorContext;
use aws_types::region::SigningRegionSet;
use std::fmt;

mod env {
    pub(super) const SIGV4A_SIGNING_REGION_SET: &str = "AWS_SIGV4A_SIGNING_REGION_SET";
}

mod profile_key {
    pub(super) const SIGV4A_SIGNING_REGION_SET: &str = "sigv4a_signing_region_set";
}

/// Load the value for the SigV4a signing region set.
///
/// Checks `AWS_SIGV4A_SIGNING_REGION_SET` env var, then `sigv4a_signing_region_set` profile key.
/// The value is a comma-delimited list of region names.
pub(crate) async fn sigv4a_signing_region_set_provider(
    provider_config: &ProviderConfig,
) -> Option<SigningRegionSet> {
    let env = provider_config.env();
    let profiles = provider_config.profile().await;

    EnvConfigValue::new()
        .env(env::SIGV4A_SIGNING_REGION_SET)
        .profile(profile_key::SIGV4A_SIGNING_REGION_SET)
        .validate(&env, profiles, parse_signing_region_set)
        .map_err(|err| {
            tracing::warn!(
                err = %DisplayErrorContext(&err),
                "invalid value for sigv4a signing region set"
            )
        })
        .unwrap_or(None)
}

fn parse_signing_region_set(csv: &str) -> Result<SigningRegionSet, InvalidSigningRegionSet> {
    let region_set: SigningRegionSet = csv
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if region_set.as_ref().is_empty() {
        return Err(InvalidSigningRegionSet {
            value: format!("Empty value in `{csv}`."),
        });
    }
    Ok(region_set)
}

#[derive(Debug)]
struct InvalidSigningRegionSet {
    value: String,
}

impl fmt::Display for InvalidSigningRegionSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Not a valid signing region set: {}", self.value)
    }
}

impl std::error::Error for InvalidSigningRegionSet {}
