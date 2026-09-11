/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

//! Raw IMDSv2 Client
//!
//! Client for direct access to IMDSv2.

use crate::imds::client::error::{BuildError, ImdsError, InnerImdsError, InvalidEndpointMode};
use crate::imds::client::token::TokenRuntimePlugin;
use crate::provider_config::ProviderConfig;
use crate::PKG_VERSION;
use aws_runtime::user_agent::{ApiMetadata, AwsUserAgent, UserAgentInterceptor};
use aws_smithy_runtime::client::metrics::MetricsRuntimePlugin;
use aws_smithy_runtime::client::orchestrator::operation::Operation;
use aws_smithy_runtime::client::retries::strategy::StandardRetryStrategy;
use aws_smithy_runtime_api::box_error::BoxError;
use aws_smithy_runtime_api::client::auth::AuthSchemeOptionResolverParams;
use aws_smithy_runtime_api::client::endpoint::{
    EndpointFuture, EndpointResolverParams, ResolveEndpoint,
};
use aws_smithy_runtime_api::client::interceptors::context::InterceptorContext;
use aws_smithy_runtime_api::client::orchestrator::{
    HttpRequest, Metadata, OrchestratorError, SensitiveOutput,
};
use aws_smithy_runtime_api::client::result::ConnectorError;
use aws_smithy_runtime_api::client::result::SdkError;
use aws_smithy_runtime_api::client::retries::classifiers::{
    ClassifyRetry, RetryAction, SharedRetryClassifier,
};
use aws_smithy_runtime_api::client::runtime_components::RuntimeComponentsBuilder;
use aws_smithy_runtime_api::client::runtime_plugin::{RuntimePlugin, SharedRuntimePlugin};
use aws_smithy_types::body::SdkBody;
use aws_smithy_types::config_bag::{FrozenLayer, Layer};
use aws_smithy_types::endpoint::Endpoint;
use aws_smithy_types::retry::RetryConfig;
use aws_smithy_types::timeout::TimeoutConfig;
use aws_types::os_shim_internal::Env;
use http::Uri;
use std::borrow::Cow;
use std::error::Error as _;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

pub mod error;
mod token;

// 6 hours
const DEFAULT_TOKEN_TTL: Duration = Duration::from_secs(21_600);
const DEFAULT_ATTEMPTS: u32 = 4;
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(1);
const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_OPERATION_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);

fn user_agent() -> AwsUserAgent {
    AwsUserAgent::new_from_environment(Env::real(), ApiMetadata::new("imds", PKG_VERSION))
}

/// IMDSv2 Client
///
/// Client for IMDSv2. This client handles fetching tokens, retrying on failure, and token
/// caching according to the specified token TTL.
///
/// _Note: This client ONLY supports IMDSv2. It will not fallback to IMDSv1. See
/// [transitioning to IMDSv2](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/configuring-instance-metadata-service.html#instance-metadata-transition-to-version-2)
/// for more information._
///
/// **Note**: When running in a Docker container, all network requests will incur an additional hop. When combined with the default IMDS hop limit of 1, this will cause requests to IMDS to timeout! To fix this issue, you'll need to set the following instance metadata settings :
/// ```txt
/// amazonec2-metadata-token=required
/// amazonec2-metadata-token-response-hop-limit=2
/// ```
///
/// On an instance that is already running, these can be set with [ModifyInstanceMetadataOptions](https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_ModifyInstanceMetadataOptions.html). On a new instance, these can be set with the `MetadataOptions` field on [RunInstances](https://docs.aws.amazon.com/AWSEC2/latest/APIReference/API_RunInstances.html).
///
/// For more information about IMDSv2 vs. IMDSv1 see [this guide](https://docs.aws.amazon.com/AWSEC2/latest/WindowsGuide/configuring-instance-metadata-service.html)
///
/// # Client Configuration
/// The IMDS client can load configuration explicitly, via environment variables, or via
/// `~/.aws/config`. It will first attempt to resolve an endpoint override. If no endpoint
/// override exists, it will attempt to resolve an [`EndpointMode`]. If no
/// [`EndpointMode`] override exists, it will fallback to [`IpV4`](EndpointMode::IpV4). An exhaustive
/// list is below:
///
/// ## Endpoint configuration list
/// 1. Explicit configuration of `Endpoint` via the [builder](Builder):
/// ```no_run
/// use aws_config::imds::client::Client;
/// # async fn docs() {
/// let client = Client::builder()
///   .endpoint("http://customimds:456/").expect("valid URI")
///   .build();
/// # }
/// ```
///
/// 2. The `AWS_EC2_METADATA_SERVICE_ENDPOINT` environment variable. Note: If this environment variable
///    is set, it MUST contain a valid URI or client construction will fail.
///
/// 3. The `ec2_metadata_service_endpoint` field in `~/.aws/config`:
/// ```ini
/// [default]
/// # ... other configuration
/// ec2_metadata_service_endpoint = http://my-custom-endpoint:444
/// ```
///
/// 4. An explicitly set endpoint mode:
/// ```no_run
/// use aws_config::imds::client::{Client, EndpointMode};
/// # async fn docs() {
/// let client = Client::builder().endpoint_mode(EndpointMode::IpV6).build();
/// # }
/// ```
///
/// 5. An [endpoint mode](EndpointMode) loaded from the `AWS_EC2_METADATA_SERVICE_ENDPOINT_MODE` environment
///    variable. Valid values: `IPv4`, `IPv6`
///
/// 6. An [endpoint mode](EndpointMode) loaded from the `ec2_metadata_service_endpoint_mode` field in
///    `~/.aws/config`:
/// ```ini
/// [default]
/// # ... other configuration
/// ec2_metadata_service_endpoint_mode = IPv4
/// ```
///
/// 7. The default value of `http://169.254.169.254` will be used.
///
#[derive(Clone, Debug)]
pub struct Client {
    operation: Operation<String, SensitiveString, InnerImdsError>,
}

impl Client {
    /// IMDS client builder
    pub fn builder() -> Builder {
        Builder::default()
    }

    /// Retrieve information from IMDS
    ///
    /// This method will handle loading and caching a session token, combining the `path` with the
    /// configured IMDS endpoint, and retrying potential errors.
    ///
    /// For more information about IMDSv2 methods and functionality, see
    /// [Instance metadata and user data](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/ec2-instance-metadata.html)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use aws_config::imds::client::Client;
    /// # async fn docs() {
    /// let client = Client::builder().build();
    /// let ami_id = client
    ///   .get("/latest/meta-data/ami-id")
    ///   .await
    ///   .expect("failure communicating with IMDS");
    /// # }
    /// ```
    pub async fn get(&self, path: impl Into<String>) -> Result<SensitiveString, ImdsError> {
        self.operation
            .invoke(path.into())
            .await
            .map_err(|err| match err {
                SdkError::ConstructionFailure(_) if err.source().is_some() => {
                    match err.into_source().map(|e| e.downcast::<ImdsError>()) {
                        Ok(Ok(token_failure)) => *token_failure,
                        Ok(Err(err)) => ImdsError::unexpected(err),
                        Err(err) => ImdsError::unexpected(err),
                    }
                }
                SdkError::ConstructionFailure(_) => ImdsError::unexpected(err),
                SdkError::ServiceError(context) => match context.err() {
                    InnerImdsError::InvalidUtf8 => {
                        ImdsError::unexpected("IMDS returned invalid UTF-8")
                    }
                    InnerImdsError::BadStatus => ImdsError::error_response(context.into_raw()),
                },
                // If the error source is an ImdsError, then we need to directly return that source.
                // That way, the IMDS token provider's errors can become the top-level ImdsError.
                // There is a unit test that checks the correct error is being extracted.
                err @ SdkError::DispatchFailure(_) => match err.into_source() {
                    Ok(source) => match source.downcast::<ConnectorError>() {
                        Ok(source) => match source.into_source().downcast::<ImdsError>() {
                            Ok(source) => *source,
                            Err(err) => ImdsError::unexpected(err),
                        },
                        Err(err) => ImdsError::unexpected(err),
                    },
                    Err(err) => ImdsError::unexpected(err),
                },
                SdkError::TimeoutError(_) | SdkError::ResponseError(_) => ImdsError::io_error(err),
                _ => ImdsError::unexpected(err),
            })
    }
}

/// New-type around `String` that doesn't emit the string value in the `Debug` impl.
#[derive(Clone)]
pub struct SensitiveString(String);

impl fmt::Debug for SensitiveString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SensitiveString")
            .field(&"** redacted **")
            .finish()
    }
}

impl AsRef<str> for SensitiveString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for SensitiveString {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<SensitiveString> for String {
    fn from(value: SensitiveString) -> Self {
        value.0
    }
}

/// Runtime plugin that is used by both the IMDS client and the inner client that resolves
/// the IMDS token and attaches it to requests. This runtime plugin marks the responses as
/// sensitive, configures user agent headers, and sets up retries and timeouts.
#[derive(Debug)]
struct ImdsCommonRuntimePlugin {
    config: FrozenLayer,
    components: RuntimeComponentsBuilder,
}

impl ImdsCommonRuntimePlugin {
    fn new(
        config: &ProviderConfig,
        endpoint_resolver: ImdsEndpointResolver,
        retry_config: RetryConfig,
        retry_classifier: SharedRetryClassifier,
        timeout_config: TimeoutConfig,
    ) -> Self {
        let mut layer = Layer::new("ImdsCommonRuntimePlugin");
        layer.store_put(AuthSchemeOptionResolverParams::new(()));
        layer.store_put(EndpointResolverParams::new(()));
        layer.store_put(SensitiveOutput);
        layer.store_put(retry_config);
        layer.store_put(timeout_config);
        layer.store_put(user_agent());

        Self {
            config: layer.freeze(),
            components: RuntimeComponentsBuilder::new("ImdsCommonRuntimePlugin")
                .with_http_client(config.http_client())
                .with_endpoint_resolver(Some(endpoint_resolver))
                .with_interceptor(UserAgentInterceptor::new())
                .with_retry_classifier(retry_classifier)
                .with_retry_strategy(Some(StandardRetryStrategy::new()))
                .with_time_source(Some(config.time_source()))
                .with_sleep_impl(config.sleep_impl()),
        }
    }
}

impl RuntimePlugin for ImdsCommonRuntimePlugin {
    fn config(&self) -> Option<FrozenLayer> {
        Some(self.config.clone())
    }

    fn runtime_components(
        &self,
        _current_components: &RuntimeComponentsBuilder,
    ) -> Cow<'_, RuntimeComponentsBuilder> {
        Cow::Borrowed(&self.components)
    }
}

/// IMDSv2 Endpoint Mode
///
/// IMDS can be accessed in two ways:
/// 1. Via the IpV4 endpoint: `http://169.254.169.254`
/// 2. Via the Ipv6 endpoint: `http://[fd00:ec2::254]`
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum EndpointMode {
    /// IpV4 mode: `http://169.254.169.254`
    ///
    /// This mode is the default unless otherwise specified.
    IpV4,
    /// IpV6 mode: `http://[fd00:ec2::254]`
    IpV6,
}

impl FromStr for EndpointMode {
    type Err = InvalidEndpointMode;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            _ if value.eq_ignore_ascii_case("ipv4") => Ok(EndpointMode::IpV4),
            _ if value.eq_ignore_ascii_case("ipv6") => Ok(EndpointMode::IpV6),
            other => Err(InvalidEndpointMode::new(other.to_owned())),
        }
    }
}

impl EndpointMode {
    /// IMDS URI for this endpoint mode
    fn endpoint(&self) -> Uri {
        match self {
            EndpointMode::IpV4 => Uri::from_static("http://169.254.169.254"),
            EndpointMode::IpV6 => Uri::from_static("http://[fd00:ec2::254]"),
        }
    }
}

/// IMDSv2 Client Builder
#[derive(Default, Debug, Clone)]
pub struct Builder {
    max_attempts: Option<u32>,
    endpoint: Option<EndpointSource>,
    mode_override: Option<EndpointMode>,
    token_ttl: Option<Duration>,
    connect_timeout: Option<Duration>,
    read_timeout: Option<Duration>,
    operation_timeout: Option<Duration>,
    operation_attempt_timeout: Option<Duration>,
    config: Option<ProviderConfig>,
    retry_classifier: Option<SharedRetryClassifier>,
}

impl Builder {
    /// Override the number of retries for fetching tokens & metadata
    ///
    /// By default, 4 attempts will be made.
    pub fn max_attempts(mut self, max_attempts: u32) -> Self {
        self.max_attempts = Some(max_attempts);
        self
    }

    /// Configure generic options of the [`Client`]
    ///
    /// # Examples
    /// ```no_run
    /// # async fn test() {
    /// use aws_config::imds::Client;
    /// use aws_config::provider_config::ProviderConfig;
    ///
    /// let provider = Client::builder()
    ///     .configure(&ProviderConfig::with_default_region().await)
    ///     .build();
    /// # }
    /// ```
    pub fn configure(mut self, provider_config: &ProviderConfig) -> Self {
        self.config = Some(provider_config.clone());
        self
    }

    /// Override the endpoint for the [`Client`]
    ///
    /// By default, the client will resolve an endpoint from the environment, AWS config, and endpoint mode.
    ///
    /// See [`Client`] for more information.
    pub fn endpoint(mut self, endpoint: impl AsRef<str>) -> Result<Self, BoxError> {
        let uri: Uri = endpoint.as_ref().parse()?;
        self.endpoint = Some(EndpointSource::Explicit(uri));
        Ok(self)
    }

    /// Override the endpoint mode for [`Client`]
    ///
    /// * When set to [`IpV4`](EndpointMode::IpV4), the endpoint will be `http://169.254.169.254`.
    /// * When set to [`IpV6`](EndpointMode::IpV6), the endpoint will be `http://[fd00:ec2::254]`.
    pub fn endpoint_mode(mut self, mode: EndpointMode) -> Self {
        self.mode_override = Some(mode);
        self
    }

    /// Override the time-to-live for the session token
    ///
    /// Requests to IMDS utilize a session token for authentication. By default, session tokens last
    /// for 6 hours. When the TTL for the token expires, a new token must be retrieved from the
    /// metadata service.
    pub fn token_ttl(mut self, ttl: Duration) -> Self {
        self.token_ttl = Some(ttl);
        self
    }

    /// Override the connect timeout for IMDS
    ///
    /// This value defaults to 1 second
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// Override the read timeout for IMDS
    ///
    /// This value defaults to 1 second
    pub fn read_timeout(mut self, timeout: Duration) -> Self {
        self.read_timeout = Some(timeout);
        self
    }

    /// Override the operation timeout for IMDS
    ///
    /// This value defaults to 1 second
    pub fn operation_timeout(mut self, timeout: Duration) -> Self {
        self.operation_timeout = Some(timeout);
        self
    }

    /// Override the operation attempt timeout for IMDS
    ///
    /// This value defaults to 1 second
    pub fn operation_attempt_timeout(mut self, timeout: Duration) -> Self {
        self.operation_attempt_timeout = Some(timeout);
        self
    }

    /// Override the retry classifier for IMDS
    ///
    /// This defaults to only retrying on server errors and 401s. The [ImdsResponseRetryClassifier] in this
    /// module offers some configuration options and can be wrapped by[SharedRetryClassifier::new()] for use
    /// here or you can create your own fully customized [SharedRetryClassifier].
    pub fn retry_classifier(mut self, retry_classifier: SharedRetryClassifier) -> Self {
        self.retry_classifier = Some(retry_classifier);
        self
    }

    /* TODO(https://github.com/awslabs/aws-sdk-rust/issues/339): Support customizing the port explicitly */
    /*
    pub fn port(mut self, port: u32) -> Self {
        self.port_override = Some(port);
        self
    }*/

    /// Build an IMDSv2 Client
    pub fn build(self) -> Client {
        let config = self.config.unwrap_or_default();
        let timeout_config = TimeoutConfig::builder()
            .connect_timeout(self.connect_timeout.unwrap_or(DEFAULT_CONNECT_TIMEOUT))
            .read_timeout(self.read_timeout.unwrap_or(DEFAULT_READ_TIMEOUT))
            .operation_attempt_timeout(
                self.operation_attempt_timeout
                    .unwrap_or(DEFAULT_OPERATION_ATTEMPT_TIMEOUT),
            )
            .operation_timeout(self.operation_timeout.unwrap_or(DEFAULT_OPERATION_TIMEOUT))
            .build();
        let endpoint_source = self
            .endpoint
            .unwrap_or_else(|| EndpointSource::Env(config.clone()));
        let endpoint_resolver = ImdsEndpointResolver {
            endpoint_source: Arc::new(endpoint_source),
            mode_override: self.mode_override,
        };
        let retry_config = RetryConfig::standard()
            .with_max_attempts(self.max_attempts.unwrap_or(DEFAULT_ATTEMPTS));
        let retry_classifier = self.retry_classifier.unwrap_or(SharedRetryClassifier::new(
            ImdsResponseRetryClassifier::default(),
        ));
        let common_plugin = SharedRuntimePlugin::new(ImdsCommonRuntimePlugin::new(
            &config,
            endpoint_resolver,
            retry_config,
            retry_classifier,
            timeout_config,
        ));
        let operation = Operation::builder()
            .service_name("imds")
            .operation_name("get")
            .runtime_plugin(common_plugin.clone())
            .runtime_plugin(TokenRuntimePlugin::new(
                common_plugin,
                self.token_ttl.unwrap_or(DEFAULT_TOKEN_TTL),
            ))
            .runtime_plugin(
                MetricsRuntimePlugin::builder()
                    .with_scope("aws_config::imds_credentials")
                    .with_time_source(config.time_source())
                    .with_metadata(Metadata::new("get_credentials", "imds"))
                    .build()
                    .expect("All required fields have been set"),
            )
            .with_connection_poisoning()
            .serializer(|path| {
                Ok(HttpRequest::try_from(
                    http::Request::builder()
                        .uri(path)
                        .body(SdkBody::empty())
                        .expect("valid request"),
                )
                .unwrap())
            })
            .deserializer(|response| {
                if response.status().is_success() {
                    std::str::from_utf8(response.body().bytes().expect("non-streaming response"))
                        .map(|data| SensitiveString::from(data.to_string()))
                        .map_err(|_| OrchestratorError::operation(InnerImdsError::InvalidUtf8))
                } else {
                    Err(OrchestratorError::operation(InnerImdsError::BadStatus))
                }
            });
        let operation = if let Some(bv) = config.behavior_version() {
            operation.behavior_version(bv).build()
        } else {
            operation.build()
        };
        Client { operation }
    }
}

mod env {
    pub(super) const ENDPOINT: &str = "AWS_EC2_METADATA_SERVICE_ENDPOINT";
    pub(super) const ENDPOINT_MODE: &str = "AWS_EC2_METADATA_SERVICE_ENDPOINT_MODE";
}

mod profile_keys {
    pub(super) const ENDPOINT: &str = "ec2_metadata_service_endpoint";
    pub(super) const ENDPOINT_MODE: &str = "ec2_metadata_service_endpoint_mode";
}

/// Endpoint Configuration Abstraction
#[derive(Debug, Clone)]
enum EndpointSource {
    Explicit(Uri),
    Env(ProviderConfig),
}

impl EndpointSource {
    async fn endpoint(&self, mode_override: Option<EndpointMode>) -> Result<Uri, BuildError> {
        match self {
            EndpointSource::Explicit(uri) => {
                if mode_override.is_some() {
                    tracing::warn!(endpoint = ?uri, mode = ?mode_override,
                        "Endpoint mode override was set in combination with an explicit endpoint. \
                        The mode override will be ignored.")
                }
                Ok(uri.clone())
            }
            EndpointSource::Env(conf) => {
                let env = conf.env();
                // load an endpoint override from the environment
                let profile = conf.profile().await;
                let uri_override = if let Ok(uri) = env.get(env::ENDPOINT) {
                    Some(Cow::Owned(uri))
                } else {
                    profile
                        .and_then(|profile| profile.get(profile_keys::ENDPOINT))
                        .map(Cow::Borrowed)
                };
                if let Some(uri) = uri_override {
                    return Uri::try_from(uri.as_ref()).map_err(BuildError::invalid_endpoint_uri);
                }

                // if not, load a endpoint mode from the environment
                let mode = if let Some(mode) = mode_override {
                    mode
                } else if let Ok(mode) = env.get(env::ENDPOINT_MODE) {
                    mode.parse::<EndpointMode>()
                        .map_err(BuildError::invalid_endpoint_mode)?
                } else if let Some(mode) = profile.and_then(|p| p.get(profile_keys::ENDPOINT_MODE))
                {
                    mode.parse::<EndpointMode>()
                        .map_err(BuildError::invalid_endpoint_mode)?
                } else {
                    EndpointMode::IpV4
                };

                Ok(mode.endpoint())
            }
        }
    }
}

#[derive(Clone, Debug)]
struct ImdsEndpointResolver {
    endpoint_source: Arc<EndpointSource>,
    mode_override: Option<EndpointMode>,
}

impl ResolveEndpoint for ImdsEndpointResolver {
    fn resolve_endpoint<'a>(&'a self, _: &'a EndpointResolverParams) -> EndpointFuture<'a> {
        EndpointFuture::new(async move {
            self.endpoint_source
                .endpoint(self.mode_override.clone())
                .await
                .map(|uri| Endpoint::builder().url(uri.to_string()).build())
                .map_err(|err| err.into())
        })
    }
}

/// IMDS Response Retry Classifier
///
/// Possible status codes:
/// - 200 (OK)
/// - 400 (Missing or invalid parameters) **Not Retryable**
/// - 401 (Unauthorized, expired token) **Retryable**
/// - 403 (IMDS disabled): **Not Retryable**
/// - 404 (Not found): **Not Retryable**
/// - >=500 (server error): **Retryable**
/// - Timeouts: Not retried by default, but this is configurable via [Self::with_retry_connect_timeouts()]
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct ImdsResponseRetryClassifier {
    retry_connect_timeouts: bool,
}

impl ImdsResponseRetryClassifier {
    /// Indicate whether the IMDS client should retry on connection timeouts
    pub fn with_retry_connect_timeouts(mut self, retry_connect_timeouts: bool) -> Self {
        self.retry_connect_timeouts = retry_connect_timeouts;
        self
    }
}

impl ClassifyRetry for ImdsResponseRetryClassifier {
    fn name(&self) -> &'static str {
        "ImdsResponseRetryClassifier"
    }

    fn classify_retry(&self, ctx: &InterceptorContext) -> RetryAction {
        if let Some(response) = ctx.response() {
            let status = response.status();
            match status {
                _ if status.is_server_error() => RetryAction::server_error(),
                // 401 indicates that the token has expired, this is retryable
                _ if status.as_u16() == 401 => RetryAction::server_error(),
                // This catch-all includes successful responses that fail to parse. These should not be retried.
                _ => RetryAction::NoActionIndicated,
            }
        } else if self.retry_connect_timeouts {
            RetryAction::server_error()
        } else {
            // This is the default behavior.
            // Don't retry timeouts for IMDS, or else it will take ~30 seconds for the default
            // credentials provider chain to fail to provide credentials.
            // Also don't retry non-responses.
            RetryAction::NoActionIndicated
        }
    }
}
