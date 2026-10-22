use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Client;
use sea_orm::DatabaseConnection;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use super::models::{BrowseFiles, FilePage, Folder, PAGE_SIZE};
use crate::app_error::AppCommandError;

const BASE: &str = "https://gateway.iyw.cn/ai-agent-new/api/knowledge";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

pub struct KnowledgeClient {
    token: crate::acp::account_credentials::AccountAccessToken,
    pub scope: String,
}

impl KnowledgeClient {
    pub async fn new(conn: &DatabaseConnection) -> Result<Self, AppCommandError> {
        let scope = super::selection::account_scope(conn).await?;
        let token = crate::commands::iyw_account::iyw_account_access_token_core(conn)
            .await?
            .ok_or_else(|| AppCommandError::authentication_failed("请先登录后使用知识库"))?;
        if scope != super::selection::account_scope(conn).await? {
            return Err(AppCommandError::permission_denied(
                "账号或空间已变更，请重试",
            ));
        }
        Ok(Self { token, scope })
    }

    pub async fn folders(&self, category: u8) -> Result<Vec<Folder>, AppCommandError> {
        super::models::validate_category(category)?;
        let data: Value = self
            .post("folders/tree", json!({"category": category}))
            .await?;
        serde_json::from_value(
            data.get("folderTree")
                .cloned()
                .ok_or_else(invalid_response)?,
        )
        .map_err(|_| invalid_response())
    }

    pub async fn files(&self, input: &BrowseFiles) -> Result<FilePage, AppCommandError> {
        super::models::validate_category(input.category)?;
        if !super::models::valid_id(&input.folder_id) || input.page == 0 {
            return Err(AppCommandError::invalid_input("文件夹或页码无效"));
        }
        self.post(
            "files/list",
            json!({"category":input.category,
            "folderId":input.folder_id,"includeChildren":false,"sortBy":"name",
            "sortOrder":"asc","page":input.page,"pageSize":PAGE_SIZE}),
        )
        .await
    }

    pub async fn post<T: DeserializeOwned>(
        &self,
        operation: &str,
        body: Value,
    ) -> Result<T, AppCommandError> {
        let mut response = client()?
            .post(format!("{BASE}/{operation}"))
            .header("token", self.token.expose())
            .json(&body)
            .send()
            .await
            .map_err(network_error)?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(network_error)? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(invalid_response());
            }
            bytes.extend_from_slice(&chunk);
        }
        let envelope: Value = serde_json::from_slice(&bytes).map_err(|_| invalid_response())?;
        if !status.is_success() || envelope["code"].as_i64() != Some(1) {
            tracing::warn!(operation, status = status.as_u16(), code = ?envelope["code"].as_i64(),
                "[knowledge] upstream request failed");
            return Err(upstream_error(operation, status.as_u16()));
        }
        serde_json::from_value(envelope.get("data").cloned().ok_or_else(invalid_response)?)
            .map_err(|_| invalid_response())
    }
}

fn upstream_error(operation: &str, status: u16) -> AppCommandError {
    let error = match status {
        400 | 404 => AppCommandError::invalid_input("知识库范围无效或无权访问，请重新选择"),
        401 | 403 => {
            AppCommandError::authentication_failed("知识库登录已失效或无权访问，请重新登录")
        }
        _ => AppCommandError::network("线上知识库请求失败，请稍后重试"),
    };
    error.with_detail(format!("operation={operation},http_status={status}"))
}

fn client() -> Result<&'static Client, AppCommandError> {
    static CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .connect_timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent("iyw-claw")
                .build()
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|_| AppCommandError::network("无法连接线上知识库"))
}

fn network_error(error: reqwest::Error) -> AppCommandError {
    tracing::warn!(
        timeout = error.is_timeout(),
        connect = error.is_connect(),
        "[knowledge] request unavailable"
    );
    AppCommandError::network("线上知识库连接失败，请稍后重试").with_detail(format!(
        "timeout={},connect={}",
        error.is_timeout(),
        error.is_connect()
    ))
}

pub fn invalid_response() -> AppCommandError {
    AppCommandError::network("线上知识库返回了无效数据")
}
