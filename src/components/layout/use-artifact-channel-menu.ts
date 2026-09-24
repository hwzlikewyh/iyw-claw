"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import type { TaskArtifactInfo } from "@/lib/api"
import { randomUUID } from "@/lib/utils"
import {
  listArtifactChannelTargets,
  sendArtifactToChannels,
  type ArtifactChannelTarget,
  type ArtifactDeliveryResult,
} from "@/lib/artifact-notifications"

type MenuTranslation = ReturnType<
  typeof useTranslations<"Folder.taskArtifacts.channelSend">
>

function translated(
  t: MenuTranslation,
  value: string,
  fallback: "failed" | "unknown" = "failed"
) {
  const key = value as Parameters<MenuTranslation>[0]
  return t.has(key) ? t(key) : t(fallback)
}

const RESULT_TOAST_DURATION_MS = 10_000

function resultDetails(
  result: ArtifactDeliveryResult,
  t: MenuTranslation
): string {
  const errors = [
    result.error,
    result.reason,
    result.message_error,
    result.file_error,
    ...(result.artifact_errors?.map((item) => item.error) ?? []),
  ].filter(Boolean)
  const status = translated(t, result.status, "unknown")
  const detail = errors.map((error) => translated(t, `errors.${error}`))
  const children = result.items?.map((item) => resultDetails(item, t)) ?? []
  return [result.channel_name, status, ...new Set([...detail, ...children])]
    .filter(Boolean)
    .join(": ")
}

function useArtifactSender(artifact: TaskArtifactInfo | undefined) {
  const t = useTranslations("Folder.taskArtifacts.channelSend")
  const [sending, setSending] = useState(false)
  const active = useRef(false)
  const send = useCallback(
    async (channelId?: number) => {
      if (!artifact || active.current) return
      active.current = true
      setSending(true)
      const toastId = toast.loading(t("sending"))
      try {
        const result = await sendArtifactToChannels({
          artifactId: artifact.id,
          conversationId: artifact.conversationId,
          channelId,
          requestId: randomUUID(),
        })
        const description = resultDetails(result, t)
        if (result.status === "success")
          toast.success(t("success"), { id: toastId, description })
        else
          toast.warning(translated(t, result.status, "unknown"), {
            id: toastId,
            description,
            duration: RESULT_TOAST_DURATION_MS,
          })
      } catch {
        toast.error(t("unknown"), { id: toastId })
      } finally {
        active.current = false
        setSending(false)
      }
    },
    [artifact, t]
  )
  return { send, sending }
}

function useChannelTargets(artifact: TaskArtifactInfo | undefined) {
  const [targets, setTargets] = useState<ArtifactChannelTarget[]>([])
  const [loading, setLoading] = useState(true)
  const [failed, setFailed] = useState(false)
  const load = useCallback(async () => {
    if (!artifact || artifact.status !== "available") return
    setLoading(true)
    try {
      setTargets(await listArtifactChannelTargets())
      setFailed(false)
    } catch {
      setFailed(true)
    } finally {
      setLoading(false)
    }
  }, [artifact])
  useEffect(() => {
    void load()
  }, [load])
  return { targets, loading, failed, load }
}

function channelLabel(target: ArtifactChannelTarget, t: MenuTranslation) {
  const label = t("sendTo", { name: target.name })
  const recipient = target.targetName ? ` · ${target.targetName}` : ""
  const error = target.error
    ? ` (${translated(t, `errors.${target.error}`)})`
    : ""
  return `${label}${recipient}${error}`
}

export function useArtifactChannelMenu(artifact: TaskArtifactInfo | undefined) {
  const t = useTranslations("Folder.taskArtifacts.channelSend")
  const { targets, loading, failed, load } = useChannelTargets(artifact)
  const { send, sending } = useArtifactSender(artifact)
  if (!artifact || artifact.status !== "available") return []
  if (loading)
    return [
      {
        id: "channelsLoading",
        label: t("loading"),
        disabled: true,
        onSelect: () => {},
      },
    ]
  if (failed)
    return [
      {
        id: "channelsRetry",
        label: t("loadFailed"),
        onSelect: () => void load(),
      },
    ]
  const entries = targets.map((target) => ({
    id: `channel-${target.channelId}`,
    label: channelLabel(target, t),
    disabled: sending || Boolean(target.error),
    onSelect: () => void send(target.channelId),
  }))
  if (targets.length > 1)
    entries.push({
      id: "channelsAll",
      label: t("sendAll"),
      disabled: sending || !targets.some((target) => !target.error),
      onSelect: () => void send(),
    })
  return entries
}
