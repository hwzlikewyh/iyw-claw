"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import type { ChangeEventHandler } from "react"
import { useTranslations } from "next-intl"
import { Eye, EyeOff } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type { AcpAgentInfo } from "@/lib/types"
import { cn } from "@/lib/utils"

const API_KEY_ENV = "DEEPSEEK_API_KEY"
const BASE_URL_ENV = "DEEPSEEK_BASE_URL"
const PROVIDER_ENV = "DEEPSEEK_ACP_PROVIDER"
const MODEL_ENV = "DEEPSEEK_ACP_MODEL"

export const DEEPSEEK_DEFAULT_BASE_URL = "https://api.deepseek.com"
export const DEEPSEEK_PANEL_ENV_KEYS = [
  API_KEY_ENV,
  BASE_URL_ENV,
  PROVIDER_ENV,
  MODEL_ENV,
] as const

export function buildDeepSeekEnv(
  previousEnv: Record<string, string>,
  apiKey: string,
  baseUrl: string,
  provider: string,
  model: string
): Record<string, string> {
  const env = { ...previousEnv }
  const setOrDelete = (key: string, value: string) => {
    const trimmed = value.trim()
    if (trimmed) env[key] = trimmed
    else delete env[key]
  }
  if (/^https?:\/\//i.test((env[PROVIDER_ENV] ?? "").trim())) {
    delete env[PROVIDER_ENV]
  }
  setOrDelete(API_KEY_ENV, apiKey)
  setOrDelete(BASE_URL_ENV, baseUrl.trim().replace(/\/+$/, ""))
  setOrDelete(PROVIDER_ENV, provider)
  setOrDelete(MODEL_ENV, model)
  return env
}

export function isValidDeepSeekBaseUrl(value: string): boolean {
  const trimmed = value.trim()
  if (!trimmed) return true
  if (/[?#]/.test(trimmed)) return false
  try {
    const url = new URL(trimmed)
    return (
      (url.protocol === "http:" || url.protocol === "https:") &&
      !url.username &&
      !url.password
    )
  } catch {
    return false
  }
}

export function isValidDeepSeekProvider(value: string): boolean {
  const trimmed = value.trim()
  return (
    !trimmed ||
    (/^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$/.test(trimmed) && trimmed.length <= 64)
  )
}

function useSyncedField(persisted: string) {
  const [value, setValue] = useState(persisted)
  const seeded = useRef(persisted)
  const dirty = useRef(false)

  useEffect(() => {
    /* eslint-disable react-hooks/set-state-in-effect */
    if (seeded.current !== persisted) {
      seeded.current = persisted
      if (!dirty.current) setValue(persisted)
    }
    /* eslint-enable react-hooks/set-state-in-effect */
  }, [persisted])

  const onChange = useCallback<ChangeEventHandler<HTMLInputElement>>(
    (event) => {
      dirty.current = true
      setValue(event.target.value)
    },
    []
  )
  const markSaved = useCallback((next: string) => {
    seeded.current = next
    dirty.current = false
    setValue(next)
  }, [])
  return { value, onChange, markSaved }
}

export function useDeepSeekFields(agent: AcpAgentInfo) {
  const storedProvider = agent.env[PROVIDER_ENV] ?? ""
  const providerValue = /^https?:\/\//i.test(storedProvider.trim())
    ? ""
    : storedProvider
  const apiKey = useSyncedField(agent.env[API_KEY_ENV] ?? "")
  const baseUrl = useSyncedField(agent.env[BASE_URL_ENV] ?? "")
  const provider = useSyncedField(providerValue)
  const model = useSyncedField(agent.env[MODEL_ENV] ?? "")
  const markApiKeySaved = apiKey.markSaved
  const markBaseUrlSaved = baseUrl.markSaved
  const markProviderSaved = provider.markSaved
  const markModelSaved = model.markSaved
  const markSaved = useCallback(
    (env: Record<string, string>) => {
      markApiKeySaved(env[API_KEY_ENV] ?? "")
      markBaseUrlSaved(env[BASE_URL_ENV] ?? "")
      markProviderSaved(env[PROVIDER_ENV] ?? "")
      markModelSaved(env[MODEL_ENV] ?? "")
    },
    [markApiKeySaved, markBaseUrlSaved, markModelSaved, markProviderSaved]
  )
  return { apiKey, baseUrl, provider, model, markSaved }
}

function DeepSeekTextField({
  id,
  label,
  value,
  onChange,
  placeholder,
  hint,
  invalid = false,
  type = "text",
  saving,
}: {
  id: string
  label: string
  value: string
  onChange: ChangeEventHandler<HTMLInputElement>
  placeholder?: string
  hint: string
  invalid?: boolean
  type?: string
  saving: boolean
}) {
  const hintId = `${id}-hint`
  return (
    <div className="space-y-1.5">
      <label htmlFor={id} className="text-[11px] text-muted-foreground">
        {label}
      </label>
      <Input
        id={id}
        type={type}
        value={value}
        onChange={onChange}
        placeholder={placeholder}
        disabled={saving}
        aria-invalid={invalid}
        aria-describedby={hintId}
      />
      <p
        id={hintId}
        className={cn(
          "text-[11px]",
          invalid ? "text-destructive" : "text-muted-foreground"
        )}
      >
        {hint}
      </p>
    </div>
  )
}

export function DeepSeekBaseUrlField({
  value,
  saving,
  onChange,
}: {
  value: string
  saving: boolean
  onChange: ChangeEventHandler<HTMLInputElement>
}) {
  const t = useTranslations("AcpAgentSettings")
  const valid = isValidDeepSeekBaseUrl(value)
  return (
    <DeepSeekTextField
      id="deepseek-base-url"
      type="url"
      label={t("deepseek.baseUrlLabel")}
      value={value}
      onChange={onChange}
      placeholder={DEEPSEEK_DEFAULT_BASE_URL}
      hint={valid ? t("deepseek.baseUrlHint") : t("deepseek.baseUrlInvalid")}
      invalid={!valid}
      saving={saving}
    />
  )
}

export function DeepSeekProviderField({
  value,
  saving,
  onChange,
}: {
  value: string
  saving: boolean
  onChange: ChangeEventHandler<HTMLInputElement>
}) {
  const t = useTranslations("AcpAgentSettings")
  const valid = isValidDeepSeekProvider(value)
  return (
    <DeepSeekTextField
      id="deepseek-provider"
      label={t("deepseek.providerLabel")}
      value={value}
      onChange={onChange}
      placeholder="deepseek"
      hint={valid ? t("deepseek.providerHint") : t("deepseek.providerInvalid")}
      invalid={!valid}
      saving={saving}
    />
  )
}

export function DeepSeekModelField({
  value,
  saving,
  onChange,
}: {
  value: string
  saving: boolean
  onChange: ChangeEventHandler<HTMLInputElement>
}) {
  const t = useTranslations("AcpAgentSettings")
  return (
    <DeepSeekTextField
      id="deepseek-model"
      label={t("deepseek.modelLabel")}
      value={value}
      onChange={onChange}
      placeholder="deepseek-chat"
      hint={t("deepseek.modelHint")}
      saving={saving}
    />
  )
}

export function DeepSeekApiKeyField({
  value,
  saving,
  onChange,
}: {
  value: string
  saving: boolean
  onChange: ChangeEventHandler<HTMLInputElement>
}) {
  const t = useTranslations("AcpAgentSettings")
  const [showKey, setShowKey] = useState(false)
  const actionLabel = t(showKey ? "actions.hideApiKey" : "actions.showApiKey")
  return (
    <div className="space-y-1.5">
      <label
        htmlFor="deepseek-api-key"
        className="text-[11px] text-muted-foreground"
      >
        {t("deepseek.apiKeyLabel")}
      </label>
      <div className="flex items-center gap-2">
        <Input
          id="deepseek-api-key"
          type={showKey ? "text" : "password"}
          value={value}
          onChange={onChange}
          placeholder="sk-..."
          disabled={saving}
        />
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => setShowKey((visible) => !visible)}
          title={actionLabel}
          aria-label={actionLabel}
        >
          {showKey ? (
            <EyeOff className="h-3.5 w-3.5" />
          ) : (
            <Eye className="h-3.5 w-3.5" />
          )}
        </Button>
      </div>
      <p className="text-[11px] text-muted-foreground">
        {t("deepseek.apiKeyHint")}
      </p>
    </div>
  )
}
