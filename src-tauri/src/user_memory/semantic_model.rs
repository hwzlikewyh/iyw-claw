use crate::app_error::AppCommandError;

pub(super) fn model_error(detail: impl std::fmt::Display) -> AppCommandError {
    AppCommandError::configuration_invalid("记忆向量索引暂时不可用").with_detail(detail.to_string())
}
