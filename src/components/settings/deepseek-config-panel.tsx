"use client"

import { useTranslations } from "next-intl"
import { Loader2, Save } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import {
  buildDeepSeekEnv,
  DeepSeekApiKeyField,
  DeepSeekBaseUrlField,
  DeepSeekModelField,
  DeepSeekProviderField,
  isValidDeepSeekBaseUrl,
  isValidDeepSeekProvider,
  useDeepSeekFields,
} from "@/components/settings/deepseek-config-fields"
import type { AcpAgentInfo } from "@/lib/types"

export { DEEPSEEK_PANEL_ENV_KEYS } from "@/components/settings/deepseek-config-fields"

function DeepSeekPanelHeader() {
  const t = useTranslations("AcpAgentSettings")
  return (
    <div>
      <label className="text-xs font-medium">
        {t("deepseek.configManagement")}
      </label>
      <p className="mt-1 text-[11px] text-muted-foreground">
        {t("deepseek.configDescription")}
      </p>
    </div>
  )
}

function DeepSeekSaveButton({
  saving,
  disabled,
  onSave,
}: {
  saving: boolean
  disabled: boolean
  onSave: () => void
}) {
  const t = useTranslations("AcpAgentSettings")
  return (
    <div className="flex justify-end">
      <Button
        type="button"
        size="sm"
        onClick={onSave}
        disabled={disabled}
        className="gap-1.5"
      >
        {saving ? (
          <>
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            {t("actions.saving")}
          </>
        ) : (
          <>
            <Save className="h-3.5 w-3.5" />
            {t("actions.saveDeepSeekConfig")}
          </>
        )}
      </Button>
    </div>
  )
}

function DeepSeekConfigFields({
  fields,
  saving,
}: {
  fields: ReturnType<typeof useDeepSeekFields>
  saving: boolean
}) {
  return (
    <>
      <DeepSeekBaseUrlField
        value={fields.baseUrl.value}
        saving={saving}
        onChange={fields.baseUrl.onChange}
      />
      <DeepSeekApiKeyField
        value={fields.apiKey.value}
        saving={saving}
        onChange={fields.apiKey.onChange}
      />
      <DeepSeekProviderField
        value={fields.provider.value}
        saving={saving}
        onChange={fields.provider.onChange}
      />
      <DeepSeekModelField
        value={fields.model.value}
        saving={saving}
        onChange={fields.model.onChange}
      />
    </>
  )
}

interface DeepSeekConfigPanelProps {
  agent: AcpAgentInfo
  saving: boolean
  onSaveEnv: (env: Record<string, string>, enabled: boolean) => Promise<unknown>
}

export function DeepSeekConfigPanel({
  agent,
  saving,
  onSaveEnv,
}: DeepSeekConfigPanelProps) {
  const t = useTranslations("AcpAgentSettings")
  const fields = useDeepSeekFields(agent)
  const baseUrlValid = isValidDeepSeekBaseUrl(fields.baseUrl.value)
  const providerValid = isValidDeepSeekProvider(fields.provider.value)

  const handleSave = async () => {
    const env = buildDeepSeekEnv(
      agent.env,
      fields.apiKey.value,
      fields.baseUrl.value,
      fields.provider.value,
      fields.model.value
    )
    try {
      await onSaveEnv(env, agent.enabled)
      fields.markSaved(env)
      toast.success(t("toasts.deepseekSaved"))
    } catch (error) {
      console.error("[DeepSeek] save config failed", error)
      toast.error(t("toasts.saveDeepSeekFailed"))
    }
  }

  return (
    <div className="space-y-3 rounded-md border bg-muted/10 p-3">
      <DeepSeekPanelHeader />
      <DeepSeekConfigFields fields={fields} saving={saving} />
      <DeepSeekSaveButton
        saving={saving}
        disabled={saving || !baseUrlValid || !providerValid}
        onSave={() => void handleSave()}
      />
    </div>
  )
}
