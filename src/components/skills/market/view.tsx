"use client"

import { useCallback, useState } from "react"
import { useSkillMarket } from "@/hooks/use-skill-market"
import { useSkillMarketInstall } from "@/hooks/use-skill-market-install"
import { SkillMarketDetailDialog } from "@/components/skills/market/detail-dialog"
import { SkillMarketInstallPanel } from "@/components/skills/market/install-panel"
import { SkillMarketList } from "@/components/skills/market/list"
import { SkillMarketToolbar } from "@/components/skills/market/toolbar"
import {
  SkillMarketUploadDialog,
  type SkillMarketUploadMode,
} from "@/components/skills/market/upload-dialog"
import { ManagementDialogs } from "@/components/skills/market/management-dialogs"
import type { SkillMarketV2Detail, SkillMarketV2Item } from "@/lib/skill-market"
import type {
  SkillMarketAddVersionRequestV2,
  SkillMarketMetadataRequestV2,
  SkillMarketPublishRequestV2,
} from "@/lib/skill-market-source"

export function SkillMarketView({
  onOpenConnectors,
}: {
  onOpenConnectors: () => void
}) {
  const market = useSkillMarket()
  const install = useSkillMarketInstall()

  const [uploadOpen, setUploadOpen] = useState(false)
  const [uploadMode, setUploadMode] = useState<SkillMarketUploadMode>("publish")
  const [uploadBusy, setUploadBusy] = useState(false)
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
      setDetailOpen(false)
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
      setUploadBusy(true)
      try {
        await market.publish(request)
        setUploadOpen(false)
      } finally {
        setUploadBusy(false)
      }
    },
    [market]
  )

  const addVersion = useCallback(
    async (request: SkillMarketAddVersionRequestV2) => {
      setUploadBusy(true)
      try {
        await market.addVersion(request)
        setUploadOpen(false)
      } finally {
        setUploadBusy(false)
      }
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
      <section className="min-h-0 min-w-0 flex-1">
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
      </section>
      <SkillMarketDetailDialog
        open={detailOpen}
        onOpenChange={setDetailOpen}
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
        onPrimaryAction={handlePrimaryAction}
        onEditMetadata={(detail) => {
          setDetailOpen(false)
          setEditTarget(detail)
        }}
        onDelete={(detail) => {
          setDetailOpen(false)
          setDeleteTarget(detail)
        }}
        onUninstall={(detail) => {
          setDetailOpen(false)
          setUninstallTarget(detail)
        }}
        onRebuildArtifact={(detail, version) => {
          void market.rebuildArtifact(detail.id, version)
        }}
      />
      <SkillMarketInstallPanel
        controller={install}
        pendingTarget={pendingTarget}
        onOpenConnectors={onOpenConnectors}
        onInstalled={(skillId, version) => {
          market.applyInstalled(skillId, version)
          market.refresh()
        }}
        onClose={() => setPendingTarget(null)}
      />
      <SkillMarketUploadDialog
        open={uploadOpen}
        mode={uploadMode}
        categories={market.categories}
        targetSkillId={market.selectedId}
        busy={uploadBusy}
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
