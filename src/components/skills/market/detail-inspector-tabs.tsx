"use client"

import {
  Bot,
  Building2,
  FolderTree,
  Loader2,
  ShieldAlert,
  Wrench,
} from "lucide-react"
import { useLocale, useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { MarketBadge } from "@/components/skills/market/badges"
import { SkillMarketFilesTree } from "@/components/skills/market/files-tree"
import { useAcpAgents } from "@/hooks/use-acp-agents"
import {
  artifactStatusBadgeInfo,
  formatSkillBytes,
  type SkillMarketV2Detail,
  type SkillMarketV2FileNode,
  type SkillMarketV2Version,
} from "@/lib/skill-market"
import { cn } from "@/lib/utils"
import { AGENT_LABELS } from "@/lib/types"

interface Props {
  detail: SkillMarketV2Detail
  activeVersion: SkillMarketV2Version
  versions: SkillMarketV2Version[]
  files: {
    value: SkillMarketV2FileNode[] | null
    loading: boolean
    error: string | null
    requested: boolean
  }
  onOpenFiles: () => void
  onSelectVersion: (version: string) => void
  onRebuildArtifact: (version: string) => void
}

function formatDate(locale: string, value: string | null): string {
  if (!value) return "-"
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return "-"
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(date)
}

function InfoRow({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex items-start justify-between gap-3 border-b py-2 text-xs last:border-b-0">
      <span className="shrink-0 text-muted-foreground">{label}</span>
      <span className="min-w-0 break-words text-right">{value}</span>
    </div>
  )
}

export function DetailInspectorTabs(props: Props) {
  const t = useTranslations("SkillMarketV2")
  const locale = useLocale()
  const { agents, fresh } = useAcpAgents()
  const targets = props.detail.installTargets.map((agentType) => ({
    agentType,
    agent: agents.find((item) => item.agent_type === agentType),
  }))
  return (
    <Tabs defaultValue="overview" className="min-h-0 flex-1 gap-0">
      <TabsList
        variant="line"
        className="h-10 w-full shrink-0 justify-start overflow-x-auto border-b bg-background px-3"
      >
        {(
          ["overview", "files", "versions", "dependencies", "targets"] as const
        ).map((tab) => (
          <TabsTrigger key={tab} value={tab} className="h-9 text-xs">
            {t(`detail.${tab}`)}
          </TabsTrigger>
        ))}
      </TabsList>
      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        <TabsContent value="overview" className="space-y-4">
          <p className="text-xs leading-5 text-muted-foreground">
            {props.detail.summary}
          </p>
          <div className="flex flex-wrap gap-1.5">
            {props.detail.tags.map((tag) => (
              <span
                key={tag}
                className="border bg-background px-1.5 py-0.5 text-[10px] text-muted-foreground"
              >
                {tag}
              </span>
            ))}
          </div>
          <div>
            <InfoRow
              label={t("detail.updated")}
              value={formatDate(locale, props.detail.updatedAt)}
            />
            <InfoRow
              label={t("detail.compatibility")}
              value={t(`compatibility.${props.detail.compatibility}`)}
            />
            <InfoRow
              label={t("detail.clientVersion")}
              value={props.detail.compatibilityDetail.minClientVersion ?? "-"}
            />
            <InfoRow
              label={t("detail.osArch")}
              value={props.detail.compatibilityDetail.osArch ?? "-"}
            />
            <InfoRow
              label={t("detail.ownership")}
              value={t(
                `detail.ownershipSourceValue.${props.detail.ownership.source}`
              )}
            />
          </div>
          <p className="flex items-center gap-1.5 text-[10px] text-muted-foreground">
            <ShieldAlert className="size-3" aria-hidden="true" />
            {t("install.profileRule")}
          </p>
        </TabsContent>

        <TabsContent value="files">
          {props.files.requested ? (
            <SkillMarketFilesTree
              files={props.files.value ?? []}
              loading={props.files.loading}
              error={props.files.error}
              onRetry={props.onOpenFiles}
            />
          ) : (
            <Button size="sm" variant="outline" onClick={props.onOpenFiles}>
              <FolderTree className="size-3.5" aria-hidden="true" />
              {t("detail.loadFiles")}
            </Button>
          )}
        </TabsContent>

        <TabsContent value="versions" className="space-y-2">
          {props.versions.map((item) => (
            <div
              key={item.id}
              className={cn(
                "flex w-full items-center gap-2 border bg-background px-3 py-2 text-left",
                item.version === props.activeVersion.version &&
                  "border-primary/50 bg-primary/5"
              )}
            >
              <button
                type="button"
                disabled={item.status !== "ready"}
                onClick={() => props.onSelectVersion(item.version)}
                className="min-w-0 flex-1 truncate text-left font-mono text-xs font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
              >
                v{item.version}
              </button>
              <MarketBadge info={artifactStatusBadgeInfo(item.status)} />
              <span className="text-[10px] text-muted-foreground">
                {formatSkillBytes(item.artifactSize)}
              </span>
              {item.status === "failed" ? (
                <Button
                  size="icon-xs"
                  variant="ghost"
                  aria-label={t("manage.rebuildArtifact")}
                  title={t("manage.rebuildArtifact")}
                  onClick={(event) => {
                    event.stopPropagation()
                    props.onRebuildArtifact(item.version)
                  }}
                >
                  <Wrench className="size-3" />
                </Button>
              ) : null}
            </div>
          ))}
        </TabsContent>

        <TabsContent value="dependencies" className="space-y-2">
          {props.activeVersion.dependencies.length ? (
            props.activeVersion.dependencies.map((dependency) => (
              <div
                key={dependency.skillId}
                className="flex items-center gap-2 border bg-background px-3 py-2"
              >
                <Building2 className="size-3.5 text-muted-foreground" />
                <span className="min-w-0 flex-1 truncate font-mono text-xs">
                  {dependency.slug}
                </span>
                <span className="font-mono text-[10px] text-muted-foreground">
                  v{dependency.version}
                </span>
              </div>
            ))
          ) : (
            <p className="text-xs text-muted-foreground">
              {t("detail.noDependencies")}
            </p>
          )}
        </TabsContent>

        <TabsContent value="targets" className="space-y-2">
          {!fresh ? (
            <p className="flex items-center gap-2 text-xs text-muted-foreground">
              <Loader2 className="size-3.5 animate-spin" aria-hidden="true" />
              {t("install.targetsLoading")}
            </p>
          ) : targets.length ? (
            <div className="divide-y border-y">
              {targets.map(({ agentType, agent }) => (
                <div
                  key={agentType}
                  className="flex min-h-12 items-center gap-3 py-2"
                >
                  <Bot className="size-4 text-muted-foreground" />
                  <span className="min-w-0 flex-1 text-xs font-medium">
                    {AGENT_LABELS[agentType]}
                  </span>
                  <span className="text-right text-[10px] text-muted-foreground">
                    {t("install.targetVersion", {
                      version: agent?.installed_version ?? "-",
                    })}
                    <br />
                    {t("install.defaultMode")}
                  </span>
                </div>
              ))}
            </div>
          ) : (
            <p className="text-xs text-muted-foreground">
              {t("install.noTargets")}
            </p>
          )}
        </TabsContent>
      </div>
    </Tabs>
  )
}
