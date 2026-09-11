/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use std::env::VarError;

use aws_credential_types::attributes::AccountId;
use aws_credential_types::credential_feature::AwsCredentialFeature;
use aws_credential_types::provider::{self, error::CredentialsError, future, ProvideCredentials};
use aws_credential_types::Credentials;
use aws_types::os_shim_internal::Env;

/// Load Credentials from Environment Variables
///
/// `EnvironmentVariableCredentialsProvider` uses the following variables:
/// - `AWS_ACCESS_KEY_ID`
/// - `AWS_SECRET_ACCESS_KEY` with fallback to `SECRET_ACCESS_KEY`
/// - `AWS_SESSION_TOKEN` (optional)
/// - `AWS_ACCOUNT_ID` (optional)
#[derive(Debug, Clone)]
pub struct EnvironmentVariableCredentialsProvider {
    env: Env,
}

impl EnvironmentVariableCredentialsProvider {
    fn credentials(&self) -> provider::Result {
        let access_key = self
            .env
            .get("AWS_ACCESS_KEY_ID")
            .and_then(err_if_blank)
            .map_err(to_cred_error)?;
        let secret_key = self
            .env
            .get("AWS_SECRET_ACCESS_KEY")
            .and_then(err_if_blank)
            .or_else(|_| self.env.get("SECRET_ACCESS_KEY"))
            .and_then(err_if_blank)
            .map_err(to_cred_error)?;
        let session_token =
            self.env
                .get("AWS_SESSION_TOKEN")
                .ok()
                .and_then(|token| match token.trim() {
                    "" => None,
                    s => Some(s.to_string()),
                });
        let account_id =
            self.env
                .get("AWS_ACCOUNT_ID")
                .ok()
                .and_then(|account_id| match account_id.trim() {
                    "" => None,
                    s => Some(AccountId::from(s)),
                });
        let mut builder = Credentials::builder()
            .access_key_id(access_key)
            .secret_access_key(secret_key)
            .provider_name(ENV_PROVIDER);
        builder.set_session_token(session_token);
        builder.set_account_id(account_id);
        let mut creds = builder.build();
        creds
            .get_property_mut_or_default::<Vec<AwsCredentialFeature>>()
            .push(AwsCredentialFeature::CredentialsEnvVars);
        Ok(creds)
    }
}

impl EnvironmentVariableCredentialsProvider {
    /// Create a `EnvironmentVariableCredentialsProvider`
    pub fn new() -> Self {
        Self::new_with_env(Env::real())
    }

    /// Create a new `EnvironmentVariableCredentialsProvider` with `Env` overridden
    ///
    /// This function is intended for tests that mock out the process environment.
    pub(crate) fn new_with_env(env: Env) -> Self {
        Self { env }
    }
}

impl Default for EnvironmentVariableCredentialsProvider {
    fn default() -> Self {
        Self::new()
    }
}

const ENV_PROVIDER: &str = "EnvironmentVariable";

impl ProvideCredentials for EnvironmentVariableCredentialsProvider {
    fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        future::ProvideCredentials::ready(self.credentials())
    }
}

fn to_cred_error(err: VarError) -> CredentialsError {
    match err {
        VarError::NotPresent => CredentialsError::not_loaded("environment variable not set"),
        e @ VarError::NotUnicode(_) => CredentialsError::unhandled(e),
    }
}

fn err_if_blank(value: String) -> Result<String, VarError> {
    if value.trim().is_empty() {
        Err(VarError::NotPresent)
    } else {
        Ok(value)
    }
}
