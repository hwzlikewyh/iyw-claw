"use client"

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
import type { PluginPreparation } from "@/lib/plugin-preparation"
import { pluginPreparationIssueKey } from "@/lib/plugin-preparation-copy"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"

export function PluginDependencyRows({ view }: { view: PluginPreparation }) {
  const t = useTranslations("PluginPreparation")
  const route = useWorkbenchRoute()
  if (!view.dependencies.verified)
    return <p className="text-sm text-destructive">{t("snapshotMissing")}</p>
  const dependencies = view.dependencies.dependencies
  if (!dependencies.length)
    return (
      <p className="text-[13px] text-muted-foreground">{t("noDependencies")}</p>
    )
  return (
    <ul className="space-y-2 text-[13px]">
      {dependencies.map((dependency, index) => (
        <li
          key={`${dependency.id}:${index}`}
          className="flex flex-wrap justify-between gap-2"
        >
          <Button
            variant="link"
            size="xs"
            className="h-auto min-w-0 whitespace-normal break-words p-0 text-left text-[13px]"
            onClick={() =>
              route.openSkillMarket(dependency.slug, dependency.packageType)
            }
          >
            {dependency.slug} · {dependency.requiredVersion}
          </Button>
          <span
            className={
              dependency.ready ? "text-muted-foreground" : "text-destructive"
            }
          >
            {dependency.reason && dependency.reason !== "dependency_unavailable"
              ? t(pluginPreparationIssueKey(dependency.reason))
              : dependency.ready
                ? t("dependencyReady")
                : dependency.installedVersion
                  ? t("dependencyVersion", {
                      version: dependency.installedVersion,
                    })
                  : t("dependencyMissing")}
          </span>
        </li>
      ))}
    </ul>
  )
}

interface ReviewProps {
  view: PluginPreparation
  enabled: boolean
  busy: boolean
  error: string
  onConfirm: () => void
  onClose: () => void
}

export function PluginPermissionReview({
  view,
  enabled,
  busy,
  error,
  onConfirm,
  onClose,
}: ReviewProps) {
  const t = useTranslations("PluginPreparation")
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose()
      }}
    >
      <DialogContent className="flex max-h-[85dvh] flex-col">
        <DialogHeader>
          <DialogTitle>
            {t(enabled ? "approveTitle" : "disableTitle")}
          </DialogTitle>
          <DialogDescription>
            {t(enabled ? "approveHint" : "disableHint", {
              workspace: view.workspaceName || "—",
            })}
          </DialogDescription>
        </DialogHeader>
        <ReviewBody view={view} error={error} />
        <DialogFooter>
          <Button variant="outline" disabled={busy} onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button
            variant={enabled ? "default" : "destructive"}
            disabled={busy}
            onClick={onConfirm}
          >
            {t(busy ? "saving" : enabled ? "confirmApprove" : "confirmDisable")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function ReviewBody({ view, error }: Pick<ReviewProps, "view" | "error">) {
  return (
    <div className="min-h-0 space-y-4 overflow-y-auto">
      <p className="break-words text-sm">
        {view.slug} · {view.installedVersion}
      </p>
      <PermissionRows view={view} />
      {error ? (
        <p role="alert" className="break-words text-sm text-destructive">
          {error}
        </p>
      ) : null}
    </div>
  )
}

function PermissionRows({ view }: { view: PluginPreparation }) {
  const t = useTranslations("PluginPreparation")
  const permissions = view.permissions
  const rows = [
    ["read", permissions?.workspace.read || []],
    ["write", permissions?.workspace.write || []],
    ["network", permissions?.network.connectDomains || []],
    ["resources", permissions?.network.resourceDomains || []],
    ["frames", permissions?.network.frameDomains || []],
    ["baseUris", permissions?.network.baseUriDomains || []],
    ["host", (permissions?.host || []).map((action) => t(HOST_LABELS[action]))],
  ] as const
  return (
    <dl className="space-y-3 text-sm">
      {rows.map(([label, values]) => (
        <div key={label}>
          <dt className="font-medium">{t(`permissions.${label}`)}</dt>
          <dd className="mt-1 break-words text-muted-foreground">
            {values.length ? values.join(" · ") : t("none")}
          </dd>
        </div>
      ))}
    </dl>
  )
}

const HOST_LABELS = {
  "send-message": "hostActions.sendMessage",
  "clipboard-write": "hostActions.clipboardWrite",
  "open-link": "hostActions.openLink",
  camera: "hostActions.camera",
  microphone: "hostActions.microphone",
  geolocation: "hostActions.geolocation",
} as const
