"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import { useSearchParams } from "next/navigation"
import { toErrorMessage } from "@/lib/app-error"
import type {
  SkillMarketInstallArtifactProgress,
  SkillMarketInstallErrorCode,
  SkillMarketInstallPlanV2,
  SkillMarketInstallSession,
  SkillMarketV2Item,
} from "@/lib/skill-market"
import {
  getSkillMarketSource,
  SkillMarketSourceError,
} from "@/lib/skill-market-source"
import { recordSkillMarketMetric } from "@/hooks/use-skill-market"

const TICK_MS = 125
const VERIFY_MS = 400
const EXTRACT_MS = 400
const ACTIVATE_MS = 800
const TICKET_REFRESH_AFTER_MS = 600
const TICKET_REFRESH_DURATION_MS = 900

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

function parseInstallFail(value: string | null): SkillMarketInstallErrorCode | null {
  switch (value) {
    case "disk_full":
    case "download_failed":
    case "checksum_mismatch":
    case "signature_invalid":
    case "plan_expired":
    case "catalog_stale":
      return value
    default:
      return null
  }
}

interface SimState {
  plan: SkillMarketInstallPlanV2
  t0: number
  failCode: SkillMarketInstallErrorCode | null
  failItemId: string | null
  activatedAt: number | null
  ticketRefreshedAt: number | null
  ticketRefreshCount: number
  refreshingTicket: boolean
  items: SkillMarketInstallArtifactProgress[]
  overall: SkillMarketInstallSession["status"]
  receivedBytes: number
}

function initialItems(plan: SkillMarketInstallPlanV2): SkillMarketInstallArtifactProgress[] {
  return plan.items.map((planItem) => ({
    artifactId: planItem.artifactId,
    displayName: planItem.displayName,
    version: planItem.version,
    phase: "pending",
    bytesReceived: 0,
    bytesTotal: planItem.artifactSize,
    errorCode: null,
    message: null,
  }))
}

function advanceItem(
  item: SkillMarketInstallArtifactProgress,
  elapsed: number,
  failCode: SkillMarketInstallErrorCode | null,
  failItemId: string | null
): SkillMarketInstallArtifactProgress {
  if (
    item.phase === "done" ||
    item.phase === "failed" ||
    item.phase === "canceled"
  ) {
    return item
  }
  const downloadMs = 1400 + (item.bytesTotal % 600)
  const ratio = Math.min(1, elapsed / downloadMs)
  if (failCode && item.artifactId === failItemId && ratio >= 0.6) {
    return {
      ...item,
      phase: "failed",
      bytesReceived: Math.round(item.bytesTotal * ratio),
      errorCode: failCode,
      message: failCode,
    }
  }
  if (elapsed < downloadMs) {
    return {
      ...item,
      phase: "downloading",
      bytesReceived: Math.round(item.bytesTotal * ratio),
    }
  }
  if (elapsed < downloadMs + VERIFY_MS) {
    return { ...item, phase: "verifying", bytesReceived: item.bytesTotal }
  }
  if (elapsed < downloadMs + VERIFY_MS + EXTRACT_MS) {
    return { ...item, phase: "extracting", bytesReceived: item.bytesTotal }
  }
  return { ...item, phase: "done", bytesReceived: item.bytesTotal }
}

export function useSkillMarketInstall() {
  const searchParams = useSearchParams()
  const failCode = parseInstallFail(searchParams.get("installFail"))

  const [session, setSession] =
    useState<SkillMarketInstallSession>(INITIAL_SESSION)
  const simRef = useRef<SimState | null>(null)
  const planRef = useRef<SkillMarketInstallPlanV2 | null>(null)
  const timerRef = useRef<number | null>(null)

  const stopTimer = useCallback(() => {
    if (timerRef.current !== null) {
      window.clearInterval(timerRef.current)
      timerRef.current = null
    }
  }, [])

  useEffect(() => stopTimer, [stopTimer])

  const syncSession = useCallback(() => {
    const sim = simRef.current
    if (!sim) return
    setSession({
      status: sim.overall,
      plan: sim.plan,
      items: sim.items.map((item) => ({ ...item })),
      overallBytes: sim.plan.totalBytes,
      receivedBytes: sim.receivedBytes,
      errorCode:
        sim.items.find((item) => item.phase === "failed")?.errorCode ?? null,
      errorMessage:
        sim.items.find((item) => item.phase === "failed")?.message ?? null,
      startedAt: sim.t0,
      refreshingTicket: sim.refreshingTicket,
      ticketRefreshCount: sim.ticketRefreshCount,
    })
  }, [])

  const advanceSim = useCallback(() => {
    const sim = simRef.current
    if (!sim) return
    const now = Date.now()
    const elapsed = now - sim.t0

    if (sim.ticketRefreshedAt === null && elapsed >= TICKET_REFRESH_AFTER_MS) {
      sim.ticketRefreshedAt = now
      sim.refreshingTicket = true
      window.setTimeout(() => {
        sim.refreshingTicket = false
        sim.ticketRefreshCount += 1
        syncSession()
      }, TICKET_REFRESH_DURATION_MS)
    }

    sim.items = sim.items.map((item) =>
      advanceItem(item, elapsed, sim.failCode, sim.failItemId)
    )
    const allDone = sim.items.every((item) => item.phase === "done")
    const failed = sim.items.some((item) => item.phase === "failed")
    const canceled = sim.items.some((item) => item.phase === "canceled")
    if (sim.overall === "running" && allDone) {
      sim.overall = "activating"
      sim.activatedAt = now
    }
    if (sim.overall === "activating" && sim.activatedAt !== null) {
      if (now - sim.activatedAt >= ACTIVATE_MS) sim.overall = "done"
    }
    if (failed) sim.overall = "failed"
    if (canceled) sim.overall = "canceled"
    sim.receivedBytes = sim.items.reduce(
      (sum, item) => sum + item.bytesReceived,
      0
    )
    syncSession()
  }, [syncSession])

  const startTimer = useCallback(() => {
    stopTimer()
    timerRef.current = window.setInterval(() => {
      advanceSim()
      const sim = simRef.current
      if (
        sim &&
        (sim.overall === "done" ||
          sim.overall === "failed" ||
          sim.overall === "canceled")
      ) {
        stopTimer()
      }
    }, TICK_MS)
  }, [advanceSim, stopTimer])

  const beginResolve = useCallback(
    async (item: SkillMarketV2Item, version: string) => {
      stopTimer()
      const startedAt = performance.now()
      setSession((current) => ({
        ...current,
        status: "resolving",
        errorCode: null,
        errorMessage: null,
      }))
      try {
        const plan = await getSkillMarketSource().resolve(item.id, version)
        recordSkillMarketMetric("actionReady", performance.now() - startedAt)
        planRef.current = plan
        setSession({
          status: "confirming",
          plan,
          items: initialItems(plan),
          overallBytes: plan.totalBytes,
          receivedBytes: 0,
          errorCode: null,
          errorMessage: null,
          startedAt: null,
          refreshingTicket: false,
          ticketRefreshCount: 0,
        })
      } catch (error) {
        const code =
          error instanceof SkillMarketSourceError
            ? error.code
            : "download_failed"
        setSession((current) => ({
          ...current,
          status: "failed",
          errorCode: code,
          errorMessage: toErrorMessage(error),
        }))
      }
    },
    [stopTimer]
  )

  const start = useCallback(() => {
    const plan = planRef.current
    if (!plan) return
    stopTimer()
    simRef.current = {
      plan,
      t0: Date.now(),
      failCode,
      failItemId: plan.items[0]?.artifactId ?? null,
      activatedAt: null,
      ticketRefreshedAt: null,
      ticketRefreshCount: 0,
      refreshingTicket: false,
      items: initialItems(plan),
      overall: "running",
      receivedBytes: 0,
    }
    syncSession()
    startTimer()
  }, [failCode, startTimer, stopTimer, syncSession])

  const cancel = useCallback(() => {
    stopTimer()
    setSession((current) => {
      if (
        current.status !== "running" &&
        current.status !== "activating"
      ) {
        return current
      }
      if (current.status === "activating") return current
      const sim = simRef.current
      if (sim) {
        sim.overall = "canceled"
        sim.items = sim.items.map((item) =>
          item.phase === "done" || item.phase === "failed"
            ? item
            : { ...item, phase: "canceled", errorCode: "canceled" }
        )
      }
      return {
        ...current,
        status: "canceled",
        items: sim?.items ?? current.items,
        errorCode: "canceled",
        errorMessage: null,
      }
    })
  }, [stopTimer])

  const retry = useCallback(() => {
    const plan = planRef.current
    if (!plan) return
    stopTimer()
    simRef.current = {
      plan,
      t0: Date.now(),
      failCode,
      failItemId: plan.items[0]?.artifactId ?? null,
      activatedAt: null,
      ticketRefreshedAt: null,
      ticketRefreshCount: 0,
      refreshingTicket: false,
      items: initialItems(plan),
      overall: "running",
      receivedBytes: 0,
    }
    syncSession()
    startTimer()
  }, [failCode, startTimer, stopTimer, syncSession])

  const reset = useCallback(() => {
    stopTimer()
    planRef.current = null
    simRef.current = null
    setSession(INITIAL_SESSION)
  }, [stopTimer])

  return {
    session,
    beginResolve,
    start,
    cancel,
    retry,
    reset,
  }
}
