"use client"

import { useCallback, useEffect, useState } from "react"

export interface ScenarioPreference {
  hidden: boolean
  promptOverride: string
  sortOrder: number | null
}

export type ScenarioPreferences = Record<string, ScenarioPreference>

const STORAGE_KEY = "iyw-claw.scenario-preferences.v1"

function readPreferences(): ScenarioPreferences {
  if (typeof window === "undefined") return {}
  try {
    const value = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "{}")
    if (!value || typeof value !== "object") return {}
    return Object.fromEntries(
      Object.entries(value).flatMap(([id, raw]) => {
        if (!raw || typeof raw !== "object") return []
        const item = raw as Partial<ScenarioPreference>
        const sortOrder = Number(item.sortOrder)
        return [
          [
            id,
            {
              hidden: item.hidden === true,
              promptOverride:
                typeof item.promptOverride === "string"
                  ? item.promptOverride
                  : "",
              sortOrder:
                item.sortOrder === null || !Number.isFinite(sortOrder)
                  ? null
                  : Math.max(0, Math.trunc(sortOrder)),
            },
          ],
        ]
      })
    )
  } catch {
    return {}
  }
}

function writePreferences(value: ScenarioPreferences) {
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(value))
}

export function useScenarioPreferences() {
  const [preferences, setPreferences] =
    useState<ScenarioPreferences>(readPreferences)

  useEffect(() => {
    const onStorage = (event: StorageEvent) => {
      if (event.key === STORAGE_KEY) setPreferences(readPreferences())
    }
    window.addEventListener("storage", onStorage)
    return () => window.removeEventListener("storage", onStorage)
  }, [])

  const updatePreference = useCallback(
    (scenarioId: string, patch: Partial<ScenarioPreference>) => {
      setPreferences((current) => {
        const previous = current[scenarioId] ?? {
          hidden: false,
          promptOverride: "",
          sortOrder: null,
        }
        const next = { ...current, [scenarioId]: { ...previous, ...patch } }
        writePreferences(next)
        return next
      })
    },
    []
  )

  const resetPreference = useCallback((scenarioId: string) => {
    setPreferences((current) => {
      const next = { ...current }
      delete next[scenarioId]
      writePreferences(next)
      return next
    })
  }, [])

  return { preferences, updatePreference, resetPreference }
}
