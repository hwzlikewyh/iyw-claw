"use client"

import { useCallback, useEffect, useState } from "react"
import { Eye, EyeOff, KeyRound, Loader2, Save, Trash2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import {
  deleteChatNaturalRouterApiKey,
  getChatCommandPrefix,
  getChatNaturalRouterConfig,
  saveChatNaturalRouterApiKey,
  setChatCommandPrefix,
  setChatNaturalRouterConfig,
} from "@/lib/api"
import type {
  ChatNaturalRouterConfig,
  ChatNaturalRouterConfigInput,
} from "@/lib/types"

const BUILT_IN_COMMANDS = [
  { name: "folder [n|path]", descKey: "folderDesc" },
  { name: "agent [n|name]", descKey: "agentDesc" },
  { name: "task <description>", descKey: "taskDesc" },
  { name: "sessions", descKey: "sessionsDesc" },
  { name: "resume [id]", descKey: "resumeDesc" },
  { name: "cancel", descKey: "cancelDesc" },
  { name: "approve [always]", descKey: "approveDesc" },
  { name: "deny", descKey: "denyDesc" },
  { name: "search <keyword>", descKey: "searchDesc" },
  { name: "today", descKey: "todayDesc" },
  { name: "status", descKey: "statusDesc" },
  { name: "help", descKey: "helpDesc" },
] as const

const DEFAULT_ROUTER_CONFIG: ChatNaturalRouterConfig = {
  enabled: false,
  apiUrl: "https://api.openai.com/v1/chat/completions",
  model: "gpt-4o-mini",
  timeoutMs: 6000,
  minConfidence: 0.72,
  hasApiKey: false,
}

export function ChannelCommandsTab() {
  const t = useTranslations("ChatChannelSettings.commands")
  const [prefix, setPrefix] = useState("/")
  const [inputPrefix, setInputPrefix] = useState("/")
  const [routerConfig, setRouterConfig] = useState<ChatNaturalRouterConfig>(
    DEFAULT_ROUTER_CONFIG
  )
  const [routerDraft, setRouterDraft] = useState<ChatNaturalRouterConfigInput>(
    DEFAULT_ROUTER_CONFIG
  )
  const [apiKey, setApiKey] = useState("")
  const [showApiKey, setShowApiKey] = useState(false)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [routerSaving, setRouterSaving] = useState(false)
  const [keySaving, setKeySaving] = useState(false)

  useEffect(() => {
    Promise.allSettled([getChatCommandPrefix(), getChatNaturalRouterConfig()])
      .then(([prefixResult, routerResult]) => {
        if (prefixResult.status === "fulfilled") {
          setPrefix(prefixResult.value)
          setInputPrefix(prefixResult.value)
        }
        if (routerResult.status === "fulfilled") {
          setRouterConfig(routerResult.value)
          setRouterDraft({
            enabled: routerResult.value.enabled,
            apiUrl: routerResult.value.apiUrl,
            model: routerResult.value.model,
            timeoutMs: routerResult.value.timeoutMs,
            minConfidence: routerResult.value.minConfidence,
          })
        }
      })
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [])

  const handleSavePrefix = useCallback(async () => {
    const trimmed = inputPrefix.trim()
    if (
      trimmed.length === 0 ||
      trimmed.length > 3 ||
      /[a-zA-Z0-9]/.test(trimmed)
    ) {
      toast.error(t("prefixInvalid"))
      return
    }
    setSaving(true)
    try {
      await setChatCommandPrefix(trimmed)
      setPrefix(trimmed)
      toast.success(t("prefixSaved"))
    } catch {
      toast.error(t("prefixSaveFailed"))
    } finally {
      setSaving(false)
    }
  }, [inputPrefix, t])

  const handleSaveRouter = useCallback(async () => {
    const next = {
      ...routerDraft,
      apiUrl: routerDraft.apiUrl.trim(),
      model: routerDraft.model.trim(),
      timeoutMs: Number(routerDraft.timeoutMs),
      minConfidence: Number(routerDraft.minConfidence),
    }
    if (
      !next.apiUrl ||
      !next.model ||
      !Number.isFinite(next.timeoutMs) ||
      next.timeoutMs < 1000 ||
      next.timeoutMs > 30000 ||
      !Number.isFinite(next.minConfidence) ||
      next.minConfidence < 0 ||
      next.minConfidence > 1
    ) {
      toast.error(t("routerConfigInvalid"))
      return
    }

    setRouterSaving(true)
    try {
      await setChatNaturalRouterConfig(next)
      setRouterConfig((current) => ({
        ...next,
        hasApiKey: current.hasApiKey,
      }))
      setRouterDraft(next)
      toast.success(t("routerConfigSaved"))
    } catch {
      toast.error(t("routerConfigSaveFailed"))
    } finally {
      setRouterSaving(false)
    }
  }, [routerDraft, t])

  const handleSaveApiKey = useCallback(async () => {
    const token = apiKey.trim()
    if (!token) {
      toast.error(t("routerApiKeyEmpty"))
      return
    }
    setKeySaving(true)
    try {
      await saveChatNaturalRouterApiKey(token)
      setApiKey("")
      setRouterConfig((current) => ({ ...current, hasApiKey: true }))
      toast.success(t("routerApiKeySaved"))
    } catch {
      toast.error(t("routerApiKeySaveFailed"))
    } finally {
      setKeySaving(false)
    }
  }, [apiKey, t])

  const handleDeleteApiKey = useCallback(async () => {
    setKeySaving(true)
    try {
      await deleteChatNaturalRouterApiKey()
      setRouterConfig((current) => ({ ...current, hasApiKey: false }))
      toast.success(t("routerApiKeyDeleted"))
    } catch {
      toast.error(t("routerApiKeyDeleteFailed"))
    } finally {
      setKeySaving(false)
    }
  }, [t])

  const dirty = inputPrefix !== prefix
  const routerDirty =
    routerDraft.enabled !== routerConfig.enabled ||
    routerDraft.apiUrl !== routerConfig.apiUrl ||
    routerDraft.model !== routerConfig.model ||
    routerDraft.timeoutMs !== routerConfig.timeoutMs ||
    routerDraft.minConfidence !== routerConfig.minConfidence

  if (loading) {
    return (
      <div className="h-full flex items-center justify-center text-sm text-muted-foreground gap-2">
        <Loader2 className="h-4 w-4 animate-spin" />
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <section className="space-y-4">
        <div className="space-y-1">
          <h3 className="text-sm font-medium">{t("routerTitle")}</h3>
          <p className="text-xs text-muted-foreground">
            {t("routerDescription")}
          </p>
        </div>

        <div className="flex items-center justify-between gap-4 rounded-lg border px-4 py-3">
          <div className="space-y-0.5">
            <Label htmlFor="chat-router-enabled" className="text-sm">
              {t("routerEnabled")}
            </Label>
            <p className="text-xs text-muted-foreground">
              {t("routerEnabledHint")}
            </p>
          </div>
          <Switch
            id="chat-router-enabled"
            checked={routerDraft.enabled}
            onCheckedChange={(enabled) =>
              setRouterDraft((current) => ({ ...current, enabled }))
            }
          />
        </div>

        <div className="grid gap-3 md:grid-cols-2">
          <div className="space-y-1.5">
            <Label htmlFor="chat-router-api-url" className="text-xs">
              {t("routerApiUrlLabel")}
            </Label>
            <Input
              id="chat-router-api-url"
              value={routerDraft.apiUrl}
              placeholder={t("routerApiUrlPlaceholder")}
              onChange={(e) =>
                setRouterDraft((current) => ({
                  ...current,
                  apiUrl: e.target.value,
                }))
              }
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="chat-router-model" className="text-xs">
              {t("routerModelLabel")}
            </Label>
            <Input
              id="chat-router-model"
              value={routerDraft.model}
              onChange={(e) =>
                setRouterDraft((current) => ({
                  ...current,
                  model: e.target.value,
                }))
              }
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="chat-router-timeout" className="text-xs">
              {t("routerTimeoutLabel")}
            </Label>
            <Input
              id="chat-router-timeout"
              type="number"
              min={1000}
              max={30000}
              step={500}
              value={routerDraft.timeoutMs}
              onChange={(e) =>
                setRouterDraft((current) => ({
                  ...current,
                  timeoutMs: Number(e.target.value),
                }))
              }
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="chat-router-confidence" className="text-xs">
              {t("routerConfidenceLabel")}
            </Label>
            <Input
              id="chat-router-confidence"
              type="number"
              min={0}
              max={1}
              step={0.01}
              value={routerDraft.minConfidence}
              onChange={(e) =>
                setRouterDraft((current) => ({
                  ...current,
                  minConfidence: Number(e.target.value),
                }))
              }
            />
          </div>
        </div>

        <div className="flex items-center gap-2">
          <Button
            size="sm"
            disabled={!routerDirty || routerSaving}
            onClick={handleSaveRouter}
          >
            {routerSaving ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Save className="h-3.5 w-3.5 mr-1" />
            )}
            {t("save")}
          </Button>
          <span className="text-xs text-muted-foreground">
            {routerConfig.hasApiKey
              ? t("routerApiKeyStored")
              : t("routerApiKeyMissing")}
          </span>
        </div>

        <div className="space-y-1.5">
          <Label htmlFor="chat-router-api-key" className="text-xs">
            {t("routerApiKeyLabel")}
          </Label>
          <div className="flex items-center gap-2">
            <div className="relative min-w-0 flex-1">
              <Input
                id="chat-router-api-key"
                type={showApiKey ? "text" : "password"}
                value={apiKey}
                placeholder={t("routerApiKeyPlaceholder")}
                onChange={(e) => setApiKey(e.target.value)}
                className="pr-9"
              />
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="absolute right-1 top-1/2 h-7 w-7 -translate-y-1/2"
                onClick={() => setShowApiKey((value) => !value)}
                aria-label={t("routerApiKeyToggle")}
              >
                {showApiKey ? (
                  <EyeOff className="h-3.5 w-3.5" />
                ) : (
                  <Eye className="h-3.5 w-3.5" />
                )}
              </Button>
            </div>
            <Button size="sm" disabled={keySaving} onClick={handleSaveApiKey}>
              {keySaving ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <KeyRound className="h-3.5 w-3.5 mr-1" />
              )}
              {t("routerApiKeySave")}
            </Button>
            {routerConfig.hasApiKey && (
              <Button
                size="icon"
                variant="outline"
                disabled={keySaving}
                onClick={handleDeleteApiKey}
                aria-label={t("routerApiKeyDelete")}
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            )}
          </div>
        </div>
      </section>

      <section className="space-y-2">
        <h3 className="text-sm font-medium">{t("prefixLabel")}</h3>
        <p className="text-xs text-muted-foreground">
          {t("prefixDescription")}
        </p>
        <div className="flex items-center gap-2">
          <Input
            value={inputPrefix}
            onChange={(e) => setInputPrefix(e.target.value)}
            className="w-20 text-center font-mono"
            maxLength={3}
          />
          <Button
            size="sm"
            disabled={!dirty || saving}
            onClick={handleSavePrefix}
          >
            {saving ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Save className="h-3.5 w-3.5 mr-1" />
            )}
            {t("save")}
          </Button>
        </div>
      </section>

      <section className="space-y-2">
        <h3 className="text-sm font-medium">{t("title")}</h3>
        <p className="text-xs text-muted-foreground">{t("description")}</p>
        <div className="space-y-1">
          {BUILT_IN_COMMANDS.map((cmd) => (
            <div
              key={cmd.name}
              className="flex items-center justify-between rounded-lg border bg-card px-4 py-3"
            >
              <code className="text-sm font-mono">
                {prefix}
                {cmd.name}
              </code>
              <span className="text-xs text-muted-foreground">
                {t(cmd.descKey)}
              </span>
            </div>
          ))}
        </div>
      </section>
    </div>
  )
}
