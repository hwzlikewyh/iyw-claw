"use client"

import { useCallback, useRef, useState } from "react"
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
  const code = extractAppCommandError(error)?.code
  if (code === "artifact_not_ready") return "artifact_not_ready"
  if (code === "permission_denied") return "audience_denied"
  if (code === "dependency_missing") return "dependency_unavailable"
  return "download_failed"
}

export function useSkillMarketInstall() {
  const [session, setSession] =
    useState<SkillMarketInstallSession>(INITIAL_SESSION)
  const planRef = useRef<SkillMarketInstallPlanV2 | null>(null)
  const resolveRef = useRef<{
    item: SkillMarketV2Item
    version: string
  } | null>(null)

  const beginResolve = useCallback(
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
          errorMessage: toErrorMessage(error),
        }))
      }
    },
    []
  )

  const start = useCallback(async (agentTypes: AgentType[]) => {
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
        errorMessage: toErrorMessage(error),
      }))
    }
  }, [])

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
