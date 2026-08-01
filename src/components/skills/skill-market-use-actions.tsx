"use client"

import { useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { extractAppCommandError, toErrorMessage } from "@/lib/app-error"
import {
  skillMarketAddVersion,
  skillMarketDelete,
  skillMarketInstall,
  skillMarketPublish,
  skillMarketUpdateMetadata,
  type SkillMarketAddVersionRequest,
  type SkillMarketDetail,
  type SkillMarketMetadataRequest,
  type SkillMarketPublishRequest,
} from "@/lib/skill-market"
import type { AgentType } from "@/lib/types"

type ActionOptions = {
  detail: SkillMarketDetail | null
  agentType: AgentType | null
  installedVersion: string | null
  onInstalledChanged: () => Promise<void>
  onRefresh: () => void
}

type ActionRunner = (
  key: string,
  action: () => Promise<void>,
  message: string
) => Promise<void>

function useActionRunner(onRefresh: () => void) {
  const t = useTranslations("SkillsSettings.market")
  const [busyAction, setBusyAction] = useState<string | null>(null)
  const run: ActionRunner = async (
    key: string,
    action: () => Promise<void>,
    message: string
  ) => {
    setBusyAction(key)
    try {
      await action()
      toast.success(message)
      onRefresh()
    } catch (error) {
      if (extractAppCommandError(error)?.code === "artifact_not_ready") {
        toast.error(t("toasts.artifactNotReady"), {
          description: toErrorMessage(error),
        })
      } else {
        toast.error(t("toasts.actionFailed"), {
          description: toErrorMessage(error),
        })
      }
      throw error
    } finally {
      setBusyAction(null)
    }
  }
  return { busyAction, run, t }
}

function createRemoteActions(
  detail: SkillMarketDetail | null,
  run: ActionRunner,
  t: ReturnType<typeof useTranslations>
) {
  const publish = (request: SkillMarketPublishRequest) =>
    run(
      "publish",
      async () => void (await skillMarketPublish(request)),
      t("toasts.published")
    )
  const updateMetadata = (request: SkillMarketMetadataRequest) =>
    run(
      "metadata",
      async () => void (await skillMarketUpdateMetadata(request)),
      t("toasts.metadataUpdated")
    )
  const addVersion = (request: SkillMarketAddVersionRequest) =>
    run(
      "version",
      async () => void (await skillMarketAddVersion(request)),
      t("toasts.versionPublished")
    )
  const deleteRemote = () =>
    detail
      ? run("delete", () => skillMarketDelete(detail.id), t("toasts.deleted"))
      : Promise.resolve()
  return { publish, updateMetadata, addVersion, deleteRemote }
}

export function useSkillMarketActions(options: ActionOptions) {
  const { busyAction, run, t } = useActionRunner(options.onRefresh)
  const install = async (version: string) => {
    const { detail, agentType } = options
    if (!detail || !agentType) {
      toast.error(t("toasts.noTarget"))
      return
    }
    await run(
      "install",
      async () => {
        await skillMarketInstall(detail.id, version, agentType)
        await options.onInstalledChanged()
      },
      t(options.installedVersion ? "toasts.updated" : "toasts.installed")
    )
  }
  const remote = createRemoteActions(options.detail, run, t)
  return {
    busyAction,
    run,
    install,
    ...remote,
  }
}
