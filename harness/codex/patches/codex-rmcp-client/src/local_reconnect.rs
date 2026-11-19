use super::*;

type LocalService = RunningService<RoleClient, ElicitationClientService>;
const MAX_HANDSHAKE_WAIT: Duration = Duration::from_secs(8);
const OFFLINE_RETRY: Duration = Duration::from_secs(3);
pub(super) const LOCAL_RETRY_DELAYS_MS: &[u64] = &[100, 250, 500, 1_000, 2_000];

fn is_loopback_endpoint(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|endpoint| match endpoint.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    })
}

pub(super) fn http_config(url: &str) -> StreamableHttpClientTransportConfig {
    let mut config = StreamableHttpClientTransportConfig::with_uri(url.to_owned());
    if is_loopback_endpoint(url) {
        let mut backoff = rmcp::transport::common::client_side_sse::ExponentialBackoff::default();
        backoff.base_duration = Duration::from_millis(LOCAL_RETRY_DELAYS_MS[0]);
        backoff.max_delay = Some(OFFLINE_RETRY);
        config.retry_config = Arc::new(backoff);
    }
    config
}

impl RmcpClient {
    /// 仅恢复宿主内存通道和已配置的回环地址。
    pub fn supports_local_reconnect(&self) -> bool {
        match &self.transport_recipe {
            TransportRecipe::InProcess { .. } => true,
            TransportRecipe::Stdio { .. } => false,
            TransportRecipe::StreamableHttp { url, .. } => is_loopback_endpoint(url),
        }
    }

    /// 空闲连接由传输结束事件唤醒。等待期间不持有客户端或旧服务。
    pub fn watch_local_disconnects(self: &Arc<Self>) {
        if !self.supports_local_reconnect() {
            return;
        }
        let owner = Arc::downgrade(self);
        tokio::spawn(async move {
            while let Some(client) = owner.upgrade() {
                let Ok(service) = client.service().await else {
                    break;
                };
                let closed = service.service().transport_closed.clone();
                let failed = Arc::downgrade(&service);
                drop(service);
                drop(client);
                closed.cancelled().await;
                let (Some(client), Some(failed)) = (owner.upgrade(), failed.upgrade()) else {
                    continue;
                };
                if let Err(error) = client.reconnect_local(&failed).await {
                    if is_authentication_required_error(&error) {
                        break;
                    }
                    drop(failed);
                    drop(client);
                    time::sleep(OFFLINE_RETRY).await;
                }
            }
        });
    }

    async fn reconnect_local(&self, service: &Arc<LocalService>) -> Result<()> {
        time::timeout(MAX_HANDSHAKE_WAIT, self.reinitialize_session(service))
            .await
            .context("timed out reconnecting the local MCP transport")?
    }

    pub(super) async fn service_for_new_request(&self) -> Result<Arc<LocalService>> {
        let service = self.service().await?;
        if self.supports_local_reconnect()
            && (service.is_transport_closed()
                || service.is_closed()
                || service.service().transport_closed.is_cancelled())
        {
            self.reconnect_local(&service).await?;
            return self.service().await;
        }
        Ok(service)
    }

    pub(super) async fn run_local_operation<T, F, Fut>(
        &self,
        label: &str,
        timeout: Option<Duration>,
        pause: &ElicitationPauseState,
        operation: &F,
    ) -> Result<T>
    where
        F: Fn(Arc<LocalService>) -> Fut,
        Fut: std::future::Future<Output = std::result::Result<T, ServiceError>>,
    {
        let service = self.service_for_new_request().await?;
        let first = Self::run_service_operation_with_transient_retries(
            Arc::clone(&service),
            label,
            timeout,
            pause.clone(),
            operation,
        )
        .await;
        let error = match first {
            Ok(result) => return Ok(result),
            Err(error) => error,
        };
        let expired = Self::is_session_expired_404(&error);
        if !expired && !is_disconnect(&error) {
            return Err(error.into());
        }
        service.service().transport_closed.cancel();
        // 方法名来自宿主。未知业务调用和包含交互的操作可能已经生效。
        if !expired
            && !matches!(
                label,
                "tools/list" | "resources/list" | "resources/templates/list" | "resources/read"
            )
        {
            return Err(error.into());
        }
        self.reconnect_local(&service).await?;
        let recovered = self.service().await?;
        let result = Self::run_service_operation_once(
            Arc::clone(&recovered),
            label,
            timeout,
            pause.clone(),
            operation,
        )
        .await;
        if result
            .as_ref()
            .is_err_and(|error| Self::is_session_expired_404(error) || is_disconnect(error))
        {
            recovered.service().transport_closed.cancel();
        }
        result.map_err(Into::into)
    }
}

fn is_disconnect(error: &ClientOperationError) -> bool {
    match error {
        ClientOperationError::Service(ServiceError::TransportClosed) => true,
        ClientOperationError::Service(ServiceError::TransportSend(error)) => error
            .error
            .downcast_ref::<StreamableHttpError<StreamableHttpClientAdapterError>>()
            .is_some_and(|error| match error {
                StreamableHttpError::Client(StreamableHttpClientAdapterError::HttpRequest(_)) => {
                    RmcpClient::is_retryable_streamable_http_error(error)
                }
                StreamableHttpError::SessionExpired
                | StreamableHttpError::TransportChannelClosed
                | StreamableHttpError::UnexpectedEndOfStream
                | StreamableHttpError::Io(_) => true,
                _ => false,
            }),
        _ => false,
    }
}
