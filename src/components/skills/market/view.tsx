"use client"

import { useCallback, useState } from "react"
import { useSkillMarket } from "@/hooks/use-skill-market"
import { useSkillMarketInstall } from "@/hooks/use-skill-market-install"
import { SkillMarketDetail } from "@/components/skills/market/detail"
import { SkillMarketInstallPanel } from "@/components/skills/market/install-panel"
import { SkillMarketList } from "@/components/skills/market/list"
import { SkillMarketToolbar } from "@/components/skills/market/toolbar"
import {
  SkillMarketUploadDialog,
  type SkillMarketUploadMode,
} from "@/components/skills/market/upload-dialog"
import { ManagementDialogs } from "@/components/skills/market/management-dialogs"
import type {
  SkillMarketV2Detail,
  SkillMarketV2Item,
} from "@/lib/skill-market"
import type {
  SkillMarketAddVersionRequestV2,
  SkillMarketMetadataRequestV2,
  SkillMarketPublishRequestV2,
} from "@/lib/skill-market-source"
import { cn } from "@/lib/utils"

export function SkillMarketView() {
  const market = useSkillMarket()
  const install = useSkillMarketInstall()

  const [uploadOpen, setUploadOpen] = useState(false)
  const [uploadMode, setUploadMode] = useState<SkillMarketUploadMode>("publish")
  const [detailOpen, setDetailOpen] = useState(false)
  const [pendingTarget, setPendingTarget] = useState<{
    name: string
    version: string
  } | null>(null)
  const [editTarget, setEditTarget] = useState<SkillMarketV2Detail | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<SkillMarketV2Detail | null>(
    null
  )
  const [uninstallTarget, setUninstallTarget] =
    useState<SkillMarketV2Detail | null>(null)
  const [manageBusy, setManageBusy] = useState(false)

  const handlePrimaryAction = useCallback(
    (item: SkillMarketV2Item, version: string) => {
      setPendingTarget({ name: item.displayName, version })
      void install.beginResolve(item, version)
    },
    [install]
  )

  const openUpload = (mode: SkillMarketUploadMode) => {
    setUploadMode(mode)
    setUploadOpen(true)
  }

  const publish = useCallback(
    async (request: SkillMarketPublishRequestV2) => {
      await market.publish(request)
    },
    [market]
  )

  const addVersion = useCallback(
    async (request: SkillMarketAddVersionRequestV2) => {
      await market.addVersion(request)
    },
    [market]
  )

  const saveMetadata = useCallback(
    async (request: SkillMarketMetadataRequestV2) => {
      setManageBusy(true)
      try {
        await market.updateMetadata(request)
      } finally {
        setManageBusy(false)
      }
    },
    [market]
  )

  const confirmDelete = useCallback(
    async (id: string) => {
      setManageBusy(true)
      try {
        await market.deleteSkill(id)
      } finally {
        setManageBusy(false)
      }
    },
    [market]
  )

  const confirmUninstall = useCallback(
    async (id: string) => {
      setManageBusy(true)
      try {
        await market.uninstallSkill(id)
      } finally {
        setManageBusy(false)
      }
    },
    [market]
  )

  return (
    <div className="flex h-full min-h-0 flex-col bg-background">
      <SkillMarketToolbar
        query={market.query}
        categories={market.categories}
        revision={market.list.revision}
        offline={market.list.offline}
        loading={market.list.loading}
        onQueryChange={market.updateQuery}
        onRefresh={market.refresh}
        onUpload={() => openUpload("publish")}
      />
      <div className="flex min-h-0 flex-1">
        <aside
          className={cn(
            "min-h-0 w-full border-r lg:w-80 lg:shrink-0",
            detailOpen && "hidden lg:block"
          )}
        >
          <SkillMarketList
            items={market.list.items}
            selectedId={market.selectedId}
            loading={market.list.loading}
            error={market.list.error}
            total={market.list.total}
            nextCursor={market.list.nextCursor}
            onSelect={(item) => {
              market.selectItem(item)
              setDetailOpen(true)
            }}
            onPrimaryAction={(item) =>
              handlePrimaryAction(item, item.currentVersion.version)
            }
            onLoadMore={market.loadMore}
            onRetry={market.refresh}
          />
        </aside>
        <main
          className={cn(
            "min-h-0 flex-1",
            !detailOpen && "hidden lg:block"
          )}
        >
          <SkillMarketDetail
            detail={market.detail.value}
            versions={market.versions.value}
            versionsLoading={market.versions.loading}
            selectedVersion={market.selectedVersion}
            loading={market.detail.loading}
            error={market.detail.error}
            files={market.files}
            onSelectVersion={market.selectVersion}
            onOpenFiles={market.openFiles}
            onRetry={market.retryDetail}
            onBack={() => setDetailOpen(false)}
            onPrimaryAction={handlePrimaryAction}
            onEditMetadata={setEditTarget}
            onDelete={setDeleteTarget}
            onUninstall={setUninstallTarget}
            onRebuildArtifact={(detail) => {
              void market.rebuildArtifact(
                detail.id,
                detail.currentVersion.version
              )
            }}
          />
        </main>
      </div>
      <SkillMarketInstallPanel
        controller={install}
        pendingTarget={pendingTarget}
        onInstalled={market.applyInstalled}
        onClose={() => setPendingTarget(null)}
      />
      <SkillMarketUploadDialog
        open={uploadOpen}
        mode={uploadMode}
        categories={market.categories}
        targetSkillId={market.selectedId}
        busy={false}
        onOpenChange={setUploadOpen}
        onPublish={publish}
        onAddVersion={addVersion}
      />
      <ManagementDialogs
        editTarget={editTarget}
        deleteTarget={deleteTarget}
        uninstallTarget={uninstallTarget}
        categories={market.categories}
        busy={manageBusy}
        onEditClose={() => setEditTarget(null)}
        onDeleteClose={() => setDeleteTarget(null)}
        onUninstallClose={() => setUninstallTarget(null)}
        onSaveMetadata={saveMetadata}
        onConfirmDelete={confirmDelete}
        onConfirmUninstall={confirmUninstall}
      />
    </div>
  )
}
