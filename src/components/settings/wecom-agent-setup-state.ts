"use client"

import { useMemo, useRef, useState } from "react"

import {
  generateWecomAgentSecrets,
  parseChannelConfig,
  type ChannelStoredConfig,
} from "@/lib/chat-channel-setup"
import type { ChatChannelInfo } from "@/lib/types"
import type { WecomAgentStep } from "./wecom-agent-setup-parts"

export function useWecomAgentSetupState(draft?: ChatChannelInfo) {
  const stored = useMemo<ChannelStoredConfig>(
    () => (draft ? parseChannelConfig(draft) : {}),
    [draft]
  )
  const resumedCallback = stored.setup_state === "pending_callback"
  const [step, setStep] = useState<WecomAgentStep>(() =>
    stored.callback_verified_at ? 4 : resumedCallback ? 3 : 1
  )
  const [working, setWorking] = useState(draft)
  const [webRunning, setWebRunning] = useState<boolean | null>(null)
  const [conditionConfirmed, setConditionConfirmed] = useState(false)
  const [externalUrl, setExternalUrl] = useState(stored.external_base_url ?? "")
  const [corpId, setCorpId] = useState(stored.corp_id ?? "")
  const [agentId, setAgentId] = useState(stored.agent_id ?? "")
  const [appSecret, setAppSecret] = useState("")
  const [secrets, setSecrets] = useState(() => initialSecrets(stored))
  const [secretsAvailable, setSecretsAvailable] = useState(!resumedCallback)
  const [regenerateOpen, setRegenerateOpen] = useState(false)
  const [callbackVerified, setCallbackVerified] = useState(
    Boolean(stored.callback_verified_at)
  )
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const regenerationInFlight = useRef(false)
  return {
    stored,
    step,
    setStep,
    working,
    setWorking,
    webRunning,
    setWebRunning,
    conditionConfirmed,
    setConditionConfirmed,
    externalUrl,
    setExternalUrl,
    corpId,
    setCorpId,
    agentId,
    setAgentId,
    appSecret,
    setAppSecret,
    secrets,
    setSecrets,
    secretsAvailable,
    setSecretsAvailable,
    regenerateOpen,
    setRegenerateOpen,
    callbackVerified,
    setCallbackVerified,
    loading,
    setLoading,
    error,
    setError,
    regenerationInFlight,
  }
}

function initialSecrets(stored: ChannelStoredConfig) {
  const generated = generateWecomAgentSecrets()
  return stored.callback_path
    ? { ...generated, callbackPath: stored.callback_path }
    : generated
}

export type WecomAgentSetupState = ReturnType<typeof useWecomAgentSetupState>
