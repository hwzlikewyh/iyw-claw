"use client"

import { SkillMarketDetail } from "@/components/skills/skill-market-detail"
import {
  LocalSkillDetail,
  SkillMarketInstalledList,
} from "@/components/skills/skill-market-installed"
import { SkillMarketList } from "@/components/skills/skill-market-list"
import {
  SkillMarketFilters,
  SkillMarketHeader,
  type SkillMarketSection,
} from "@/components/skills/skill-market-toolbar"
import type { useSkillMarketActions } from "@/components/skills/skill-market-use-actions"
import type { useSkillMarketData } from "@/components/skills/skill-market-use-data"
import type { WorkspaceDialogState } from "@/components/skills/skill-market-workspace-dialogs"
import type { AgentSkillItem } from "@/lib/types"

type MarketData = ReturnType<typeof useSkillMarketData>
type MarketActions = ReturnType<typeof useSkillMarketActions>

type ContentProps = {
  section: SkillMarketSection
  dialogs: WorkspaceDialogState
  data: MarketData
  actions: MarketActions
  installedSkills: AgentSkillItem[]
  installedLoading: boolean
  installedVersion: string | null
  installDisabled: boolean
  localBusyId: string | null
  activeLocal: AgentSkillItem | null
  selectedLocalMarketId: string | null
  onSectionChange: (section: SkillMarketSection) => void
  onSelectLocal: (skill: AgentSkillItem) => void
  onOpenFolder: (skill: AgentSkillItem) => Promise<void>
  onToggleLocal: (skill: AgentSkillItem, enabled: boolean) => Promise<void>
}

function MarketFilters(props: ContentProps) {
  if (props.section === "installed") return null
  return (
    <SkillMarketFilters
      section={props.section}
      categories={props.data.categories}
      query={props.data.query}
      category={props.data.category}
      publisher={props.data.publisher}
      visibility={props.data.visibility}
      onQueryChange={props.data.setQuery}
      onCategoryChange={props.data.setCategory}
      onPublisherChange={props.data.setPublisher}
      onVisibilityChange={props.data.setVisibility}
    />
  )
}

function WorkspaceList(props: ContentProps) {
  if (props.section === "installed") {
    return (
      <SkillMarketInstalledList
        skills={props.installedSkills}
        remoteById={props.data.remoteById}
        selectedId={props.activeLocal?.id ?? null}
        loading={props.installedLoading}
        onSelect={props.onSelectLocal}
      />
    )
  }
  return (
    <SkillMarketList
      items={props.data.items}
      selectedId={props.data.selectedId}
      loading={props.data.listLoading}
      error={props.data.listError}
      page={props.data.page}
      pageSize={props.data.pageSize}
      total={props.data.total}
      onSelect={props.data.selectItem}
      onRetry={props.data.refresh}
      onPageChange={props.data.setPage}
    />
  )
}

function RemoteDetail(props: ContentProps) {
  const canManage = props.section !== "installed"
  return (
    <SkillMarketDetail
      detail={props.data.detail}
      versions={props.data.versions}
      installedVersion={props.installedVersion}
      loading={props.data.detailLoading}
      detailError={props.data.detailError}
      versionsLoading={props.data.versionsLoading}
      versionsError={props.data.versionsError}
      busy={props.actions.busyAction === "install"}
      installDisabled={props.installDisabled}
      onVersionChange={props.data.setSelectedVersion}
      onRetryDetail={props.data.retryDetail}
      onRetryVersions={props.data.retryVersions}
      onInstall={(version) =>
        void props.actions.install(version).catch(() => {})
      }
      onEdit={canManage ? () => props.dialogs.setMetadataOpen(true) : undefined}
      onAddVersion={
        canManage ? () => props.dialogs.setVersionOpen(true) : undefined
      }
      onDelete={canManage ? () => props.dialogs.setDeleteOpen(true) : undefined}
      onUninstall={
        !canManage && props.activeLocal
          ? () => props.dialogs.setUninstallOpen(true)
          : undefined
      }
    />
  )
}

function WorkspaceDetail(props: ContentProps) {
  if (props.section !== "installed" || props.selectedLocalMarketId) {
    return <RemoteDetail {...props} />
  }
  return (
    <LocalSkillDetail
      skill={props.activeLocal}
      toggling={props.localBusyId === props.activeLocal?.id}
      deleting={props.localBusyId === props.activeLocal?.id}
      onOpenFolder={() => {
        if (props.activeLocal) {
          void props.onOpenFolder(props.activeLocal).catch(() => {})
        }
      }}
      onToggle={(enabled) => {
        if (props.activeLocal) {
          void props.onToggleLocal(props.activeLocal, enabled).catch(() => {})
        }
      }}
      onDelete={() => {
        if (props.activeLocal) props.dialogs.setUninstallOpen(true)
      }}
    />
  )
}

export function SkillMarketWorkspaceContent(props: ContentProps) {
  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      <SkillMarketHeader
        section={props.section}
        onSectionChange={props.onSectionChange}
        onTool={props.dialogs.setTool}
      />
      <main className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-7xl p-4">
          <MarketFilters {...props} />
          <div className="mt-4 grid min-w-0 gap-4 md:grid-cols-[minmax(0,1fr)_24rem]">
            <section className="min-w-0">
              <WorkspaceList {...props} />
            </section>
            <WorkspaceDetail {...props} />
          </div>
        </div>
      </main>
    </div>
  )
}
