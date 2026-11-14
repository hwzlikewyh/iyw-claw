"use client"

import {
  useCallback,
  useRef,
  useState,
  type Dispatch,
  type RefObject,
  type SetStateAction,
} from "react"
import { useTranslations } from "next-intl"
import { extractAppCommandError, toErrorMessage } from "@/lib/app-error"
import {
  skillMarketInstall,
  type SkillMarketInstallErrorCode,
  type SkillMarketInstallPlanV2,
  type SkillMarketInstallSession,
  type SkillMarketV2Item,
} from "@/lib/skill-market"
import {
  getSkillMarketSource,
  SkillMarketSourceError,
} from "@/lib/skill-market-source"
import { recordSkillMarketMetric } from "@/hooks/use-skill-market"
import type { AgentType } from "@/lib/types"
import { notifySkillMarketInstallationsChanged } from "@/lib/skill-market-installation-events"

const INITIAL_SESSION: SkillMarketInstallSession = {
  status: "idle",
  plan: null,
  items: [],
  overallBytes: 0,
  receivedBytes: 0,
  errorCode: null,
  errorMessage: null,
  startedAt: null,
  refreshingTicket: false,
  ticketRefreshCount: 0,
}

function installErrorCode(error: unknown): SkillMarketInstallErrorCode {
  if (error instanceof SkillMarketSourceError) return error.code
  if (
    extractAppCommandError(error)?.i18n_key ===
    "SkillMarketV2.install.clientIncompatible"
  )
    return "client_incompatible"
  if (
    extractAppCommandError(error)?.i18n_key ===
    "SkillMarketV2.install.platformUnsupported"
  )
    return "platform_unsupported"
  const code = extractAppCommandError(error)?.code
  if (code === "artifact_not_ready") return "artifact_not_ready"
  if (code === "permission_denied") return "audience_denied"
  if (code === "dependency_missing") return "dependency_unavailable"
  return "download_failed"
}

export function useSkillMarketInstall() {
  const displayError = useInstallErrorMessage()
  const [session, setSession] =
    useState<SkillMarketInstallSession>(INITIAL_SESSION)
  const planRef = useRef<SkillMarketInstallPlanV2 | null>(null)
  const resolveRef = useRef<{
    item: SkillMarketV2Item
    version: string
  } | null>(null)

  const beginResolve = useInstallResolve({
    planRef,
    resolveRef,
    setSession,
    displayError,
  })

  const start = useInstallStart({ planRef, setSession, displayError })

  const retry = useCallback(() => {
    const pending = resolveRef.current
    if (pending) void beginResolve(pending.item, pending.version)
  }, [beginResolve])

  const reset = useCallback(() => {
    planRef.current = null
    resolveRef.current = null
    setSession(INITIAL_SESSION)
  }, [])

  return { session, beginResolve, start, retry, reset }
}

function useInstallErrorMessage() {
  const t = useTranslations("SkillMarketV2")
  const displayError = useCallback(
    (error: unknown) => {
      const details = extractAppCommandError(error)
      if (details?.i18n_key === "SkillMarketV2.install.clientIncompatible")
        return t("install.clientIncompatible", {
          minimum: details.i18n_params?.minimum || "",
        })
      return details?.i18n_key === "SkillMarketV2.install.platformUnsupported"
        ? t("install.platformUnsupported", {
            platform: details.i18n_params?.platform || "",
          })
        : toErrorMessage(error)
    },
    [t]
  )
  return displayError
}

function useInstallStart({
  planRef,
  setSession,
  displayError,
}: {
  planRef: RefObject<SkillMarketInstallPlanV2 | null>
  setSession: Dispatch<SetStateAction<SkillMarketInstallSession>>
  displayError: (error: unknown) => string
}) {
  return useCallback(
    async (agentTypes: AgentType[]) => {
      const plan = planRef.current
      if (!plan || agentTypes.length === 0) return
      setSession((current) => ({
        ...current,
        status: "running",
        errorCode: null,
        errorMessage: null,
        startedAt: Date.now(),
      }))
      try {
        await skillMarketInstall(
          plan.targetSkillId,
          plan.targetVersion,
          agentTypes
        )
        notifySkillMarketInstallationsChanged()
        setSession((current) => ({
          ...current,
          status: "done",
          receivedBytes: current.overallBytes,
        }))
      } catch (error) {
        setSession((current) => ({
          ...current,
          status: "failed",
          errorCode: installErrorCode(error),
          errorMessage: displayError(error),
        }))
      }
    },
    [displayError, planRef, setSession]
  )
}

interface InstallResolveContext {
  planRef: RefObject<SkillMarketInstallPlanV2 | null>
  resolveRef: RefObject<{ item: SkillMarketV2Item; version: string } | null>
  setSession: Dispatch<SetStateAction<SkillMarketInstallSession>>
  displayError: (error: unknown) => string
}

function useInstallResolve({
  planRef,
  resolveRef,
  setSession,
  displayError,
}: InstallResolveContext) {
  return useCallback(
    async (item: SkillMarketV2Item, version: string) => {
      resolveRef.current = { item, version }
      const startedAt = performance.now()
      setSession({ ...INITIAL_SESSION, status: "resolving" })
      try {
        const plan = await getSkillMarketSource().resolve(item.id, version)
        recordSkillMarketMetric("actionReady", performance.now() - startedAt)
        planRef.current = plan
        setSession({
          ...INITIAL_SESSION,
          status: "confirming",
          plan,
          overallBytes: plan.totalBytes,
        })
      } catch (error) {
        setSession((current) => ({
          ...current,
          status: "failed",
          errorCode: installErrorCode(error),
          errorMessage: displayError(error),
        }))
      }
    },
    [displayError, planRef, resolveRef, setSession]
  )
}
