"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import {
  internetToolInstall,
  internetToolUninstall,
  internetToolsDetect,
  internetToolsOpencliDoctor,
  managedSkillsReconcileFamily,
} from "@/lib/api"
import { prepareBrowserExtension } from "@/lib/browser-setup-api"
import { toErrorMessage } from "@/lib/app-error"
import type { InternetToolInfo, OpencliDoctorResult } from "@/lib/types"
import { invalidateAgentSkillsCache } from "@/hooks/use-agent-skills"

type Action =
  | "loading"
  | "installing"
  | "preparing"
  | "checking"
  | "uninstalling"

function useSetupState() {
  const [tool, setTool] = useState<InternetToolInfo | null>(null)
  const [doctor, setDoctor] = useState<OpencliDoctorResult | null>(null)
  const [directory, setDirectory] = useState("")
  const [error, setError] = useState("")
  const [action, setAction] = useState<Action | null>("loading")
  const active = useRef(true)
  useEffect(() => {
    active.current = true
    return () => {
      active.current = false
    }
  }, [])
  return {
    tool,
    setTool,
    doctor,
    setDoctor,
    directory,
    setDirectory,
    error,
    setError,
    action,
    setAction,
    active,
  }
}

type SetupState = ReturnType<typeof useSetupState>

function useSetupAction({ setAction, setError, active }: SetupState) {
  const locked = useRef(false)
  return useCallback(
    async <T>(next: Action, operation: () => Promise<T>) => {
      if (locked.current) return undefined
      locked.current = true
      if (active.current) {
        setAction(next)
        setError("")
      }
      try {
        return await operation()
      } catch (cause) {
        if (active.current) setError(toErrorMessage(cause))
        return undefined
      } finally {
        locked.current = false
        if (active.current) setAction(null)
      }
    },
    [active, setAction, setError]
  )
}

function useRuntimeActions(state: SetupState) {
  const { active, setTool, setDoctor, setDirectory, setAction } = state
  const refresh = useCallback(async () => {
    const info =
      (await internetToolsDetect()).find((item) => item.id === "opencli") ??
      null
    if (active.current) setTool(info)
    return info
  }, [active, setTool])
  const install = useCallback(async () => {
    if (active.current) {
      setAction("installing")
      setDoctor(null)
    }
    const info = await internetToolInstall("opencli")
    if (active.current) setTool(info)
    if (info.status !== "installed")
      throw new Error(info.runtimeError ?? "OpenCLI installation is not ready")
    const report = await managedSkillsReconcileFamily("internet_tools")
    report.touchedAgents.forEach(invalidateAgentSkillsCache)
    return info
  }, [active, setAction, setDoctor, setTool])
  const remove = useCallback(async () => {
    const info = await internetToolUninstall("opencli")
    invalidateAgentSkillsCache()
    if (active.current) {
      setTool(info)
      setDoctor(null)
      setDirectory("")
    }
  }, [active, setTool, setDoctor, setDirectory])
  return { refresh, install, remove }
}

function useDoctorCheck(
  state: SetupState,
  run: ReturnType<typeof useSetupAction>
) {
  const pendingDoctor = useRef<Promise<OpencliDoctorResult | undefined> | null>(
    null
  )
  const { active, setDoctor } = state
  return useCallback(() => {
    if (pendingDoctor.current) return pendingDoctor.current
    const pending = run("checking", async () => {
      if (active.current) setDoctor(null)
      const result = await internetToolsOpencliDoctor()
      if (active.current) setDoctor(result)
      return result
    })
    pendingDoctor.current = pending
    void pending.finally(() => {
      pendingDoctor.current = null
    })
    return pending
  }, [active, run, setDoctor])
}

export function useOpencliSetup() {
  const state = useSetupState()
  const run = useSetupAction(state)
  const { refresh, install, remove } = useRuntimeActions(state)
  const { active, setDirectory, tool, doctor, directory, error, action } = state
  const check = useDoctorCheck(state, run)
  useEffect(() => {
    void run("loading", refresh)
  }, [refresh, run])
  const prepare = useCallback(
    () =>
      run("preparing", async () => {
        if (active.current) setDirectory("")
        const path = await prepareBrowserExtension()
        if (active.current) setDirectory(path)
        const info = await refresh()
        if (info?.status !== "installed") await install()
        return path
      }),
    [active, install, refresh, run, setDirectory]
  )
  const repair = useCallback(() => run("installing", install), [install, run])
  const uninstall = useCallback(
    () => run("uninstalling", remove),
    [remove, run]
  )
  return {
    tool,
    doctor,
    directory,
    error,
    action,
    check,
    prepare,
    repair,
    uninstall,
  }
}

export type OpencliSetup = ReturnType<typeof useOpencliSetup>

const CHECK_INTERVAL_MS = 10_000
const CHECK_WINDOW_MS = 180_000

export function useOpencliPolling(
  enabled: boolean,
  check: OpencliSetup["check"]
) {
  const [checking, setChecking] = useState(false)
  useEffect(() => {
    if (!enabled) return
    let stopped = false
    let timer: ReturnType<typeof setTimeout>
    const deadline = Date.now() + CHECK_WINDOW_MS
    const poll = async () => {
      setChecking(true)
      const result = await check()
      if (stopped) return
      if (
        result?.ok ||
        !result ||
        Date.now() >= deadline ||
        result.status === "profile_selection_required"
      ) {
        setChecking(false)
      } else {
        timer = setTimeout(() => void poll(), CHECK_INTERVAL_MS)
      }
    }
    timer = setTimeout(() => void poll(), CHECK_INTERVAL_MS)
    return () => {
      stopped = true
      clearTimeout(timer)
    }
  }, [enabled, check])
  return enabled && checking
}
