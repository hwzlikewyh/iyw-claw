"use client"

import { useTranslations } from "next-intl"

import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import type { ChannelFinalizeValues } from "./channel-finalize-form"
import { ChannelFinalizeForm } from "./channel-finalize-form"
import { WecomAgentRegenerateDialog } from "./wecom-agent-regenerate-dialog"
import { WecomAgentStepper } from "./wecom-agent-setup-parts"
import {
  WecomAgentCallbackStep,
  WecomAgentCredentialsStep,
  WecomAgentRequirementsStep,
} from "./wecom-agent-setup-steps"
import type { WecomAgentSetupState } from "./wecom-agent-setup-state"

export interface WecomAgentSetupActions {
  saveParameters: () => void
  confirmRegenerate: () => void
  finalize: (values: ChannelFinalizeValues) => void
}

export function WecomAgentSetupView({
  open,
  state,
  normalizedUrl,
  callbackUrl,
  actions,
  onOpenChange,
}: {
  open: boolean
  state: WecomAgentSetupState
  normalizedUrl: string | null
  callbackUrl: string
  actions: WecomAgentSetupActions
  onOpenChange: (open: boolean) => void
}) {
  const t = useTranslations("ChatChannelSettings")
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="min-w-0 max-h-[90vh] overflow-x-hidden overflow-y-auto sm:max-w-4xl">
        <DialogHeader className="min-w-0">
          <DialogTitle>{t("market.wecomAgent.title")}</DialogTitle>
        </DialogHeader>
        <WecomAgentStepper step={state.step} />
        <SetupStep
          state={state}
          normalizedUrl={normalizedUrl}
          callbackUrl={callbackUrl}
          actions={actions}
          onCancel={() => onOpenChange(false)}
        />
        <WecomAgentRegenerateDialog
          open={state.regenerateOpen}
          loading={state.loading}
          onOpenChange={state.setRegenerateOpen}
          onConfirm={actions.confirmRegenerate}
        />
      </DialogContent>
    </Dialog>
  )
}

function SetupStep({
  state,
  normalizedUrl,
  callbackUrl,
  actions,
  onCancel,
}: {
  state: WecomAgentSetupState
  normalizedUrl: string | null
  callbackUrl: string
  actions: WecomAgentSetupActions
  onCancel: () => void
}) {
  if (state.step === 1) {
    return (
      <RequirementsContent
        state={state}
        normalizedUrl={normalizedUrl}
        onCancel={onCancel}
      />
    )
  }
  if (state.step === 2) {
    return (
      <CredentialsContent state={state} actions={actions} onCancel={onCancel} />
    )
  }
  if (state.step === 3) {
    return (
      <CallbackContent
        state={state}
        callbackUrl={callbackUrl}
        onCancel={onCancel}
      />
    )
  }
  return <FinalizeContent state={state} actions={actions} onCancel={onCancel} />
}

function RequirementsContent({
  state,
  normalizedUrl,
  onCancel,
}: {
  state: WecomAgentSetupState
  normalizedUrl: string | null
  onCancel: () => void
}) {
  return (
    <WecomAgentRequirementsStep
      webRunning={state.webRunning}
      externalUrl={state.externalUrl}
      conditionConfirmed={state.conditionConfirmed}
      loading={state.loading}
      nextDisabled={!normalizedUrl || !state.conditionConfirmed}
      onExternalUrlChange={state.setExternalUrl}
      onConditionConfirmedChange={state.setConditionConfirmed}
      onCancel={onCancel}
      onNext={() => state.setStep(2)}
    />
  )
}

function CredentialsContent({ state, actions, onCancel }: StepContentProps) {
  return (
    <WecomAgentCredentialsStep
      corpId={state.corpId}
      agentId={state.agentId}
      appSecret={state.appSecret}
      loading={state.loading}
      error={state.error}
      onCorpIdChange={state.setCorpId}
      onAgentIdChange={state.setAgentId}
      onAppSecretChange={state.setAppSecret}
      onCancel={onCancel}
      onNext={actions.saveParameters}
    />
  )
}

function CallbackContent({
  state,
  callbackUrl,
  onCancel,
}: {
  state: WecomAgentSetupState
  callbackUrl: string
  onCancel: () => void
}) {
  return (
    <WecomAgentCallbackStep
      callbackUrl={callbackUrl}
      callbackToken={state.secrets.callbackToken}
      encodingAesKey={state.secrets.encodingAesKey}
      secretsAvailable={state.secretsAvailable}
      callbackVerified={state.callbackVerified}
      loading={state.loading}
      error={state.error}
      onRegenerate={() => state.setRegenerateOpen(true)}
      onCancel={onCancel}
      onNext={() => state.setStep(4)}
    />
  )
}

function FinalizeContent({ state, actions, onCancel }: StepContentProps) {
  const t = useTranslations("ChatChannelSettings")
  return (
    <ChannelFinalizeForm
      channelType="wecom_agent"
      initialName={state.working?.name ?? t("market.draftNames.wecom_agent")}
      initialDefaultAgentType={state.stored.default_agent_type ?? null}
      initialDefaultUserId={state.stored.default_user_id ?? ""}
      initialDailyReportEnabled={state.working?.daily_report_enabled ?? false}
      initialDailyReportTime={state.working?.daily_report_time ?? "18:00"}
      showDefaultUserId
      submitting={state.loading}
      error={state.error}
      onCancel={onCancel}
      onSubmit={actions.finalize}
    />
  )
}

interface StepContentProps {
  state: WecomAgentSetupState
  actions: WecomAgentSetupActions
  onCancel: () => void
}
