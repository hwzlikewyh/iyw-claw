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

import { iywAccountLoginWithPassword, iywAccountLogout } from "@/lib/api"
import { resetActiveConversationUsage } from "@/lib/conversation-usage-refresh"
import { extractAppCommandError, toErrorMessage } from "@/lib/app-error"
import type { IywAccountProfile } from "@/lib/types"
import {
  iywAccountAuth,
  type IywAuthAction,
  type IywLoginResult,
} from "@/lib/iyw-account-flow"
import {
  useProfileRequest,
  usePeriodicProfileRefresh,
} from "./use-account-profile-refresh"

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
  loginFlow: IywLoginResult | null
  authenticate: (
    action: IywAuthAction,
    data?: Record<string, unknown>
  ) => Promise<IywLoginResult>
  cancelLogin: () => Promise<void>
  invalidateLogin: () => void
}

const IywAccountContext = createContext<IywAccountContextValue | null>(null)

export function IywAccountProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<IywAccountStatus>("checking")
  const [profile, setProfile] = useState<IywAccountProfile | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [actionLoading, setActionLoading] = useState(false)
  const [loginFlow, setLoginFlow] = useState<IywLoginResult | null>(null)
  const actionPendingRef = useRef(false)
  const requestProfile = useProfileRequest()
  const profileGenerationRef = useRef(0)
  const lastSuccessfulProfileRef = useRef<IywAccountProfile | null>(null)
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
    lastSuccessfulProfileRef.current = null
    resetActiveConversationUsage()
    setProfile(null)
    setLoginFlow(null)
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
    const previous = lastSuccessfulProfileRef.current
    if (
      previous?.user_id !== next.user_id ||
      previous?.org_code !== next.org_code ||
      previous?.logged_in !== next.logged_in
    )
      resetActiveConversationUsage()
    if (next.workspace_pending && previous?.logged_in) {
      next = {
        ...next,
        user_id: next.user_id ?? previous.user_id,
        name: next.name ?? previous.name,
        nick_name: next.nick_name ?? previous.nick_name,
        phone: next.phone ?? previous.phone,
        avatar_url: next.avatar_url ?? previous.avatar_url,
        org_list: next.org_list?.length ? next.org_list : previous.org_list,
      }
    }
    if (next.logged_in) lastSuccessfulProfileRef.current = next
    setProfile(next)
    setError(null)
    setStatus(next.logged_in ? "authenticated" : "login_required")
  }, [])

  const refreshProfile = useCallback(async () => {
    if (loginFlow || actionPendingRef.current) return
    const generation = getGeneration()
    setStatus((current) => (current === "authenticated" ? current : "checking"))
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

      if (lastSuccessfulProfileRef.current?.logged_in) {
        console.warn(
          "[iyw-account] Profile refresh failed; retaining last successful profile",
          toErrorMessage(reason)
        )
        setError(toErrorMessage(reason))
        setStatus("authenticated")
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
    loginFlow,
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
      lastSuccessfulProfileRef.current = null
      if (mountedRef.current) {
        setLoginFlow(null)
        applyProfile(next)
      }
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

  const authenticate = useCallback(
    async (action: IywAuthAction, data: Record<string, unknown> = {}) => {
      if (actionPendingRef.current)
        throw new Error("An account operation is already running")
      actionPendingRef.current = true
      const generation = advanceGeneration()
      if (action !== "wechat_poll") setActionLoading(true)
      try {
        const result = await iywAccountAuth(action, data)
        if (!isGenerationCurrent(generation)) return result
        if (result.profile?.logged_in) {
          // 作废操作期间启动的轮询，避免旧资料覆盖新的空间和点数。
          advanceGeneration()
          setLoginFlow(null)
          applyProfile(result.profile)
        } else if (result.flow_id) {
          setLoginFlow(result)
          setStatus("login_required")
        }
        return result
      } catch (reason) {
        if (
          isGenerationCurrent(generation) &&
          extractAppCommandError(reason)?.code === "authentication_failed"
        ) {
          markAuthenticationRequired()
        }
        throw reason
      } finally {
        actionPendingRef.current = false
        if (mountedRef.current) setActionLoading(false)
      }
    },
    [
      advanceGeneration,
      applyProfile,
      isGenerationCurrent,
      markAuthenticationRequired,
    ]
  )

  const cancelLogin = useCallback(async () => {
    advanceGeneration()
    const flowId = loginFlow?.flow_id
    setLoginFlow(null)
    if (flowId) await iywAccountAuth("cancel", { flowId })
  }, [advanceGeneration, loginFlow])

  const logout = useCallback(async () => {
    setActionLoading(true)
    const generation = advanceGeneration()
    try {
      await iywAccountLogout()
      if (!isGenerationCurrent(generation)) return
      advanceGeneration()
      lastSuccessfulProfileRef.current = null
      setProfile(null)
      setLoginFlow(null)
      setError(null)
      setStatus("login_required")
    } finally {
      if (mountedRef.current) setActionLoading(false)
    }
  }, [advanceGeneration, isGenerationCurrent])

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
      loginFlow,
      authenticate,
      cancelLogin,
      invalidateLogin: advanceGeneration,
    }),
    [
      actionLoading,
      advanceGeneration,
      loginFlow,
      authenticate,
      cancelLogin,
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

export const useOptionalIywAccount = () => useContext(IywAccountContext)
