"use client"

import { useCallback, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import {
  installAgentVersion,
  rollbackAgentVersion,
  setAgentVersionPin,
  switchAgentVersion,
} from "@/lib/api"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import { toErrorMessage } from "@/lib/app-error"
import type { AgentType, AgentVersionInventory } from "@/lib/types"

type Operation = "install" | "switch" | "pin" | "rollback" | "refresh"

interface VersionOperationArgs {
  agentType: AgentType
  selectedVersion: string
  inventory?: AgentVersionInventory
  isPinned: boolean
  load: (refresh: boolean) => Promise<void>
  setError: (error: string | null) => void
  onChanged: () => Promise<void>
}

export function useVersionOperations(args: VersionOperationArgs) {
  const { agentType, selectedVersion, load, setError, onChanged } = args
  const t = useTranslations("AcpAgentSettings.versionCenter")
  const [busy, setBusy] = useState<Operation | null>(null)
  const [pendingActivationVersion, setPendingActivationVersion] = useState<
    string | null
  >(null)
  const run = useCallback(
    async (operation: Operation, action: () => Promise<unknown>) => {
      setBusy(operation)
      setError(null)
      try {
        const result = await action()
        const activationState = operationResult(result)?.activationState ?? null
        await Promise.all([load(false), onChanged()])
        setPendingActivationVersion(
          activationState === "pending" ? selectedVersion : null
        )
        toast.success(
          t(
            activationState === "pending"
              ? "success.pendingActivation"
              : `success.${operation}`,
            {
              name: getAgentDisplayName(agentType),
              version: selectedVersion,
            }
          )
        )
      } catch (reason) {
        const message = toErrorMessage(reason)
        setError(message)
        toast.error(t("operationFailed"), { description: message })
      } finally {
        setBusy(null)
      }
    },
    [agentType, load, onChanged, selectedVersion, setError, t]
  )
  return {
    ...buildOperations(args, busy, setBusy, run, t),
    pendingActivationVersion,
  }
}

function operationResult(
  value: unknown
): { activationState?: "active" | "pending" } | null {
  return value && typeof value === "object" ? value : null
}

function buildOperations(
  args: VersionOperationArgs,
  busy: Operation | null,
  setBusy: (value: Operation | null) => void,
  run: (operation: Operation, action: () => Promise<unknown>) => Promise<void>,
  t: ReturnType<typeof useTranslations>
) {
  const { agentType, selectedVersion, inventory, isPinned, load } = args
  const refresh = () => {
    setBusy("refresh")
    void Promise.all([load(true), args.onChanged()])
      .then(() => toast.success(t("success.refresh")))
      .catch(() => {})
      .finally(() => setBusy(null))
  }
  return {
    busy,
    refresh,
    install: () =>
      void run("install", () =>
        installAgentVersion(agentType, selectedVersion)
      ),
    switchVersion: () =>
      void run("switch", () => switchAgentVersion(agentType, selectedVersion)),
    togglePin: () =>
      void run("pin", () =>
        setAgentVersionPin(
          agentType,
          isPinned ? null : selectedVersion,
          inventory?.updateChannel
        )
      ),
    rollback: () => void run("rollback", () => rollbackAgentVersion(agentType)),
  }
}
