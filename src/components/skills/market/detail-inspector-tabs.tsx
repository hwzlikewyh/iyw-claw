"use client"

import { Bot, Building2, FolderTree, Loader2, Wrench } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { MarketBadge } from "@/components/skills/market/badges"
import { DetailOverview } from "@/components/skills/market/detail-overview"
import { SkillMarketFilesTree } from "@/components/skills/market/files-tree"
import { PluginComponents } from "@/components/skills/market/plugin-components"
import { useAcpAgents } from "@/hooks/use-acp-agents"
import {
  artifactStatusBadgeInfo,
  formatSkillBytes,
  type SkillMarketTranslator,
  type SkillMarketV2Detail,
  type SkillMarketV2FileNode,
  type SkillMarketV2Version,
} from "@/lib/skill-market"
import { AGENT_LABELS } from "@/lib/types"
import { cn } from "@/lib/utils"

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

function TabLabel({ label, count }: { label: string; count?: number }) {
  return (
    <span className="flex w-full items-center justify-between gap-3">
      <span>{label}</span>
      {count !== undefined ? (
        <span className="rounded-full bg-background px-1.5 py-0.5 text-[9px] text-muted-foreground">
          {count}
        </span>
      ) : null}
    </span>
  )
}

function VersionList({
  versions,
  activeVersion,
  onSelect,
  onRebuild,
}: {
  versions: SkillMarketV2Version[]
  activeVersion: SkillMarketV2Version
  onSelect: (version: string) => void
  onRebuild: (version: string) => void
}) {
  const t = useTranslations("SkillMarketV2")
  return (
    <div className="space-y-2">
      {versions.map((item) => (
        <div
          key={item.id}
          className={cn(
            "flex items-center gap-2 border bg-background px-3 py-2",
            item.version === activeVersion.version &&
              "border-primary/40 bg-primary/5"
          )}
        >
          <button
            type="button"
            disabled={item.status !== "ready"}
            className="min-w-0 flex-1 truncate text-left font-mono text-xs font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
            onClick={() => onSelect(item.version)}
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
              onClick={() => onRebuild(item.version)}
            >
              <Wrench className="size-3" />
            </Button>
          ) : null}
        </div>
      ))}
    </div>
  )
}

function Dependencies({ version }: { version: SkillMarketV2Version }) {
  const t = useTranslations("SkillMarketV2")
  if (!version.dependencies.length) {
    return (
      <p className="text-xs text-muted-foreground">
        {t("detail.noDependencies")}
      </p>
    )
  }
  return (
    <div className="space-y-2">
      {version.dependencies.map((dependency) => (
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
      ))}
    </div>
  )
}

function InstallTargets({ detail }: { detail: SkillMarketV2Detail }) {
  const t = useTranslations("SkillMarketV2")
  const { agents, fresh } = useAcpAgents()
  if (!fresh) {
    return (
      <p className="flex items-center gap-2 text-xs text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin" />
        {t("install.targetsLoading")}
      </p>
    )
  }
  if (!detail.installTargets.length) {
    return (
      <p className="text-xs text-muted-foreground">{t("install.noTargets")}</p>
    )
  }
  return (
    <div className="divide-y border-y">
      {detail.installTargets.map((agentType) => {
        const agent = agents.find((item) => item.agent_type === agentType)
        return (
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
            </span>
          </div>
        )
      })}
    </div>
  )
}

export function DetailInspectorTabs(props: Props) {
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
  const tabs = [
    { id: "overview", count: undefined },
    { id: "versions", count: props.versions.length },
    { id: "files", count: props.activeVersion.fileCount },
    { id: "dependencies", count: props.activeVersion.dependencies.length },
    ...(props.activeVersion.packageType === "plugin"
      ? [
          {
            id: "components",
            count: props.activeVersion.plugin?.components.length ?? 0,
          },
        ]
      : []),
    { id: "targets", count: props.detail.installTargets.length },
  ]
  return (
    <Tabs
      defaultValue="overview"
      orientation="vertical"
      className="min-h-0 min-w-0 flex-1 flex-col gap-0 md:grid md:grid-cols-[11rem_minmax(0,1fr)]"
      onValueChange={(value) => {
        if (value === "files" && !props.files.requested) props.onOpenFiles()
      }}
    >
      <TabsList
        variant="line"
        className="!h-11 !w-full shrink-0 !flex-row justify-start overflow-x-auto border-b bg-muted/10 px-3 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden md:!h-full md:!w-44 md:!flex-col md:items-stretch md:justify-start md:overflow-visible md:border-r md:border-b-0 md:p-3"
      >
        {tabs.map((tab) => (
          <TabsTrigger
            key={tab.id}
            value={tab.id}
            className="!h-9 !w-auto flex-none rounded-md px-3 text-xs md:!w-full"
          >
            <TabLabel label={t(`detail.${tab.id}`)} count={tab.count} />
          </TabsTrigger>
        ))}
      </TabsList>
      <div className="min-h-0 min-w-0 flex-1 overflow-y-auto p-5 sm:p-6">
        <TabsContent value="overview">
          <DetailOverview detail={props.detail} version={props.activeVersion} />
        </TabsContent>
        <TabsContent value="versions">
          <VersionList
            versions={props.versions}
            activeVersion={props.activeVersion}
            onSelect={props.onSelectVersion}
            onRebuild={props.onRebuildArtifact}
          />
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
              <FolderTree className="size-3.5" />
              {t("detail.loadFiles")}
            </Button>
          )}
        </TabsContent>
        <TabsContent value="dependencies">
          <Dependencies version={props.activeVersion} />
        </TabsContent>
        {props.activeVersion.packageType === "plugin" ? (
          <TabsContent value="components">
            <PluginComponents plugin={props.activeVersion.plugin} />
          </TabsContent>
        ) : null}
        <TabsContent value="targets">
          <InstallTargets detail={props.detail} />
        </TabsContent>
      </div>
    </Tabs>
  )
}
