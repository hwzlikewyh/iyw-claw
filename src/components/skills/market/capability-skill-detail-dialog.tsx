"use client"

import { useState } from "react"
import { useSkillMarketActivation } from "@/hooks/use-skill-market-activation"
import { useSkillMarketInstall } from "@/hooks/use-skill-market-install"
import { SkillMarketInstallPanel } from "@/components/skills/market/install-panel"
import { SkillMarketDetailDialog } from "@/components/skills/market/detail-dialog"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"
import { notifySkillMarketInstallationsChanged } from "@/lib/skill-market-installation-events"
import type { SkillMarketV2Detail, SkillMarketV2Item } from "@/lib/skill-market"
import {
  EMPTY_FILES,
  itemAsDetail,
  markInstalled,
  useDetailData,
} from "./capability-skill-detail-data"

function useInstallWorkflow(
  item: SkillMarketV2Item | null,
  setDetailState: ReturnType<typeof useDetailData>["setState"],
  onClose: () => void,
  onInstalled: () => void
) {
  const controller = useSkillMarketInstall()
  const [target, setTarget] = useState<{
    name: string
    version: string
  } | null>(null)
  const begin = (detail: SkillMarketV2Detail, version: string) => {
    setTarget({ name: detail.displayName, version })
    onClose()
    void controller.beginResolve(detail, version)
  }
  const installed = (skillId: string, version: string) => {
    setDetailState((current) => ({
      ...current,
      detail:
        current.detail?.id === skillId
          ? markInstalled(current.detail, version)
          : item?.id === skillId
            ? markInstalled(itemAsDetail(item), version)
            : current.detail,
    }))
    notifySkillMarketInstallationsChanged()
    onInstalled()
  }
  return {
    controller,
    target,
    begin,
    installed,
    close: () => setTarget(null),
  }
}

export function CapabilitySkillDetailDialog({
  item,
  open,
  onClose,
  onInstalled,
}: {
  item: SkillMarketV2Item | null
  open: boolean
  onClose: () => void
  onInstalled: () => void
}) {
  const route = useWorkbenchRoute()
  const data = useDetailData(item)
  const activation = useSkillMarketActivation({
    view: "all",
    detailOpen: open,
    detail: data.detail,
  })
  const workflow = useInstallWorkflow(item, data.setState, onClose, onInstalled)
  return (
    <>
      <MarketDetailDialogView
        item={item}
        open={open}
        onClose={onClose}
        data={data}
        activation={activation}
        onPrimaryAction={workflow.begin}
      />
      <SkillMarketInstallPanel
        controller={workflow.controller}
        pendingTarget={workflow.target}
        onInstalled={workflow.installed}
        onOpenConnectors={route.openConnectorMarket}
        onClose={workflow.close}
      />
    </>
  )
}

function useDetailNavigation(
  item: SkillMarketV2Item | null,
  onClose: () => void,
  activation: ReturnType<typeof useSkillMarketActivation>
) {
  const route = useWorkbenchRoute()
  const retryActivation = () => void activation.inventory.refresh()
  return {
    retryActivation,
    openMarket: () => {
      if (!item) return
      onClose()
      route.openSkillMarket(item.slug, item.packageType)
    },
    openConnectors: () => {
      onClose()
      route.openConnectorMarket()
    },
  }
}

function MarketDetailDialogView({
  item,
  open,
  onClose,
  data,
  activation,
  onPrimaryAction,
}: {
  item: SkillMarketV2Item | null
  open: boolean
  onClose: () => void
  data: ReturnType<typeof useDetailData>
  activation: ReturnType<typeof useSkillMarketActivation>
  onPrimaryAction: (detail: SkillMarketV2Detail, version: string) => void
}) {
  const navigation = useDetailNavigation(item, onClose, activation)
  return (
    <SkillMarketDetailDialog
      open={open}
      onOpenChange={(value) => !value && onClose()}
      detail={data.detail}
      versions={data.current ? data.state.versions : []}
      versionsLoading={!data.current || data.state.versionsLoading}
      selectedVersion={data.current ? data.state.selectedVersion : null}
      loading={data.current && data.state.loading && !data.detail}
      error={data.current ? data.state.error : null}
      files={data.current ? data.state.files : EMPTY_FILES}
      activation={activation.activation}
      activationBusy={activation.activationBusy}
      activationError={activation.activationError}
      onSelectVersion={data.selectVersion}
      onOpenFiles={data.openFiles}
      onRetry={data.retry}
      onPrimaryAction={onPrimaryAction}
      onToggleActivation={(enabled) => {
        void activation.setEnabled(enabled).finally(data.retry)
      }}
      onOpenInventory={navigation.openMarket}
      onOpenConnectors={navigation.openConnectors}
      onRetryActivation={navigation.retryActivation}
      onEditMetadata={navigation.openMarket}
      onDelete={navigation.openMarket}
      onUninstall={navigation.openMarket}
      onRebuildArtifact={async (detail, version) => {
        await data.source.rebuildArtifact(detail.id, version)
        data.retry()
      }}
    />
  )
}
