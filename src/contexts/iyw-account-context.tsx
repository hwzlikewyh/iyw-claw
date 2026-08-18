"use client"

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react"

import {
  iywAccountGetProfile,
  iywAccountLoginWithPassword,
  iywAccountLogout,
} from "@/lib/api"
import { extractAppCommandError, toErrorMessage } from "@/lib/app-error"
import type { IywAccountProfile } from "@/lib/types"

const PROFILE_REFRESH_INTERVAL_MS = 60_000

export type IywAccountStatus =
  | "checking"
  | "login_required"
  | "authenticated"
  | "error"

interface IywAccountContextValue {
  status: IywAccountStatus
  profile: IywAccountProfile | null
  error: string | null
  actionLoading: boolean
  refreshProfile: () => Promise<void>
  loginWithPassword: (params: {
    username: string
    password: string
  }) => Promise<IywAccountProfile>
  completeLogin: (profile: IywAccountProfile) => void
  logout: () => Promise<void>
}

const IywAccountContext = createContext<IywAccountContextValue | null>(null)

interface ActiveProfileRequest {
  generation: number
  request: Promise<IywAccountProfile>
}

function useProfileRequest() {
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

function usePeriodicProfileRefresh({
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

export function IywAccountProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<IywAccountStatus>("checking")
  const [profile, setProfile] = useState<IywAccountProfile | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [actionLoading, setActionLoading] = useState(false)
  const requestProfile = useProfileRequest()
  const profileGenerationRef = useRef(0)
  const mountedRef = useRef(true)

  const getGeneration = useCallback(() => profileGenerationRef.current, [])
  const isGenerationCurrent = useCallback(
    (generation: number) =>
      mountedRef.current && profileGenerationRef.current === generation,
    []
  )
  const advanceGeneration = useCallback(() => {
    profileGenerationRef.current += 1
    return profileGenerationRef.current
  }, [])

  const markAuthenticationRequired = useCallback(() => {
    if (!mountedRef.current) return
    setProfile(null)
    setError(null)
    setStatus("login_required")
  }, [])

  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
    }
  }, [])

  const applyProfile = useCallback((next: IywAccountProfile) => {
    setProfile(next)
    setError(null)
    setStatus(next.logged_in ? "authenticated" : "login_required")
  }, [])

  const refreshProfile = useCallback(async () => {
    const generation = getGeneration()
    setStatus("checking")
    setError(null)
    try {
      const next = await requestProfile(generation)
      if (isGenerationCurrent(generation)) applyProfile(next)
    } catch (reason) {
      if (!isGenerationCurrent(generation)) return
      if (extractAppCommandError(reason)?.code === "authentication_failed") {
        markAuthenticationRequired()
        return
      }
      setProfile(null)
      setError(toErrorMessage(reason))
      setStatus("error")
    }
  }, [
    applyProfile,
    getGeneration,
    isGenerationCurrent,
    markAuthenticationRequired,
    requestProfile,
  ])

  useEffect(() => {
    void refreshProfile()
  }, [refreshProfile])

  usePeriodicProfileRefresh({
    status,
    requestProfile,
    applyProfile,
    onAuthenticationFailure: markAuthenticationRequired,
    getGeneration,
    isGenerationCurrent,
  })

  const completeLogin = useCallback(
    (next: IywAccountProfile) => {
      advanceGeneration()
      if (mountedRef.current) applyProfile(next)
    },
    [advanceGeneration, applyProfile]
  )

  const loginWithPassword = useCallback(
    async (params: { username: string; password: string }) => {
      setActionLoading(true)
      try {
        const next = await iywAccountLoginWithPassword(params)
        completeLogin(next)
        return next
      } finally {
        if (mountedRef.current) setActionLoading(false)
      }
    },
    [completeLogin]
  )

  const logout = useCallback(async () => {
    setActionLoading(true)
    const generation = getGeneration()
    try {
      await iywAccountLogout()
      if (!isGenerationCurrent(generation)) return
      advanceGeneration()
      setProfile(null)
      setError(null)
      setStatus("login_required")
    } finally {
      if (mountedRef.current) setActionLoading(false)
    }
  }, [advanceGeneration, getGeneration, isGenerationCurrent])

  const value = useMemo<IywAccountContextValue>(
    () => ({
      status,
      profile,
      error,
      actionLoading,
      refreshProfile,
      loginWithPassword,
      completeLogin,
      logout,
    }),
    [
      actionLoading,
      completeLogin,
      error,
      loginWithPassword,
      logout,
      profile,
      refreshProfile,
      status,
    ]
  )

  return (
    <IywAccountContext.Provider value={value}>
      {children}
    </IywAccountContext.Provider>
  )
}

export function useIywAccount() {
  const value = useContext(IywAccountContext)
  if (!value) {
    throw new Error("useIywAccount must be used within IywAccountProvider")
  }
  return value
}
