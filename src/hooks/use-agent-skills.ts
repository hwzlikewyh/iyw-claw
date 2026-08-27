"use client"

import { useCallback, useEffect, useMemo, useState } from "react"

import { acpListAgentSkills } from "@/lib/api"
import type { AgentSkillItem, AgentType } from "@/lib/types"

// Cache/inflight keyed by `${agentType}|${workspacePath ?? ""}` so different
// folders keep their own skill list, and switching folders never serves stale
// entries from a previous workspace.
const cache = new Map<string, AgentSkillItem[]>()
const inflight = new Map<string, Promise<AgentSkillItem[]>>()
const forcedInflight = new Map<string, Promise<AgentSkillItem[]>>()
const generations = new Map<string, number>()
const subscribers = new Map<
  string,
  Set<(skills: AgentSkillItem[] | null) => void>
>()

const EMPTY: AgentSkillItem[] = []

function makeKey(agentType: AgentType, workspacePath: string | null): string {
  return `${agentType}|${workspacePath ?? ""}`
}

function fetchSkills(
  agentType: AgentType,
  workspacePath: string | null,
  force = false
): Promise<AgentSkillItem[]> {
  const key = makeKey(agentType, workspacePath)
  if (force) {
    const existing = forcedInflight.get(key)
    if (existing) return existing
  }
  let promise = inflight.get(key)
  if (!promise || force) {
    const generation = (generations.get(key) ?? 0) + (force ? 1 : 0)
    generations.set(key, generation)
    promise = acpListAgentSkills({ agentType, workspacePath })
      .then((result) => {
        const skills = result.supported
          ? result.skills.filter((skill) => skill.enabled)
          : EMPTY
        if (generations.get(key) !== generation) {
          return cache.get(key) ?? EMPTY
        }
        cache.set(key, skills)
        notifySubscribers(key, skills)
        return skills
      })
      .catch((err) => {
        console.warn("[useAgentSkills] failed:", err)
        if (force) throw err
        return cache.get(key) ?? EMPTY
      })
      .finally(() => {
        if (inflight.get(key) === promise) inflight.delete(key)
        if (forcedInflight.get(key) === promise) forcedInflight.delete(key)
      })
    inflight.set(key, promise)
    if (force) forcedInflight.set(key, promise)
  }
  return promise
}

function notifySubscribers(key: string, skills: AgentSkillItem[] | null) {
  for (const subscriber of subscribers.get(key) ?? []) subscriber(skills)
}

export function refreshAgentSkills(
  agentType: AgentType,
  workspacePath?: string | null
): Promise<AgentSkillItem[]> {
  return fetchSkills(agentType, workspacePath ?? null, true)
}

export function useAgentSkills(
  agentType: AgentType | null,
  workspacePath?: string | null
): AgentSkillItem[] {
  const normalizedPath = workspacePath ?? null
  const cacheKey = useMemo(
    () => (agentType ? makeKey(agentType, normalizedPath) : null),
    [agentType, normalizedPath]
  )
  const cached = cacheKey ? (cache.get(cacheKey) ?? null) : null
  // Track which (agentType, workspacePath) the fetched result belongs to so
  // stale data from a previous key is never returned after a switch.
  const [fetched, setFetched] = useState<{
    key: string
    skills: AgentSkillItem[]
  } | null>(null)

  const doFetch = useCallback(() => {
    if (!agentType || !cacheKey || cache.has(cacheKey)) return
    let cancelled = false
    fetchSkills(agentType, normalizedPath).then((list) => {
      if (!cancelled) setFetched({ key: cacheKey, skills: list })
    })
    return () => {
      cancelled = true
    }
  }, [agentType, cacheKey, normalizedPath])

  // Initial fetch
  useEffect(() => doFetch(), [doFetch])

  useEffect(() => {
    if (!cacheKey) return
    const subscriber = (skills: AgentSkillItem[] | null) => {
      setFetched(skills ? { key: cacheKey, skills } : null)
    }
    const current = subscribers.get(cacheKey) ?? new Set()
    current.add(subscriber)
    subscribers.set(cacheKey, current)
    return () => {
      current.delete(subscriber)
      if (!current.size) subscribers.delete(cacheKey)
    }
  }, [cacheKey])

  // Re-fetch when window regains focus (covers cross-window cache
  // invalidation — e.g. settings window creates/removes skills while the
  // conversation window stays mounted). Only invalidate the current key to
  // avoid clobbering caches for other folders.
  useEffect(() => {
    const onFocus = () => {
      if (!cacheKey || !agentType) return
      void refreshAgentSkills(agentType, normalizedPath).catch((error) => {
        console.warn("[useAgentSkills] focus refresh failed:", error)
      })
    }
    window.addEventListener("focus", onFocus)
    return () => window.removeEventListener("focus", onFocus)
  }, [agentType, cacheKey, normalizedPath])

  if (!agentType || !cacheKey) return EMPTY
  if (fetched && fetched.key === cacheKey) return fetched.skills
  if (cached) return cached
  return EMPTY
}

export function invalidateAgentSkillsCache(agentType?: AgentType) {
  const prefix = agentType ? `${agentType}|` : null
  const keys = new Set([
    ...cache.keys(),
    ...inflight.keys(),
    ...forcedInflight.keys(),
    ...generations.keys(),
  ])
  for (const key of keys) {
    if (prefix && !key.startsWith(prefix)) continue
    generations.set(key, (generations.get(key) ?? 0) + 1)
    cache.delete(key)
    inflight.delete(key)
    forcedInflight.delete(key)
    notifySubscribers(key, null)
  }
}
