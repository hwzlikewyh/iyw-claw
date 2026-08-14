"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  skillActivationSet,
  skillInventoryList,
  skillReconcile,
  skillTakeOver,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import type { SkillMarketActivationTarget } from "@/lib/skill-market-activation"
import { useTabStore } from "@/stores/tab-store"
import type {
  AgentType,
  LogicalSkillInventoryItem,
  SkillInventorySnapshot,
} from "@/lib/types"

export interface SkillActivationBatchResult {
  changed: number
  unchanged: number
  blocked: number
  failed: number
  errors: string[]
  issues: string[]
}

function emptyBatchResult(): SkillActivationBatchResult {
  return {
    changed: 0,
    unchanged: 0,
    blocked: 0,
    failed: 0,
    errors: [],
    issues: [],
  }
}

function targetIssue(
  target: SkillMarketActivationTarget,
  reason: string
): string {
  return `${target.skillId} (${target.agentType}): ${reason}`
}

function blockedIssue(
  target: SkillMarketActivationTarget,
  nextEnabled: boolean
): string {
  const reasons = [...target.blockedReasons]
  if (!nextEnabled && target.requiredBy.length) {
    reasons.push(`required_by:${target.requiredBy.join(",")}`)
  }
  return targetIssue(target, reasons.join(", "))
}

export function useSkillInventory(enabled: boolean) {
  const tabs = useTabStore((state) => state.tabs)
  const activeTabId = useTabStore((state) => state.activeTabId)
  const workspacePath = useMemo(
    () => tabs.find((tab) => tab.id === activeTabId)?.workingDir ?? null,
    [activeTabId, tabs]
  )
  const [snapshot, setSnapshot] = useState<SkillInventorySnapshot | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [busyKey, setBusyKey] = useState<string | null>(null)
  const requestRef = useRef(0)
  const mutationBusyRef = useRef(false)

  const refresh = useCallback(async () => {
    const requestId = ++requestRef.current
    setLoading(true)
    setError(null)
    try {
      const next = await skillInventoryList(workspacePath)
      if (requestId === requestRef.current) setSnapshot(next)
    } catch (cause) {
      if (requestId === requestRef.current) setError(toErrorMessage(cause))
    } finally {
      if (requestId === requestRef.current) setLoading(false)
    }
  }, [workspacePath])

  useEffect(() => {
    if (!enabled) return
    void refresh()
  }, [enabled, refresh])

  const setActivation = useCallback(
    async (
      skill: LogicalSkillInventoryItem,
      agentType: AgentType,
      nextEnabled: boolean
    ) => {
      if (mutationBusyRef.current) return
      mutationBusyRef.current = true
      const key = `${skill.scope}:${skill.skillId}:${agentType}`
      setBusyKey(key)
      setError(null)
      try {
        const result = await skillActivationSet({
          skillId: skill.skillId,
          scope: skill.scope,
          workspacePath,
          agentType,
          enabled: nextEnabled,
          expectedRevision: snapshot?.revision ?? null,
        })
        if (result.error) {
          setError(result.error)
        } else if (result.actualEnabled !== nextEnabled) {
          setError(
            `${skill.skillId} (${agentType}): expected ${nextEnabled ? "enabled" : "disabled"}, received ${result.actualEnabled ? "enabled" : "disabled"}`
          )
        }
        await refresh()
      } catch (cause) {
        setError(toErrorMessage(cause))
      } finally {
        setBusyKey(null)
        mutationBusyRef.current = false
      }
    },
    [refresh, snapshot?.revision, workspacePath]
  )

  const setActivations = useCallback(
    async (
      targets: SkillMarketActivationTarget[],
      nextEnabled: boolean
    ): Promise<SkillActivationBatchResult> => {
      const result = emptyBatchResult()
      if (mutationBusyRef.current) {
        const issue = "Another Skill mutation is already in progress."
        result.failed = 1
        result.errors.push(issue)
        result.issues.push(issue)
        return result
      }
      mutationBusyRef.current = true
      let revision = snapshot?.revision ?? null
      setBusyKey("bulk:activation")
      setError(null)
      try {
        for (const target of targets) {
          if (target.actualEnabled === nextEnabled) {
            result.unchanged += 1
            continue
          }
          if (
            target.blockedReasons.length ||
            (!nextEnabled && target.requiredBy.length)
          ) {
            result.blocked += 1
            result.issues.push(blockedIssue(target, nextEnabled))
            continue
          }
          try {
            const applied = await skillActivationSet({
              skillId: target.skillId,
              scope: target.scope,
              workspacePath,
              agentType: target.agentType,
              enabled: nextEnabled,
              expectedRevision: revision,
            })
            revision = applied.revision
            if (applied.error || applied.actualEnabled !== nextEnabled) {
              const reason =
                applied.error ??
                `expected ${nextEnabled ? "enabled" : "disabled"}, received ${applied.actualEnabled ? "enabled" : "disabled"}`
              const issue = targetIssue(target, reason)
              result.failed += 1
              result.errors.push(issue)
              result.issues.push(issue)
            } else {
              result.changed += 1
            }
          } catch (cause) {
            const issue = targetIssue(target, toErrorMessage(cause))
            result.failed += 1
            result.errors.push(issue)
            result.issues.push(issue)
            break
          }
        }
        if (result.errors.length) setError(result.errors[0])
        return result
      } finally {
        await refresh()
        setBusyKey(null)
        mutationBusyRef.current = false
      }
    },
    [refresh, snapshot?.revision, workspacePath]
  )

  const takeOver = useCallback(
    async (
      skill: LogicalSkillInventoryItem,
      sourcePath: string,
      agentType: AgentType
    ) => {
      if (mutationBusyRef.current) return
      mutationBusyRef.current = true
      setBusyKey(`take-over:${skill.scope}:${skill.skillId}:${agentType}`)
      setError(null)
      try {
        const result = await skillTakeOver({
          skillId: skill.skillId,
          sourcePath,
          workspacePath,
          agentType,
          expectedRevision: snapshot?.revision ?? null,
        })
        setSnapshot(result.snapshot)
        if (result.error) setError(result.error)
      } catch (cause) {
        setError(toErrorMessage(cause))
      } finally {
        setBusyKey(null)
        mutationBusyRef.current = false
      }
    },
    [snapshot?.revision, workspacePath]
  )

  const reconcile = useCallback(async () => {
    if (mutationBusyRef.current) return
    mutationBusyRef.current = true
    setBusyKey("reconcile")
    setError(null)
    try {
      const result = await skillReconcile({ workspacePath })
      setSnapshot(result.snapshot)
      if (result.error) setError(result.error)
    } catch (cause) {
      setError(toErrorMessage(cause))
    } finally {
      setBusyKey(null)
      mutationBusyRef.current = false
    }
  }, [workspacePath])

  return {
    snapshot,
    workspacePath,
    loading,
    error,
    busyKey,
    refresh,
    setActivation,
    setActivations,
    takeOver,
    reconcile,
  }
}
