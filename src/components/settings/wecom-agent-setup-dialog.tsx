"use client"

import { useTranslations } from "next-intl"

import type { ChatChannelInfo } from "@/lib/types"
import {
  confirmWecomRegeneration,
  finalizeWecomSetup,
  saveWecomParameters,
  type SetupCallbacks,
  type SetupMessages,
} from "./wecom-agent-setup-actions"
import {
  useCallbackVerificationPoll,
  useWebServerProbe,
} from "./wecom-agent-setup-effects"
import { normalizeHttpsUrl } from "./wecom-agent-setup-parts"
import {
  useWecomAgentSetupState,
  type WecomAgentSetupState,
} from "./wecom-agent-setup-state"
import {
  WecomAgentSetupView,
  type WecomAgentSetupActions,
} from "./wecom-agent-setup-view"

interface WecomAgentSetupDialogProps {
  open: boolean
  draft?: ChatChannelInfo
  onOpenChange: (open: boolean) => void
  onComplete: () => void
}

export function WecomAgentSetupDialog({
  open,
  draft,
  onOpenChange,
  onComplete,
}: WecomAgentSetupDialogProps) {
  const t = useTranslations("ChatChannelSettings")
  const state = useWecomAgentSetupState(draft)
  useWebServerProbe(open, state.setWebRunning)
  useCallbackVerificationPoll({
    open,
    step: state.step,
    working: state.working,
    verified: state.callbackVerified,
    setVerified: state.setCallbackVerified,
    setError: state.setError,
  })
  const normalizedUrl = normalizeHttpsUrl(state.externalUrl)
  const messages: SetupMessages = {
    parametersRequired: t("market.wecomAgent.parametersRequired"),
    draftName: t("market.draftNames.wecom_agent"),
    reportUserRequired: t("market.wecomAgent.reportUserRequired"),
    savedButConnectFailed: t("savedButConnectFailed"),
  }
  const actions = createActions(state, messages, { onOpenChange, onComplete })
  return (
    <WecomAgentSetupView
      open={open}
      state={state}
      normalizedUrl={normalizedUrl}
      callbackUrl={buildCallbackUrl(state, normalizedUrl)}
      actions={actions}
      onOpenChange={onOpenChange}
    />
  )
}

function createActions(
  state: WecomAgentSetupState,
  messages: SetupMessages,
  callbacks: SetupCallbacks
): WecomAgentSetupActions {
  return {
    saveParameters: () => void saveWecomParameters(state, messages),
    confirmRegenerate: () => confirmWecomRegeneration(state),
    finalize: (values) =>
      void finalizeWecomSetup(state, { values, messages, callbacks }),
  }
}

function buildCallbackUrl(
  state: WecomAgentSetupState,
  normalizedUrl: string | null
) {
  if (!state.working || !normalizedUrl) return ""
  return `${normalizedUrl}/api/wecom_agent_callback/${state.working.id}/${state.secrets.callbackPath}`
}
