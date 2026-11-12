"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import type { McpMarketplaceItem } from "@/lib/types"
import type { ConnectorInstallTask } from "@/lib/connector-install-tasks"
import { ConnectorInstallFields } from "./connector-install-fields"
import { ConnectorInstallTaskStatus } from "./connector-install-task-status"
import { ConnectorOAuthDialog } from "./connector-oauth-dialog"
import { useConnectorInstall } from "./use-connector-install"

type InstallState = ReturnType<typeof useConnectorInstall>

type ConnectorInstallDialogProps = {
  item: McpMarketplaceItem
  task?: ConnectorInstallTask
  onClose: () => void
  onTaskChanged: () => void
  onManageInstalled: () => void
  installed: boolean
}

export function ConnectorInstallDialog(props: ConnectorInstallDialogProps) {
  const state = useConnectorInstall(props)
  const [oauth, setOAuth] = useState(false)
  const [editing, setEditing] = useState(false)
  const showForm =
    !state.busy &&
    (editing || !props.installed || state.task?.phase !== "completed")
  const begin = async () => {
    if (await state.install()) setEditing(false)
  }
  if (oauth)
    return (
      <ConnectorOAuthDialog
        id={props.item.server_id}
        onClose={() => setOAuth(false)}
      />
    )
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) props.onClose()
      }}
    >
      <DialogContent className="flex max-h-[85dvh] flex-col gap-0 overflow-hidden p-0">
        <DialogHeader className="p-6 pb-4">
          <DialogTitle>{props.item.name}</DialogTitle>
          <DialogDescription className="whitespace-pre-line">
            {state.detail?.description || props.item.description}
          </DialogDescription>
        </DialogHeader>
        <div className="min-h-0 space-y-4 overflow-y-auto px-6 pb-4">
          <InstallationBody state={state} showForm={showForm} />
        </div>
        <InstallationFooter
          state={state}
          onClose={props.onClose}
          onAuthorize={() => setOAuth(true)}
          onManageInstalled={props.onManageInstalled}
          onReconfigure={() => setEditing(true)}
          onBegin={() => void begin()}
          showForm={showForm}
        />
      </DialogContent>
    </Dialog>
  )
}

function InstallationBody({
  state,
  showForm,
}: {
  state: InstallState
  showForm: boolean
}) {
  const t = useTranslations("ConnectorMarket")
  return (
    <>
      <p className="text-xs text-muted-foreground">{t("installHint")}</p>
      {state.task ? (
        <ConnectorInstallTaskStatus
          task={state.task}
          onCancel={() => void state.cancel()}
        />
      ) : null}
      {showForm ? (
        <InstallationOptions state={state} />
      ) : (
        <InstallationInfo state={state} />
      )}
      {showForm ? (
        <ConnectorInstallFields
          fields={state.option?.parameters || []}
          values={state.values}
          disabled={state.busy}
          onChange={state.setValue}
        />
      ) : null}
      {state.error ? (
        <p role="alert" className="text-sm text-destructive">
          {state.error}
        </p>
      ) : null}
      {!state.detail && state.error ? (
        <Button variant="outline" onClick={state.reload}>
          {t("retry")}
        </Button>
      ) : null}
    </>
  )
}

function InstallationFooter({
  state,
  onClose,
  onAuthorize,
  onManageInstalled,
  onReconfigure,
  onBegin,
  showForm,
}: {
  state: InstallState
  onClose: () => void
  onAuthorize: () => void
  onManageInstalled: () => void
  onReconfigure: () => void
  onBegin: () => void
  showForm: boolean
}) {
  const t = useTranslations("ConnectorMarket")
  const auth = (
    state.option?.spec.iyw_install as { auth_mode?: string } | undefined
  )?.auth_mode
  return (
    <DialogFooter className="border-t p-4 sm:px-6">
      <Button variant="outline" onClick={onClose}>
        {state.busy ? t("continueInBackground") : t("close")}
      </Button>
      {state.task?.phase === "completed" && auth === "cli" ? (
        <Button onClick={onManageInstalled}>{t("cliLogin")}</Button>
      ) : null}
      {state.task?.phase === "completed" && auth === "oauth" ? (
        <Button onClick={onAuthorize}>{t("authorize")}</Button>
      ) : null}
      {state.task?.phase === "completed" && !showForm ? (
        <>
          <Button variant="outline" onClick={onReconfigure}>
            {t("configure")}
          </Button>
          <Button onClick={onManageInstalled}>{t("manageInstalled")}</Button>
        </>
      ) : null}
      {showForm ? (
        <Button disabled={state.busy || !state.option} onClick={onBegin}>
          {state.busy
            ? t("installing")
            : state.task && state.task.phase !== "completed"
              ? t("retry")
              : t("install")}
        </Button>
      ) : null}
    </DialogFooter>
  )
}

function InstallationOptions({ state }: { state: InstallState }) {
  const t = useTranslations("ConnectorMarket")
  return (
    <>
      {state.detail && !state.detail.install_options.length ? (
        <p role="alert" className="text-sm text-destructive">
          {t("incompatible")}
        </p>
      ) : null}
      {state.option?.description ? (
        <p className="text-xs text-muted-foreground">
          {state.option.description}
        </p>
      ) : null}
      <InstallationInfo state={state} />
      {state.detail ? (
        <label className="grid gap-2 text-sm">
          {t("connectionOption")}
          <select
            value={state.optionId}
            disabled={state.busy}
            className="h-10 rounded-md border bg-background px-2"
            onChange={(event) => state.setOption(event.target.value)}
          >
            {state.detail.install_options.map((option) => (
              <option key={option.id} value={option.id}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
      ) : null}
    </>
  )
}

function InstallationInfo({ state }: { state: InstallState }) {
  const t = useTranslations("ConnectorMarket")
  const installation = state.option?.spec.iyw_install as
    | { artifact?: boolean; auth_mode?: string }
    | undefined
  if (!state.option) return null
  return (
    <div className="space-y-1 rounded-md bg-muted p-3 text-sm">
      <p>
        {t("runtime")}:{" "}
        {t(
          state.option.protocol === "stdio" ? "runtime_local" : "runtime_remote"
        )}
      </p>
      <p>
        {t("authMode")}:{" "}
        {t(
          (
            {
              none: "authNone",
              env: "authParameters",
              header: "authParameters",
              oauth: "authBrowser",
              cli: "cliLogin",
            } as const
          )[
            installation?.auth_mode as
              | "none"
              | "env"
              | "header"
              | "oauth"
              | "cli"
          ] || "authNone"
        )}
      </p>
      {installation?.artifact ? (
        <p className="text-xs text-muted-foreground">{t("artifactDownload")}</p>
      ) : null}
    </div>
  )
}
