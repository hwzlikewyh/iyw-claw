"use client"

import { RefreshCw } from "lucide-react"
import { useTranslations } from "next-intl"

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { ChannelSecretValueField } from "./channel-secret-value-field"
import {
  CopyField,
  ErrorBox,
  SetupField,
  SetupStatus,
  WecomAgentGuide,
  WizardActions,
} from "./wecom-agent-setup-parts"

export function WecomAgentRequirementsStep({
  webRunning,
  externalUrl,
  conditionConfirmed,
  loading,
  nextDisabled,
  onExternalUrlChange,
  onConditionConfirmedChange,
  onCancel,
  onNext,
}: {
  webRunning: boolean | null
  externalUrl: string
  conditionConfirmed: boolean
  loading: boolean
  nextDisabled: boolean
  onExternalUrlChange: (value: string) => void
  onConditionConfirmedChange: (value: boolean) => void
  onCancel: () => void
  onNext: () => void
}) {
  const t = useTranslations("ChatChannelSettings")
  return (
    <div className="min-w-0 space-y-4">
      <SetupStatus
        ok={webRunning === true}
        text={t(
          webRunning
            ? "market.wecomAgent.webRunning"
            : "market.wecomAgent.webNotRunning"
        )}
      />
      <div className="space-y-1.5">
        <label className="text-xs font-medium">
          {t("market.wecomAgent.externalUrl")}
        </label>
        <Input
          value={externalUrl}
          onChange={(event) => onExternalUrlChange(event.target.value)}
          placeholder="https://assistant.example.com"
        />
        <p className="text-xs text-muted-foreground">
          {t("market.wecomAgent.externalUrlHint")}
        </p>
      </div>
      <label className="flex items-start gap-2 text-xs text-muted-foreground">
        <Checkbox
          checked={conditionConfirmed}
          onCheckedChange={(value) =>
            onConditionConfirmedChange(value === true)
          }
        />
        {t("market.wecomAgent.conditionConfirm")}
      </label>
      <WizardActions
        loading={loading}
        nextDisabled={nextDisabled}
        onCancel={onCancel}
        onNext={onNext}
      />
    </div>
  )
}

export function WecomAgentCredentialsStep({
  corpId,
  agentId,
  appSecret,
  loading,
  error,
  onCorpIdChange,
  onAgentIdChange,
  onAppSecretChange,
  onCancel,
  onNext,
}: {
  corpId: string
  agentId: string
  appSecret: string
  loading: boolean
  error: string | null
  onCorpIdChange: (value: string) => void
  onAgentIdChange: (value: string) => void
  onAppSecretChange: (value: string) => void
  onCancel: () => void
  onNext: () => void
}) {
  const t = useTranslations("ChatChannelSettings")
  return (
    <div className="grid min-w-0 gap-5 md:grid-cols-2">
      <WecomAgentGuide />
      <div className="min-w-0 space-y-4">
        <SetupField
          label={t("market.wecomAgent.corpId")}
          value={corpId}
          onChange={onCorpIdChange}
        />
        <SetupField
          label={t("market.wecomAgent.agentId")}
          value={agentId}
          onChange={onAgentIdChange}
        />
        <SetupField
          label={t("market.wecomAgent.appSecret")}
          value={appSecret}
          onChange={onAppSecretChange}
          secret
        />
        {error && <ErrorBox message={error} />}
        <WizardActions loading={loading} onCancel={onCancel} onNext={onNext} />
      </div>
    </div>
  )
}

export function WecomAgentCallbackStep({
  callbackUrl,
  callbackToken,
  encodingAesKey,
  secretsAvailable,
  callbackVerified,
  loading,
  error,
  onRegenerate,
  onCancel,
  onNext,
}: {
  callbackUrl: string
  callbackToken: string
  encodingAesKey: string
  secretsAvailable: boolean
  callbackVerified: boolean
  loading: boolean
  error: string | null
  onRegenerate: () => void
  onCancel: () => void
  onNext: () => void
}) {
  const t = useTranslations("ChatChannelSettings")
  return (
    <div className="grid min-w-0 gap-5 md:grid-cols-2">
      <WecomAgentGuide />
      <div className="min-w-0 space-y-4">
        <CopyField
          label={t("market.wecomAgent.callbackUrl")}
          value={callbackUrl}
        />
        {secretsAvailable ? (
          <CallbackSecretFields
            callbackToken={callbackToken}
            encodingAesKey={encodingAesKey}
            onRegenerate={onRegenerate}
          />
        ) : (
          <div className="space-y-3 rounded-md border p-3">
            <p className="text-xs text-muted-foreground">
              {t("market.wecomAgent.credentialsRetained")}
            </p>
            <Button variant="outline" size="sm" onClick={onRegenerate}>
              <RefreshCw className="h-3.5 w-3.5" />
              {t("market.regenerate")}
            </Button>
          </div>
        )}
        <SetupStatus
          ok={callbackVerified}
          pending={!callbackVerified}
          text={t(
            callbackVerified
              ? "market.wecomAgent.callbackVerified"
              : "market.wecomAgent.waitingCallback"
          )}
        />
        {error && <ErrorBox message={error} />}
        <WizardActions
          loading={loading}
          nextDisabled={!callbackVerified}
          onCancel={onCancel}
          onNext={onNext}
        />
      </div>
    </div>
  )
}

function CallbackSecretFields({
  callbackToken,
  encodingAesKey,
  onRegenerate,
}: {
  callbackToken: string
  encodingAesKey: string
  onRegenerate: () => void
}) {
  return (
    <>
      <ChannelSecretValueField
        label="Token"
        value={callbackToken}
        onRegenerate={onRegenerate}
      />
      <ChannelSecretValueField
        label="EncodingAESKey"
        value={encodingAesKey}
        onRegenerate={onRegenerate}
      />
    </>
  )
}
