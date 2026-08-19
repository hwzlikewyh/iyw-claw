"use client"

import { Bot, Loader2, PlugZap, RefreshCw, Trash2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { MarketBadge } from "@/components/skills/market/badges"
import {
  artifactStatusBadgeInfo,
  formatSkillBytes,
  type SkillMarketTranslator,
  type SkillMarketV2Detail,
  type SkillMarketV2Version,
} from "@/lib/skill-market"
import type { SkillMarketActivationSummary } from "@/lib/skill-market-activation"
import { getAgentLabel } from "@/lib/custom-agents"
import { cn } from "@/lib/utils"

function agentState({
  enabled,
  total,
}: {
  enabled: number
  total: number
}): "active" | "partial" | "inactive" {
  if (!enabled) return "inactive"
  return enabled === total ? "active" : "partial"
}

interface DetailSidePanelProps {
  detail: SkillMarketV2Detail
  version: SkillMarketV2Version
  activation: SkillMarketActivationSummary
  activationBusy: boolean
  activationError: string | null
  onEnableAll: () => void
  onOpenInventory: () => void
  onOpenConnectors: () => void
  onRetryActivation: () => void
  onUninstall: () => void
}

export function DetailSidePanel(props: DetailSidePanelProps) {
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
  return (
    <aside className="min-h-0 overflow-y-auto border-t bg-muted/10 p-4 lg:border-t-0 lg:border-l lg:p-5">
      <section className="border-b pb-4">
        <div className="flex items-center justify-between gap-3">
          <h3 className="text-xs font-semibold">
            {t("detail.activation.targets")}
          </h3>
          {props.activation.agents.length ? (
            <Button
              variant="link"
              size="xs"
              className="h-auto px-0"
              disabled={props.activationBusy}
              onClick={props.onOpenInventory}
            >
              {t("detail.activation.manage")}
            </Button>
          ) : null}
        </div>
        {props.activationError ? (
          <div className="mt-2 border-l-2 border-destructive bg-destructive/5 px-3 py-2">
            <p className="text-[10px] font-medium text-destructive">
              {t("detail.activation.loadFailed")}
            </p>
            <p className="mt-1 break-words text-[10px] leading-4 text-muted-foreground">
              {props.activationError}
            </p>
            <Button
              variant="link"
              size="xs"
              className="mt-1 h-auto px-0"
              disabled={props.activationBusy}
              onClick={props.onRetryActivation}
            >
              <RefreshCw className="size-3" />
              {t("detail.activation.retry")}
            </Button>
          </div>
        ) : null}
        {props.activation.kind === "loading" ? (
          <div className="flex min-h-20 items-center justify-center">
            <Loader2 className="size-4 animate-spin" />
          </div>
        ) : props.activation.agents.length ? (
          <div className="mt-2 divide-y border-y">
            {props.activation.agents.map((agent) => {
              const state = agentState({
                enabled: agent.enabledCount,
                total: agent.totalCount,
              })
              return (
                <div
                  key={agent.agentType}
                  className="flex min-h-12 items-start gap-2.5 py-2"
                >
                  <span className="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded-md border bg-background">
                    <Bot className="size-3.5 text-muted-foreground" />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-xs font-medium">
                      {getAgentLabel(agent.agentType)}
                    </span>
                    <span className="block text-[9px] text-muted-foreground">
                      {t("detail.activation.componentCount", {
                        enabled: agent.enabledCount,
                        total: agent.totalCount,
                      })}
                    </span>
                    {agent.requiredBy.length ? (
                      <span className="mt-1 block break-words text-[9px] leading-4 text-amber-700 dark:text-amber-300">
                        {t("detail.activation.requiredBy", {
                          values: agent.requiredBy.join(", "),
                        })}
                      </span>
                    ) : null}
                    {agent.blockedReasons.length ? (
                      <span className="mt-1 block break-words text-[9px] leading-4 text-destructive">
                        {t("detail.activation.blockedReasons", {
                          values: agent.blockedReasons.join(", "),
                        })}
                      </span>
                    ) : null}
                  </span>
                  <span
                    role="status"
                    aria-label={t(`detail.activation.${state}`)}
                    className={cn(
                      "mt-2 size-2 rounded-full bg-muted-foreground/40",
                      state === "active" && "bg-emerald-600",
                      state === "partial" && "bg-amber-500"
                    )}
                    title={t(`detail.activation.${state}`)}
                  />
                </div>
              )
            })}
          </div>
        ) : props.activationError ? null : (
          <p className="mt-2 text-xs leading-5 text-muted-foreground">
            {t(`detail.activation.${props.activation.kind}Hint`)}
          </p>
        )}
        {props.activation.kind === "connector_only" ? (
          <Button
            size="sm"
            variant="outline"
            className="mt-3 w-full"
            disabled={props.activationBusy}
            onClick={props.onOpenConnectors}
          >
            <PlugZap className="size-3.5" />
            {t("detail.activation.manageConnectors")}
          </Button>
        ) : null}
        {props.activation.kind === "partial" && props.activation.canEnable ? (
          <Button
            size="sm"
            variant="outline"
            className="mt-3 w-full"
            disabled={props.activationBusy}
            onClick={props.onEnableAll}
          >
            {t("detail.activation.enableAll")}
          </Button>
        ) : null}
      </section>
      <section className="border-b py-4">
        <h3 className="text-xs font-semibold">
          {t("detail.versionAndCompatibility")}
        </h3>
        <dl className="mt-2 divide-y">
          <Fact
            label={t("detail.installedVersion")}
            value={
              props.detail.installedVersion
                ? `v${props.detail.installedVersion}`
                : "-"
            }
          />
          <Fact
            label={t("detail.compatibility")}
            value={t(`compatibility.${props.detail.compatibility}`)}
          />
          <Fact
            label={t("detail.artifact")}
            value={
              <MarketBadge
                info={artifactStatusBadgeInfo(props.version.status)}
              />
            }
          />
          <Fact
            label={t("install.downloadSize")}
            value={formatSkillBytes(props.version.artifactSize)}
          />
        </dl>
      </section>
      {props.activation.targets.some((target) => target.requiredBy.length) ? (
        <p className="my-4 border-l-2 border-amber-500 bg-amber-500/8 px-3 py-2 text-[10px] leading-4 text-muted-foreground">
          {t("detail.activation.requiredHint")}
        </p>
      ) : null}
      {props.detail.installState !== "not_installed" ? (
        <Button
          size="sm"
          variant="outline"
          className="mt-4 w-full text-destructive"
          disabled={props.activationBusy}
          onClick={props.onUninstall}
        >
          <Trash2 className="size-3.5" />
          {t("manage.uninstall")}
        </Button>
      ) : null}
    </aside>
  )
}

function Fact({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex min-h-9 items-center justify-between gap-3 text-[10px]">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="min-w-0 text-right font-medium">{value}</dd>
    </div>
  )
}
