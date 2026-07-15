"use client"

import { useCallback, useEffect, useState } from "react"
import { Loader2, Save } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import {
  getChatCommandPrefix,
  getChatNaturalRouterConfig,
  setChatCommandPrefix,
  setChatNaturalRouterConfig,
} from "@/lib/api"
import {
  MANAGED_DEFAULT_MODEL,
  MANAGED_MODEL_OPTIONS,
  type ChatNaturalRouterConfig,
  type ChatNaturalRouterConfigInput,
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
  enabled: true,
  apiUrl: "",
  model: MANAGED_DEFAULT_MODEL,
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
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [routerSaving, setRouterSaving] = useState(false)

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
      model: routerDraft.model.trim(),
    }
    if (!MANAGED_MODEL_OPTIONS.some((model) => model === next.model)) {
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

  const dirty = inputPrefix !== prefix
  const routerDirty =
    routerDraft.enabled !== routerConfig.enabled ||
    routerDraft.model !== routerConfig.model

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

        <div className="flex items-end gap-2">
          <div className="min-w-0 flex-1 space-y-1.5">
            <Label htmlFor="chat-router-model" className="text-xs">
              {t("routerModelLabel")}
            </Label>
            <Select
              value={routerDraft.model}
              onValueChange={(model) =>
                setRouterDraft((current) => ({
                  ...current,
                  model,
                }))
              }
            >
              <SelectTrigger id="chat-router-model" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent align="start">
                {MANAGED_MODEL_OPTIONS.map((model) => (
                  <SelectItem key={model} value={model}>
                    {model}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
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
