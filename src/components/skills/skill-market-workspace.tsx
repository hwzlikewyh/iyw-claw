"use client"

import { useMemo, useState } from "react"
import type { SkillContentRequest } from "@/components/settings/skill-market-panels"
import { useSkillMarketActions } from "@/components/skills/skill-market-use-actions"
import { useSkillMarketData } from "@/components/skills/skill-market-use-data"
import { SkillMarketWorkspaceContent } from "@/components/skills/skill-market-workspace-content"
import {
  SkillMarketWorkspaceDialogs,
  type WorkspaceDialogState,
  type WorkspaceTool,
} from "@/components/skills/skill-market-workspace-dialogs"
import type { SkillMarketSection } from "@/components/skills/skill-market-toolbar"
import { getInstalledMarketInfo } from "@/lib/skill-market"
import type { AgentSkillItem, AgentType } from "@/lib/types"

type LocalAction = (skill: AgentSkillItem) => Promise<void>

export interface SkillMarketWorkspaceProps {
  installedSkills: AgentSkillItem[]
  installedLoading: boolean
  targetName: string | null
  agentType: AgentType | null
  installDisabled: boolean
  localBusyId: string | null
  onInstalledChanged: () => Promise<void>
  onImport: (request: SkillContentRequest) => Promise<void>
  onGenerate: (request: SkillContentRequest) => Promise<void>
  onOpenFolder: LocalAction
  onDeleteLocal: LocalAction
  onToggleLocal: (skill: AgentSkillItem, enabled: boolean) => Promise<void>
}

function useWorkspaceView() {
  const [section, setSection] = useState<SkillMarketSection>("market")
  const [tool, setTool] = useState<WorkspaceTool>(null)
  const [selectedLocal, setSelectedLocal] = useState<AgentSkillItem | null>(
    null
  )
  const [metadataOpen, setMetadataOpen] = useState(false)
  const [versionOpen, setVersionOpen] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [uninstallOpen, setUninstallOpen] = useState(false)
  const dialogs: WorkspaceDialogState = {
    tool,
    metadataOpen,
    versionOpen,
    deleteOpen,
    uninstallOpen,
    setTool,
    setMetadataOpen,
    setVersionOpen,
    setDeleteOpen,
    setUninstallOpen,
  }
  return { section, setSection, selectedLocal, setSelectedLocal, dialogs }
}

function findActiveLocal(
  selected: AgentSkillItem | null,
  installed: AgentSkillItem[]
) {
  if (!selected) return null
  return (
    installed.find(
      (skill) => skill.id === selected.id && skill.scope === selected.scope
    ) ?? null
  )
}

function useInstalledVersion(
  marketId: string | undefined,
  installed: AgentSkillItem[]
) {
  return useMemo(() => {
    if (!marketId) return null
    const local = installed.find(
      (skill) => getInstalledMarketInfo(skill).marketId === marketId
    )
    return local ? getInstalledMarketInfo(local).version : null
  }, [installed, marketId])
}

function useWorkspaceModel(props: SkillMarketWorkspaceProps) {
  const view = useWorkspaceView()
  const data = useSkillMarketData(view.section, props.installedSkills)
  const activeLocal = findActiveLocal(view.selectedLocal, props.installedSkills)
  const localMarketId = activeLocal
    ? getInstalledMarketInfo(activeLocal).marketId
    : null
  const installedVersion = useInstalledVersion(
    data.detail?.id,
    props.installedSkills
  )
  const actions = useSkillMarketActions({
    detail: data.detail,
    agentType: props.agentType,
    installedVersion,
    onInstalledChanged: props.onInstalledChanged,
    onRefresh: data.refresh,
  })
  return { view, data, activeLocal, localMarketId, installedVersion, actions }
}

type WorkspaceModel = ReturnType<typeof useWorkspaceModel>

function createWorkspaceHandlers(model: WorkspaceModel) {
  const selectLocal = (skill: AgentSkillItem) => {
    model.view.setSelectedLocal(skill)
    model.data.setSelectedId(getInstalledMarketInfo(skill).marketId)
    model.data.setSelectedVersion(null)
  }
  const deleteRemote = async () => {
    await model.actions.deleteRemote()
    model.view.dialogs.setDeleteOpen(false)
    model.data.setSelectedId(null)
  }
  const clearUninstalled = () => {
    model.view.setSelectedLocal(null)
    model.data.setSelectedId(null)
  }
  return { selectLocal, deleteRemote, clearUninstalled }
}

function WorkspaceLayout({
  props,
  model,
}: {
  props: SkillMarketWorkspaceProps
  model: WorkspaceModel
}) {
  const handlers = createWorkspaceHandlers(model)
  const shared = {
    data: model.data,
    actions: model.actions,
    activeLocal: model.activeLocal,
    installedVersion: model.installedVersion,
    installDisabled: props.installDisabled,
    localBusyId: props.localBusyId,
    dialogs: model.view.dialogs,
  }
  return (
    <>
      <SkillMarketWorkspaceContent
        {...shared}
        section={model.view.section}
        installedSkills={props.installedSkills}
        installedLoading={props.installedLoading}
        selectedLocalMarketId={model.localMarketId}
        onSectionChange={model.view.setSection}
        onSelectLocal={handlers.selectLocal}
        onOpenFolder={props.onOpenFolder}
        onToggleLocal={props.onToggleLocal}
      />
      <SkillMarketWorkspaceDialogs
        {...shared}
        targetName={props.targetName}
        onDeleteRemote={handlers.deleteRemote}
        onDeleteLocal={props.onDeleteLocal}
        onUninstalled={handlers.clearUninstalled}
        onImport={props.onImport}
        onGenerate={props.onGenerate}
      />
    </>
  )
}

export function SkillMarketWorkspace(props: SkillMarketWorkspaceProps) {
  const model = useWorkspaceModel(props)
  return <WorkspaceLayout props={props} model={model} />
}
