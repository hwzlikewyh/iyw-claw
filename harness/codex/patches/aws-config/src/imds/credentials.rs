/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! IMDSv2 Credentials Provider
//!
//! # Important
//! This credential provider will NOT fallback to IMDSv1. Ensure that IMDSv2 is enabled on your instances.

use super::client::error::ImdsError;
use crate::imds::{self, Client};
use crate::json_credentials::{parse_json_credentials, JsonCredentials, RefreshableCredentials};
use crate::provider_config::ProviderConfig;
use aws_credential_types::credential_feature::AwsCredentialFeature;
use aws_credential_types::provider::{self, error::CredentialsError, future, ProvideCredentials};
use aws_credential_types::Credentials;
use aws_smithy_async::time::SharedTimeSource;
use aws_types::os_shim_internal::Env;
use std::borrow::Cow;
use std::error::Error as StdError;
use std::fmt;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

const CREDENTIAL_EXPIRATION_INTERVAL: Duration = Duration::from_secs(10 * 60);
const WARNING_FOR_EXTENDING_CREDENTIALS_EXPIRY: &str =
    "Attempting credential expiration extension due to a credential service availability issue. \
    A refresh of these credentials will be attempted again within the next";

#[derive(Debug)]
struct ImdsCommunicationError {
    source: Box<dyn StdError + Send + Sync + 'static>,
}

impl fmt::Display for ImdsCommunicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "could not communicate with IMDS")
    }
}

impl StdError for ImdsCommunicationError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self.source.as_ref())
    }
}

/// IMDSv2 Credentials Provider
///
/// _Note: This credentials provider will NOT fallback to the IMDSv1 flow._
#[derive(Debug)]
pub struct ImdsCredentialsProvider {
    client: Client,
    env: Env,
    profile: Option<String>,
    time_source: SharedTimeSource,
    last_retrieved_credentials: Arc<RwLock<Option<Credentials>>>,
}

/// Builder for [`ImdsCredentialsProvider`]
#[derive(Default, Debug)]
pub struct Builder {
    provider_config: Option<ProviderConfig>,
    profile_override: Option<String>,
    imds_override: Option<imds::Client>,
    last_retrieved_credentials: Option<Credentials>,
}

impl Builder {
    /// Override the configuration used for this provider
    pub fn configure(mut self, provider_config: &ProviderConfig) -> Self {
        self.provider_config = Some(provider_config.clone());
        self
    }

    /// Override the [instance profile](instance-profile) used for this provider.
    ///
    /// When retrieving IMDS credentials, a call must first be made to
    /// `<IMDS_BASE_URL>/latest/meta-data/iam/security-credentials/`. This returns the instance
    /// profile used. By setting this parameter, retrieving the profile is skipped
    /// and the provided value is used instead.
    ///
    /// [instance-profile]: https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/iam-roles-for-amazon-ec2.html#ec2-instance-profile
    pub fn profile(mut self, profile: impl Into<String>) -> Self {
        self.profile_override = Some(profile.into());
        self
    }

    /// Override the IMDS client used for this provider
    ///
    /// The IMDS client will be loaded and configured via `~/.aws/config` and environment variables,
    /// however, if necessary the entire client may be provided directly.
    ///
    /// For more information about IMDS client configuration loading see [`imds::Client`]
    pub fn imds_client(mut self, client: imds::Client) -> Self {
        self.imds_override = Some(client);
        self
    }

    #[allow(dead_code)]

    /// Create an [`ImdsCredentialsProvider`] from this builder.
    pub fn build(self) -> ImdsCredentialsProvider {
        let provider_config = self.provider_config.unwrap_or_default();
        let env = provider_config.env();
        let client = self
            .imds_override
            .unwrap_or_else(|| imds::Client::builder().configure(&provider_config).build());
        ImdsCredentialsProvider {
            client,
            env,
            profile: self.profile_override,
            time_source: provider_config.time_source(),
            last_retrieved_credentials: Arc::new(RwLock::new(self.last_retrieved_credentials)),
        }
    }
}

mod codes {
    pub(super) const ASSUME_ROLE_UNAUTHORIZED_ACCESS: &str = "AssumeRoleUnauthorizedAccess";
}

impl ProvideCredentials for ImdsCredentialsProvider {
    fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        future::ProvideCredentials::new(self.credentials())
    }

    fn fallback_on_interrupt(&self) -> Option<Credentials> {
        self.last_retrieved_credentials.read().unwrap().clone()
    }
}

impl ImdsCredentialsProvider {
    /// Builder for [`ImdsCredentialsProvider`]
    pub fn builder() -> Builder {
        Builder::default()
    }

    fn imds_disabled(&self) -> bool {
        match self.env.get(super::env::EC2_METADATA_DISABLED) {
            Ok(value) => value.eq_ignore_ascii_case("true"),
            _ => false,
        }
    }

    /// Retrieve the instance profile from IMDS
    async fn get_profile_uncached(&self) -> Result<String, CredentialsError> {
        match self
            .client
            .get("/latest/meta-data/iam/security-credentials/")
            .await
        {
            Ok(profile) => Ok(profile.as_ref().into()),
            Err(ImdsError::ErrorResponse(context))
                if context.response().status().as_u16() == 404 =>
            {
                tracing::warn!(
                    "received 404 from IMDS when loading profile information. \
                    Hint: This instance may not have an IAM role associated."
                );
                Err(CredentialsError::not_loaded("received 404 from IMDS"))
            }
            Err(ImdsError::FailedToLoadToken(context)) if context.is_dispatch_failure() => {
                Err(CredentialsError::not_loaded(ImdsCommunicationError {
                    source: context.into_source().into(),
                }))
            }
            Err(other) => Err(CredentialsError::provider_error(other)),
        }
    }

    // Extend the cached expiration time if necessary
    //
    // This allows continued use of the credentials even when IMDS returns expired ones.
    fn maybe_extend_expiration(&self, expiration: SystemTime) -> SystemTime {
        let now = self.time_source.now();
        // If credentials from IMDS are not stale, use them as they are.
        if now < expiration {
            return expiration;
        }

        let mut rng = fastrand::Rng::with_seed(
            now.duration_since(SystemTime::UNIX_EPOCH)
                .expect("now should be after UNIX EPOCH")
                .as_secs(),
        );
        // Calculate credentials' refresh offset with jitter, which should be less than 15 minutes
        // the smallest amount of time credentials are valid for.
        // Setting it to something longer than that may have the risk of the credentials expiring
        // before the next refresh.
        let refresh_offset = CREDENTIAL_EXPIRATION_INTERVAL + Duration::from_secs(rng.u64(0..=300));
        let new_expiry = now + refresh_offset;

        tracing::warn!(
            "{WARNING_FOR_EXTENDING_CREDENTIALS_EXPIRY} {:.2} minutes.",
            refresh_offset.as_secs_f64() / 60.0,
        );

        new_expiry
    }

    async fn retrieve_credentials(&self) -> provider::Result {
        if self.imds_disabled() {
            let err = format!(
                "IMDS disabled by {} env var set to `true`",
                super::env::EC2_METADATA_DISABLED
            );
            tracing::debug!(err);
            return Err(CredentialsError::not_loaded(err));
        }
        tracing::debug!("loading credentials from IMDS");
        let profile: Cow<'_, str> = match &self.profile {
            Some(profile) => profile.into(),
            None => self.get_profile_uncached().await?.into(),
        };
        tracing::debug!(profile = %profile, "loaded profile");
        let credentials = self
            .client
            .get(format!(
                "/latest/meta-data/iam/security-credentials/{profile}",
            ))
            .await
            .map_err(CredentialsError::provider_error)?;
        match parse_json_credentials(credentials.as_ref()) {
            Ok(JsonCredentials::RefreshableCredentials(RefreshableCredentials {
                access_key_id,
                secret_access_key,
                session_token,
                account_id,
                expiration,
                ..
            })) => {
                // TODO(IMDSv2.X): Use `account_id` once the design is finalized
                let _ = account_id;
                let expiration = self.maybe_extend_expiration(expiration);
                let creds = Credentials::new(
                    access_key_id,
                    secret_access_key,
                    Some(session_token.to_string()),
                    expiration.into(),
                    "IMDSv2",
                );
                *self.last_retrieved_credentials.write().unwrap() = Some(creds.clone());
                Ok(creds)
            }
            Ok(JsonCredentials::Error { code, message })
                if code == codes::ASSUME_ROLE_UNAUTHORIZED_ACCESS =>
            {
                Err(CredentialsError::invalid_configuration(format!(
                    "Incorrect IMDS/IAM configuration: [{code}] {message}. \
                        Hint: Does this role have a trust relationship with EC2?",
                )))
            }
            Ok(JsonCredentials::Error { code, message }) => Err(CredentialsError::provider_error(
                format!("Error retrieving credentials from IMDS: {code} {message}"),
            )),
            // got bad data from IMDS, should not occur during normal operation:
            Err(invalid) => Err(CredentialsError::unhandled(invalid)),
        }
        .map(|mut creds| {
            creds
                .get_property_mut_or_default::<Vec<AwsCredentialFeature>>()
                .push(AwsCredentialFeature::CredentialsImds);
            creds
        })
    }

    async fn credentials(&self) -> provider::Result {
        match self.retrieve_credentials().await {
            creds @ Ok(_) => creds,
            // Any failure while retrieving credentials MUST NOT impede use of existing credentials.
            err => match &*self.last_retrieved_credentials.read().unwrap() {
                Some(creds) => Ok(creds.clone()),
                _ => err,
            },
        }
    }
}
