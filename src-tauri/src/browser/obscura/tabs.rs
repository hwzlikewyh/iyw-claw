use serde_json::{json, Value};

use super::super::agent_tool_support::{invalid_argument, optional_bool, optional_string};
use super::super::{BrowserError, BrowserErrorCode};
use super::actions::Action;
use super::worker::{Operation, Rpc, Worker};
use super::{script, Route, Tab};

impl Route {
    pub(super) async fn run(
        &mut self,
        action: Action,
        input: &Value,
        operation: &Operation<'_>,
    ) -> Result<Value, BrowserError> {
        if self.worker.is_none() {
            self.worker = Some(Worker::spawn()?);
        }
        if let Action::Open(url) = action {
            return self.open(url, input, operation).await;
        }
        let id = super::route::selected_id(self, input)
            .ok_or_else(|| {
                BrowserError::new(
                    BrowserErrorCode::BrowserTabNotFound,
                    "Open an Obscura page before requesting this operation",
                )
            })?
            .to_string();
        self.act(action, &id, operation).await
    }

    pub(super) async fn open(
        &mut self,
        url: String,
        input: &Value,
        operation: &Operation<'_>,
    ) -> Result<Value, BrowserError> {
        let supplied = optional_string(input, "tab_id", 128)?;
        let new_tab = optional_bool(input, "new_tab")?.unwrap_or(false);
        if supplied.is_some() && new_tab {
            return Err(invalid_argument(
                "browser_open cannot combine tab_id with new_tab=true",
            ));
        }
        let existing = supplied
            .map(str::to_string)
            .or_else(|| (!new_tab).then(|| self.active.clone()).flatten());
        let id = match existing {
            Some(id) => id,
            None => self.create_tab(operation).await?,
        };
        let worker = self
            .worker
            .as_ref()
            .ok_or_else(|| super::worker::unavailable("Obscura is unavailable"))?;
        let tab = self
            .tabs
            .get_mut(&id)
            .ok_or_else(|| BrowserError::tab_not_found(&id))?;
        tab.url = url;
        tab.epoch = tab.epoch.saturating_add(1);
        let result = worker
            .call(
                Rpc {
                    method: "Page.navigate",
                    params: json!({"url":tab.url}),
                    session: Some(tab.session.clone()),
                },
                operation,
            )
            .await?;
        navigation_result(&result)?;
        script::metadata(worker, tab, operation)
            .await
            .map_err(|error| error.effect_may_have_occurred(tab.interacted))?;
        self.active = Some(id.clone());
        Ok(self.project(Some(&id), None))
    }

    async fn create_tab(&mut self, operation: &Operation<'_>) -> Result<String, BrowserError> {
        let slot = self.slots.clone().try_acquire_owned().map_err(|_| {
            BrowserError::new(
                BrowserErrorCode::BrowserTabLimit,
                "The browser tab limit has been reached",
            )
        })?;
        let worker = self
            .worker
            .as_ref()
            .ok_or_else(|| super::worker::unavailable("Obscura is unavailable"))?;
        let created = worker
            .call(
                Rpc {
                    method: "Target.createTarget",
                    params: json!({"url":"about:blank"}),
                    session: None,
                },
                operation,
            )
            .await?;
        let target = created["targetId"]
            .as_str()
            .ok_or_else(|| super::worker::unavailable("Obscura did not create a target"))?
            .to_string();
        let attached = worker
            .call(
                Rpc {
                    method: "Target.attachToTarget",
                    params: json!({"targetId":target,"flatten":true}),
                    session: None,
                },
                operation,
            )
            .await?;
        let session = attached["sessionId"]
            .as_str()
            .ok_or_else(|| super::worker::unavailable("Obscura did not attach the target"))?
            .to_string();
        let tab = Tab::new(target, session, slot);
        let id = tab.id.clone();
        self.tabs.insert(id.clone(), tab);
        Ok(id)
    }

    pub(super) async fn act(
        &mut self,
        action: Action,
        id: &str,
        operation: &Operation<'_>,
    ) -> Result<Value, BrowserError> {
        let worker = self
            .worker
            .as_ref()
            .ok_or_else(|| super::worker::unavailable("Obscura is unavailable"))?;
        let tab = self
            .tabs
            .get_mut(id)
            .ok_or_else(|| BrowserError::tab_not_found(id))?;
        let output = match action {
            Action::Script(args) => {
                let mutating =
                    !matches!(args.first().map(String::as_str), Some("snapshot" | "read"));
                tab.interacted |= mutating;
                let result = script::Script::new(worker, tab, operation)
                    .command(&args)
                    .await;
                if mutating {
                    tab.epoch = tab.epoch.saturating_add(1);
                }
                result.map_err(|error| error.effect_may_have_occurred(mutating))?
            }
            Action::Wait(expression) => {
                script::Script::new(worker, tab, operation)
                    .evaluate(&expression)
                    .await?
            }
            Action::Screenshot => {
                script::Script::new(worker, tab, operation)
                    .screenshot()
                    .await?
            }
            Action::Close => return self.close(id, operation).await,
            _ => return Err(invalid_argument("Unsupported Obscura action")),
        };
        script::metadata(worker, tab, operation)
            .await
            .map_err(|error| error.effect_may_have_occurred(tab.interacted))?;
        Ok(self.project(Some(id), Some(json!({"success":true,"data":output}))))
    }

    async fn close(&mut self, id: &str, operation: &Operation<'_>) -> Result<Value, BrowserError> {
        let tab = self
            .tabs
            .get(id)
            .ok_or_else(|| BrowserError::tab_not_found(id))?;
        let worker = self
            .worker
            .as_ref()
            .ok_or_else(|| super::worker::unavailable("Obscura is unavailable"))?;
        worker
            .call(
                Rpc {
                    method: "Target.closeTarget",
                    params: json!({"targetId":tab.target}),
                    session: None,
                },
                operation,
            )
            .await?;
        self.tabs.remove(id);
        if self.active.as_deref() == Some(id) {
            self.active = self.tabs.keys().next().cloned();
        }
        if self.tabs.is_empty() {
            self.worker = None;
        }
        Ok(self.project(None, Some(json!({"closed":true}))))
    }
}

fn navigation_result(result: &Value) -> Result<(), BrowserError> {
    if result.get("errorText").is_some() {
        return Err(BrowserError::new(
            BrowserErrorCode::BrowserNavigationFailed,
            "Obscura could not load the requested page",
        ));
    }
    Ok(())
}
