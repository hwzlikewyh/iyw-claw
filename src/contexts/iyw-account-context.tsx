"use client"

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react"

import {
  iywAccountGetProfile,
  iywAccountLoginWithPassword,
  iywAccountLogout,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import type { IywAccountProfile } from "@/lib/types"

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

export function IywAccountProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<IywAccountStatus>("checking")
  const [profile, setProfile] = useState<IywAccountProfile | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [actionLoading, setActionLoading] = useState(false)

  const applyProfile = useCallback((next: IywAccountProfile) => {
    setProfile(next)
    setError(null)
    setStatus(next.logged_in ? "authenticated" : "login_required")
  }, [])

  const refreshProfile = useCallback(async () => {
    setStatus("checking")
    setError(null)
    try {
      applyProfile(await iywAccountGetProfile())
    } catch (reason) {
      setProfile(null)
      setError(toErrorMessage(reason))
      setStatus("error")
    }
  }, [applyProfile])

  useEffect(() => {
    void refreshProfile()
  }, [refreshProfile])

  const loginWithPassword = useCallback(
    async (params: { username: string; password: string }) => {
      setActionLoading(true)
      try {
        const next = await iywAccountLoginWithPassword(params)
        applyProfile(next)
        return next
      } finally {
        setActionLoading(false)
      }
    },
    [applyProfile]
  )

  const logout = useCallback(async () => {
    setActionLoading(true)
    try {
      await iywAccountLogout()
      setProfile(null)
      setError(null)
      setStatus("login_required")
    } finally {
      setActionLoading(false)
    }
  }, [])

  const value = useMemo<IywAccountContextValue>(
    () => ({
      status,
      profile,
      error,
      actionLoading,
      refreshProfile,
      loginWithPassword,
      completeLogin: applyProfile,
      logout,
    }),
    [
      actionLoading,
      applyProfile,
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
