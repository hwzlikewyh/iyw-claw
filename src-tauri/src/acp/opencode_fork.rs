use std::{collections::BTreeMap, net::TcpListener, path::Path, time::Duration};

use reqwest::{Client, Method};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::acp::error::AcpError;

const PORT_ENV: &str = "IYW_CLAW_OPENCODE_HTTP_PORT";
const PASSWORD_ENV: &str = "OPENCODE_SERVER_PASSWORD";
const USERNAME_ENV: &str = "OPENCODE_SERVER_USERNAME";
const USERNAME: &str = "iyw-claw";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

// ACP 命令本身启动 HTTP 服务；端口与凭证只存于该连接的启动环境。
pub fn prepare(environment: &mut BTreeMap<String, String>) -> Result<(), AcpError> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| failure(format!("分配本地端口失败：{error}")))?;
    let port = listener
        .local_addr()
        .map_err(|error| failure(format!("读取本地端口失败：{error}")))?
        .port();
    environment.insert(PORT_ENV.into(), port.to_string());
    environment.insert(USERNAME_ENV.into(), USERNAME.into());
    environment.insert(PASSWORD_ENV.into(), uuid::Uuid::new_v4().to_string());
    Ok(())
}

pub fn launch_args(environment: &BTreeMap<String, String>) -> Vec<String> {
    environment
        .get(PORT_ENV)
        .map(|port| {
            vec![
                "--hostname".into(),
                "127.0.0.1".into(),
                "--port".into(),
                port.clone(),
                "--mdns=false".into(),
            ]
        })
        .unwrap_or_default()
}

#[derive(Clone)]
pub struct OpenCodeForkClient {
    client: Client,
    port: u16,
    password: String,
}

#[derive(Deserialize)]
struct NativeMessage {
    info: NativeMessageInfo,
}

#[derive(Deserialize)]
struct NativeMessageInfo {
    id: String,
    role: String,
    time: NativeMessageTime,
}

#[derive(Deserialize)]
struct NativeMessageTime {
    completed: Option<u64>,
}

impl OpenCodeForkClient {
    pub fn from_env(environment: &BTreeMap<String, String>) -> Result<Self, AcpError> {
        let port = environment
            .get(PORT_ENV)
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| failure("本地服务端口缺失"))?;
        let password = environment
            .get(PASSWORD_ENV)
            .cloned()
            .ok_or_else(|| failure("本地服务凭证缺失"))?;
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| failure(format!("创建本地客户端失败：{error}")))?;
        Ok(Self {
            client,
            port,
            password,
        })
    }

    pub async fn fork(
        &self,
        session_id: &str,
        cwd: &Path,
        message_id: &str,
    ) -> Result<String, AcpError> {
        let messages = self.messages(session_id, cwd).await?;
        let index = messages
            .iter()
            .position(|message| message.info.id == message_id)
            .ok_or_else(|| failure("目标消息不存在，请刷新后重试"))?;
        let target = &messages[index].info;
        if target.role != "assistant" || target.time.completed.is_none() {
            return Err(failure("只能从已完成的回复分叉"));
        }
        // 原生 messageID 为不包含的边界，传下一条消息以保留所选回复。
        let body = messages
            .get(index + 1)
            .map(|next| json!({"messageID": next.info.id}))
            .unwrap_or_else(|| json!({}));
        let request = self.request(session_id, cwd, "fork")?.json(&body);
        let forked_id = self.create_fork(request, session_id).await?;
        let forked_messages = self.messages(&forked_id, cwd).await?;
        if forked_messages.len() != index + 1 {
            return Err(failure("分叉历史边界校验失败，原会话保持不变"));
        }
        tracing::info!(
            agent = "云舟",
            copied_messages = forked_messages.len(),
            "历史消息分叉完成"
        );
        Ok(forked_id)
    }

    async fn create_fork(
        &self,
        request: reqwest::RequestBuilder,
        original_id: &str,
    ) -> Result<String, AcpError> {
        let response = request
            .send()
            .await
            .map_err(|error| failure(format!("分叉请求失败：{error}")))?;
        let result: Value = decode(response, "分叉").await?;
        result
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && *id != original_id)
            .map(str::to_string)
            .ok_or_else(|| failure("分叉响应没有新的会话 ID"))
    }

    async fn messages(&self, session_id: &str, cwd: &Path) -> Result<Vec<NativeMessage>, AcpError> {
        let response = self
            .request(session_id, cwd, "message")?
            .send()
            .await
            .map_err(|error| failure(format!("读取消息失败：{error}")))?;
        decode(response, "读取消息").await
    }

    fn request(
        &self,
        session_id: &str,
        cwd: &Path,
        operation: &str,
    ) -> Result<reqwest::RequestBuilder, AcpError> {
        let mut url = reqwest::Url::parse(&format!("http://127.0.0.1:{}", self.port))
            .map_err(|error| failure(format!("本地地址无效：{error}")))?;
        url.path_segments_mut()
            .map_err(|_| failure("本地地址无法追加路径"))?
            .extend(["session", session_id, operation]);
        let method = if operation == "fork" {
            Method::POST
        } else {
            Method::GET
        };
        Ok(self
            .client
            .request(method, url)
            .basic_auth(USERNAME, Some(&self.password))
            .query(&[("directory", cwd.to_string_lossy())]))
    }
}

async fn decode<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    operation: &str,
) -> Result<T, AcpError> {
    let status = response.status();
    if !status.is_success() {
        return Err(failure(format!("{operation}返回 HTTP {status}")));
    }
    response
        .json()
        .await
        .map_err(|error| failure(format!("{operation}响应格式错误：{error}")))
}

fn failure(message: impl std::fmt::Display) -> AcpError {
    AcpError::protocol(format!("云舟历史分叉失败：{message}"))
}
