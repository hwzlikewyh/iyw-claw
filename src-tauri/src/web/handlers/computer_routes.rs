use super::{computer, computer_tools};
use axum::{routing::post, Router};

pub fn routes() -> Router {
    Router::new()
        .route("/computer_available", post(computer::computer_available))
        .route("/computer_status", post(computer::computer_status))
        .route(
            "/computer_request_permission",
            post(computer::computer_request_permission),
        )
        .route(
            "/computer_open_permission_settings",
            post(computer::computer_open_permission_settings),
        )
        .route(
            "/computer_reveal_helper",
            post(computer::computer_reveal_helper),
        )
        .route(
            "/computer_list_shareable_windows",
            post(computer::computer_list_shareable_windows),
        )
        .route(
            "/computer_window_thumbnail",
            post(computer::computer_window_thumbnail),
        )
        .route(
            "/computer_share_window",
            post(computer::computer_share_window),
        )
        .route(
            "/computer_share_windows",
            post(computer::computer_share_windows),
        )
        .route(
            "/computer_shared_state",
            post(computer::computer_shared_state),
        )
        .route(
            "/computer_share_screen",
            post(computer::computer_share_screen),
        )
        .route("/computer_share_app", post(computer::computer_share_app))
        .route("/computer_revoke_all", post(computer::computer_revoke_all))
        .route("/computer_stop", post(computer::computer_stop))
        .route(
            "/computer_stop_key_status",
            post(computer::computer_stop_key_status),
        )
        .route(
            "/computer_driver_info",
            post(computer::computer_driver_info),
        )
        .route(
            "/computer_driver_install",
            post(computer::computer_driver_install),
        )
        .route(
            "/computer_driver_uninstall",
            post(computer::computer_driver_uninstall),
        )
        .route(
            "/get_computer_tools_settings",
            post(computer_tools::get_computer_tools_settings),
        )
        .route(
            "/set_computer_tools_settings",
            post(computer_tools::set_computer_tools_settings),
        )
        .route(
            "/set_computer_tools_enabled",
            post(computer_tools::set_computer_tools_enabled),
        )
        .route(
            "/set_computer_tools_preferences",
            post(computer_tools::set_computer_tools_preferences),
        )
}
