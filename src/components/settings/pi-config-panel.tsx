"use client"

import { useCallback, useEffect, useState } from "react"
import { KeyRound, Loader2, Save, ShieldCheck } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Switch } from "@/components/ui/switch"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { acpUpdatePiConfig, loadPiConfig } from "@/lib/api"
import type { AcpAgentInfo } from "@/lib/types"
import { useAgentSdkTranslations } from "@/hooks/use-agent-sdk-translations"

/**
 * Per-agent `env_json` flag gating launch-time workspace-trust seeding. Absent or
 * any value other than `"0"` ⇒ enabled (default on): when iyw-claw connects pi to a
 * folder, the backend marks that folder trusted in pi's `trust.json` so pi loads
 * the project's local config/skills without a separate prompt. `"0"` disables.
 * Read by `seed_pi_workspace_trust` in the Rust launch path.
 */
const PI_TRUST_WORKSPACE_ENV = "PI_ACP_TRUST_WORKSPACE"

/**
 * The managed Pi runtime and profile paths are backend-owned. Workspace trust
 * is the only Pi launch setting users may override from this panel.
 */
export const PI_RESERVED_ENV_KEYS = [PI_TRUST_WORKSPACE_ENV] as const

const PI_THINKING_LEVELS = [
  "off",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
] as const

const PI_MANAGED_PROVIDER_ID = "iyw-claw"

export function buildManagedPiConfig(model: string, thinkingLevel: string) {
  return {
    provider: PI_MANAGED_PROVIDER_ID,
    model: model.trim(),
    thinkingLevel: thinkingLevel || undefined,
  }
}

/**
 * Dedicated Pi settings panel. The runtime binary and profile directories are
 * always resolved from private Agent storage; this panel only manages native
 * Pi settings and workspace trust.
 */
export function PiConfigPanel({
  agent,
  onSaveEnv,
  onSaved,
}: {
  agent: AcpAgentInfo
  onSaveEnv: (env: Record<string, string>, enabled: boolean) => Promise<unknown>
  onSaved: () => Promise<void>
}) {
  const t = useAgentSdkTranslations()

  // The model remains user-selectable; the provider is always iyw-claw.
  const [model, setModel] = useState("")
  const [thinkingLevel, setThinkingLevel] = useState("")
  const [savingCreds, setSavingCreds] = useState(false)
  const [loadingCreds, setLoadingCreds] = useState(true)

  useEffect(() => {
    let cancelled = false
    setLoadingCreds(true)
    loadPiConfig()
      .then((cfg) => {
        if (cancelled) return
        setModel(cfg.defaultModel ?? "")
        setThinkingLevel(cfg.defaultThinkingLevel ?? "")
      })
      .catch((error) => {
        console.error("[Pi] load config failed", error)
      })
      .finally(() => {
        if (!cancelled) setLoadingCreds(false)
      })
    return () => {
      cancelled = true
    }
  }, [])

  const handleSaveCreds = useCallback(async () => {
    const trimmedModel = model.trim()
    if (!trimmedModel) {
      toast.error(t("pi.modelRequired"))
      return
    }
    setSavingCreds(true)
    try {
      await acpUpdatePiConfig(buildManagedPiConfig(model, thinkingLevel))
      await onSaved()
      toast.success(t("toasts.piSaved"))
    } catch (error) {
      console.error("[Pi] save config failed", error)
      toast.error(t("toasts.savePiFailed"))
    } finally {
      setSavingCreds(false)
    }
  }, [model, thinkingLevel, onSaved, t])

  const credsIncomplete = !model.trim()

  // Workspace trust (default on): seeded into pi's trust.json at launch so pi
  // loads the opened folder's local config/skills without a separate prompt.
  const [trustWorkspace, setTrustWorkspace] = useState(
    () => (agent.env[PI_TRUST_WORKSPACE_ENV] ?? "1") !== "0"
  )
  const [savingTrust, setSavingTrust] = useState(false)

  // Self-persisting toggle: write the flag straight to env_json on change. Default
  // on ⇒ omit the key when enabling (absence = default), write "0" when disabling.
  const handleToggleTrust = useCallback(
    async (next: boolean) => {
      setTrustWorkspace(next)
      setSavingTrust(true)
      const env = { ...agent.env }
      if (next) delete env[PI_TRUST_WORKSPACE_ENV]
      else env[PI_TRUST_WORKSPACE_ENV] = "0"
      try {
        await onSaveEnv(env, agent.enabled)
      } catch (error) {
        console.error("[Pi] save workspace trust failed", error)
        setTrustWorkspace(!next)
        toast.error(t("toasts.savePiTrustFailed"))
      } finally {
        setSavingTrust(false)
      }
    },
    [agent.env, agent.enabled, onSaveEnv, t]
  )

  return (
    <div className="space-y-4">
      {/* Credentials / model — pi's native settings.json / auth.json */}
      <div className="space-y-3 rounded-md border bg-muted/10 p-3">
        <div>
          <label className="flex items-center gap-1.5 text-xs font-medium">
            <KeyRound className="h-3.5 w-3.5 text-muted-foreground" />
            {t("pi.configManagement")}
          </label>
          <p className="mt-1 text-[11px] text-muted-foreground">
            {t("pi.configDescription")}
          </p>
        </div>

        <div className="space-y-1.5">
          <label className="text-[11px] text-muted-foreground">
            {t("pi.modelLabel")}
          </label>
          <Input
            value={model}
            onChange={(event) => setModel(event.target.value)}
            placeholder="claude-sonnet-4-20250514"
            spellCheck={false}
            disabled={savingCreds || loadingCreds}
          />
        </div>

        <div className="space-y-1.5">
          <label className="text-[11px] text-muted-foreground">
            {t("pi.thinkingLabel")}
          </label>
          <Select
            value={thinkingLevel || "off"}
            onValueChange={(value) => setThinkingLevel(value)}
            disabled={savingCreds || loadingCreds}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent align="start">
              {PI_THINKING_LEVELS.map((level) => (
                <SelectItem key={level} value={level}>
                  {t(`pi.thinking.${level}`)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="flex justify-end">
          <Button
            type="button"
            size="sm"
            onClick={handleSaveCreds}
            disabled={savingCreds || loadingCreds || credsIncomplete}
            className="gap-1.5"
          >
            {savingCreds ? (
              <>
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                {t("actions.saving")}
              </>
            ) : (
              <>
                <Save className="h-3.5 w-3.5" />
                {t("pi.saveConfig")}
              </>
            )}
          </Button>
        </div>
      </div>

      {/* Workspace trust — auto-trust the folder iyw-claw launches pi into */}
      <div className="space-y-2 rounded-md border bg-muted/10 p-3">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <label
              htmlFor="pi-trust-workspace"
              className="flex items-center gap-1.5 text-xs font-medium"
            >
              <ShieldCheck className="h-3.5 w-3.5 text-muted-foreground" />
              {t("pi.trustTitle")}
            </label>
            <p className="mt-1 text-[11px] text-muted-foreground">
              {t("pi.trustDescription")}
            </p>
          </div>
          <Switch
            id="pi-trust-workspace"
            checked={trustWorkspace}
            onCheckedChange={handleToggleTrust}
            disabled={savingTrust}
          />
        </div>
        <p className="text-[11px] text-muted-foreground">{t("pi.trustHint")}</p>
      </div>
    </div>
  )
}
