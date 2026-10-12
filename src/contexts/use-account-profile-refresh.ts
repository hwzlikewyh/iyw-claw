"use client"

import { useCallback, useEffect, useRef } from "react"
import { iywAccountGetProfile } from "@/lib/api"
import { extractAppCommandError, toErrorMessage } from "@/lib/app-error"
import type { IywAccountProfile } from "@/lib/types"
import type { IywAccountStatus } from "./iyw-account-context"

const PROFILE_REFRESH_INTERVAL_MS = 60_000

interface ActiveProfileRequest {
  generation: number
  request: Promise<IywAccountProfile>
}

export function useProfileRequest() {
  const activeRequestRef = useRef<ActiveProfileRequest | null>(null)

  return useCallback((generation: number) => {
    const activeRequest = activeRequestRef.current
    if (activeRequest?.generation === generation) return activeRequest.request

    const request = iywAccountGetProfile()
    const clearRequest = () => {
      if (activeRequestRef.current?.request === request) {
        activeRequestRef.current = null
      }
    }
    activeRequestRef.current = { generation, request }
    void request.then(clearRequest, clearRequest)
    return request
  }, [])
}

interface PeriodicProfileRefreshOptions {
  status: IywAccountStatus
  requestProfile: (generation: number) => Promise<IywAccountProfile>
  applyProfile: (profile: IywAccountProfile) => void
  onAuthenticationFailure: () => void
  getGeneration: () => number
  isGenerationCurrent: (generation: number) => boolean
}

export function usePeriodicProfileRefresh({
  status,
  requestProfile,
  applyProfile,
  onAuthenticationFailure,
  getGeneration,
  isGenerationCurrent,
}: PeriodicProfileRefreshOptions) {
  useEffect(() => {
    if (status !== "authenticated") return

    let active = true
    let refreshPending = false
    const refresh = async () => {
      if (refreshPending) return
      refreshPending = true
      const generation = getGeneration()
      try {
        const next = await requestProfile(generation)
        if (active && isGenerationCurrent(generation)) applyProfile(next)
      } catch (reason) {
        if (active && isGenerationCurrent(generation)) {
          if (
            extractAppCommandError(reason)?.code === "authentication_failed"
          ) {
            onAuthenticationFailure()
          } else {
            console.warn(
              "[iyw-account] Periodic profile refresh failed",
              toErrorMessage(reason)
            )
          }
        }
      } finally {
        refreshPending = false
      }
    }
    const interval = window.setInterval(
      () => void refresh(),
      PROFILE_REFRESH_INTERVAL_MS
    )

    return () => {
      active = false
      window.clearInterval(interval)
    }
  }, [
    applyProfile,
    getGeneration,
    isGenerationCurrent,
    onAuthenticationFailure,
    requestProfile,
    status,
  ])
}
