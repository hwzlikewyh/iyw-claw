"use client"

import { useTranslations } from "next-intl"
import {
  GenerateSkillPanel,
  ImportSkillPanel,
  type SkillContentRequest,
} from "@/components/settings/skill-market-panels"
import { SkillMarketDeleteDialog } from "@/components/skills/skill-market-delete-dialog"
import { SkillMarketLocalToolDialog } from "@/components/skills/skill-market-local-tool-dialog"
import { SkillMarketVersionDialog } from "@/components/skills/skill-market-management-dialogs"
import { SkillMarketMetadataDialog } from "@/components/skills/skill-market-metadata-dialog"
import { SkillMarketUninstallDialog } from "@/components/skills/skill-market-uninstall-dialog"
import { SkillMarketUploadDialog } from "@/components/skills/skill-market-upload-dialog"
import type { useSkillMarketActions } from "@/components/skills/skill-market-use-actions"
import type { useSkillMarketData } from "@/components/skills/skill-market-use-data"
import type { AgentSkillItem } from "@/lib/types"

type MarketData = ReturnType<typeof useSkillMarketData>
type MarketActions = ReturnType<typeof useSkillMarketActions>
export type WorkspaceTool = "upload" | "import" | "generate" | null

export type WorkspaceDialogState = {
  tool: WorkspaceTool
  metadataOpen: boolean
  versionOpen: boolean
  deleteOpen: boolean
  uninstallOpen: boolean
  setTool: (tool: WorkspaceTool) => void
  setMetadataOpen: (open: boolean) => void
  setVersionOpen: (open: boolean) => void
  setDeleteOpen: (open: boolean) => void
  setUninstallOpen: (open: boolean) => void
}

type DialogProps = {
  dialogs: WorkspaceDialogState
  data: MarketData
  actions: MarketActions
  activeLocal: AgentSkillItem | null
  targetName: string | null
  installDisabled: boolean
  localBusyId: string | null
  onDeleteRemote: () => Promise<void>
  onDeleteLocal: (skill: AgentSkillItem) => Promise<void>
  onUninstalled: () => void
  onImport: (request: SkillContentRequest) => Promise<void>
  onGenerate: (request: SkillContentRequest) => Promise<void>
}

function RemoteDialogs(props: DialogProps) {
  return (
    <>
      <SkillMarketUploadDialog
        open={props.dialogs.tool === "upload"}
        categories={props.data.categories}
        busy={props.actions.busyAction === "publish"}
        onOpenChange={(open) => !open && props.dialogs.setTool(null)}
        onPublish={props.actions.publish}
      />
      <SkillMarketMetadataDialog
        open={props.dialogs.metadataOpen}
        detail={props.data.detail}
        categories={props.data.categories}
        busy={props.actions.busyAction === "metadata"}
        onOpenChange={props.dialogs.setMetadataOpen}
        onSave={props.actions.updateMetadata}
      />
      <SkillMarketVersionDialog
        open={props.dialogs.versionOpen}
        detail={props.data.detail}
        busy={props.actions.busyAction === "version"}
        onOpenChange={props.dialogs.setVersionOpen}
        onPublish={props.actions.addVersion}
      />
      <SkillMarketDeleteDialog
        detail={props.data.detail}
        open={props.dialogs.deleteOpen}
        busy={props.actions.busyAction === "delete"}
        onOpenChange={props.dialogs.setDeleteOpen}
        onDelete={props.onDeleteRemote}
      />
    </>
  )
}

async function confirmUninstall(props: DialogProps) {
  if (!props.activeLocal) return
  await props.onDeleteLocal(props.activeLocal)
  props.dialogs.setUninstallOpen(false)
  props.onUninstalled()
}

function UninstallDialog(props: DialogProps) {
  return (
    <SkillMarketUninstallDialog
      skill={props.activeLocal}
      open={props.dialogs.uninstallOpen}
      busy={props.localBusyId === props.activeLocal?.id}
      onOpenChange={props.dialogs.setUninstallOpen}
      onConfirm={() => confirmUninstall(props)}
    />
  )
}

function runLocalTool({
  props,
  key,
  action,
  message,
}: {
  props: DialogProps
  key: "import" | "generate"
  action: () => Promise<void>
  message: string
}) {
  return props.actions
    .run(key, action, message)
    .then(() => props.dialogs.setTool(null))
}

function LocalToolDialogs(props: DialogProps) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <>
      <SkillMarketLocalToolDialog
        open={props.dialogs.tool === "import"}
        title={t("import.title")}
        onOpenChange={(open) => !open && props.dialogs.setTool(null)}
      >
        <ImportSkillPanel
          targetName={props.targetName}
          disabled={props.installDisabled}
          busy={props.actions.busyAction === "import"}
          onImport={(request) =>
            void runLocalTool({
              props,
              key: "import",
              action: () => props.onImport(request),
              message: t("toasts.imported"),
            }).catch(() => {})
          }
        />
      </SkillMarketLocalToolDialog>
      <SkillMarketLocalToolDialog
        open={props.dialogs.tool === "generate"}
        title={t("generate.title")}
        onOpenChange={(open) => !open && props.dialogs.setTool(null)}
      >
        <GenerateSkillPanel
          targetName={props.targetName}
          disabled={props.installDisabled}
          busy={props.actions.busyAction === "generate"}
          onGenerate={(request) =>
            void runLocalTool({
              props,
              key: "generate",
              action: () => props.onGenerate(request),
              message: t("toasts.generated"),
            }).catch(() => {})
          }
        />
      </SkillMarketLocalToolDialog>
    </>
  )
}

export function SkillMarketWorkspaceDialogs(props: DialogProps) {
  return (
    <>
      <RemoteDialogs {...props} />
      <UninstallDialog {...props} />
      <LocalToolDialogs {...props} />
    </>
  )
}
