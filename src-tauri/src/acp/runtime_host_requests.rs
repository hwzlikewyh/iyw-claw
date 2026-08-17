use sacp::schema::{
    CreateTerminalRequest, CreateTerminalResponse, KillTerminalRequest, KillTerminalResponse,
    ReadTextFileRequest, ReadTextFileResponse, ReleaseTerminalRequest, ReleaseTerminalResponse,
    RequestPermissionRequest, RequestPermissionResponse, TerminalOutputRequest,
    TerminalOutputResponse, WaitForTerminalExitRequest, WaitForTerminalExitResponse,
    WriteTextFileRequest, WriteTextFileResponse,
};
use sacp::Responder;

use crate::acp::connection::handle_permission_request;
use crate::acp::runtime_host_responses::{respond_file_system, respond_missing, respond_terminal};
use crate::acp::runtime_host_router::SessionRequestRouter;

impl SessionRequestRouter {
    pub(super) async fn permission(
        &self,
        request: RequestPermissionRequest,
        responder: Responder<RequestPermissionResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let Some(route) = self.resolve(&session_id) else {
            return respond_missing(responder, &session_id);
        };
        handle_permission_request(
            &route.state,
            &route.emitter,
            &route.permissions,
            &route.cwd,
            request,
            responder,
        )
        .await;
        Ok(())
    }

    pub(super) async fn read_file(
        &self,
        request: ReadTextFileRequest,
        responder: Responder<ReadTextFileResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let Some(route) = self.resolve(&session_id) else {
            return respond_missing(responder, &session_id);
        };
        respond_file_system(responder, route.file_system.read_text_file(request).await)
    }

    pub(super) async fn write_file(
        &self,
        request: WriteTextFileRequest,
        responder: Responder<WriteTextFileResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let Some(route) = self.resolve(&session_id) else {
            return respond_missing(responder, &session_id);
        };
        respond_file_system(responder, route.file_system.write_text_file(request).await)
    }

    pub(super) async fn create_terminal(
        &self,
        request: CreateTerminalRequest,
        responder: Responder<CreateTerminalResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let Some(route) = self.resolve(&session_id) else {
            return respond_missing(responder, &session_id);
        };
        respond_terminal(responder, route.terminal.create_terminal(request).await)
    }

    pub(super) async fn terminal_output(
        &self,
        request: TerminalOutputRequest,
        responder: Responder<TerminalOutputResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let Some(route) = self.resolve(&session_id) else {
            return respond_missing(responder, &session_id);
        };
        respond_terminal(responder, route.terminal.terminal_output(request).await)
    }

    pub(super) async fn wait_terminal(
        &self,
        request: WaitForTerminalExitRequest,
        responder: Responder<WaitForTerminalExitResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let Some(route) = self.resolve(&session_id) else {
            return respond_missing(responder, &session_id);
        };
        respond_terminal(
            responder,
            route.terminal.wait_for_terminal_exit(request).await,
        )
    }

    pub(super) async fn kill_terminal(
        &self,
        request: KillTerminalRequest,
        responder: Responder<KillTerminalResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let Some(route) = self.resolve(&session_id) else {
            return respond_missing(responder, &session_id);
        };
        respond_terminal(responder, route.terminal.kill_terminal(request).await)
    }

    pub(super) async fn release_terminal(
        &self,
        request: ReleaseTerminalRequest,
        responder: Responder<ReleaseTerminalResponse>,
    ) -> Result<(), sacp::Error> {
        let session_id = request.session_id.clone();
        let Some(route) = self.resolve(&session_id) else {
            return respond_missing(responder, &session_id);
        };
        respond_terminal(responder, route.terminal.release_terminal(request).await)
    }
}
