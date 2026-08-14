"use client"

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  skillActivationSet,
  skillInventoryList,
  skillReconcile,
  skillTakeOver,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import { useTabStore } from "@/stores/tab-store"
import type {
  AgentType,
  LogicalSkillInventoryItem,
  SkillInventorySnapshot,
} from "@/lib/types"

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
        if (result.error) setError(result.error)
        await refresh()
      } catch (cause) {
        setError(toErrorMessage(cause))
      } finally {
        setBusyKey(null)
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
      }
    },
    [snapshot?.revision, workspacePath]
  )

  const reconcile = useCallback(async () => {
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
    takeOver,
    reconcile,
  }
}
