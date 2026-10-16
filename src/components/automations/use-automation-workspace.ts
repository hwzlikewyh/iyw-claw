"use client"
import { useCallback, useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { useAutomationsView } from "@/contexts/automations-view-context"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import {
  automationCreate,
  automationMarkSeen,
  automationTemplateList,
  automationUpdate,
} from "@/lib/api"
import type { Automation, AutomationDraft } from "@/lib/types"
import {
  AUTOMATION_TEMPLATES,
  templateFromApi,
  templateToDraft,
  type AutomationTemplate,
} from "./automation-templates"
import { useConversationAutomationDraft } from "./use-conversation-automation-draft"
import { useAutomationChannels } from "./use-automation-channels"
import { useAutomationHistory } from "./use-automation-history"

type View = "tasks" | "runs" | "detail" | "gallery" | "editor"
type Editing = {
  task: Automation | null
  seed: AutomationDraft | null
  key: number
}

export function useAutomationWorkspace() {
  const t = useTranslations("Automations")
  const { automations, unseenFailures, refetch } = useAutomationsView()
  const folders = useAppWorkspaceStore((store) => store.folders)
  const { channels } = useAutomationChannels()
  const { total: pending } = useAutomationHistory({ retryable: true })
  const [view, setView] = useState<View>("tasks")
  const [selectedId, setSelectedId] = useState<number | null>(null)
  const [editing, setEditing] = useState<Editing | null>(null)
  const [templates, setTemplates] =
    useState<AutomationTemplate[]>(AUTOMATION_TEMPLATES)
  const [busy, setBusy] = useState(false)
  const [search, setSearch] = useState("")
  const [folder, setFolder] = useState("all")
  const [filter, setFilter] = useState("all")
  const [runStatus, setRunStatus] = useState("all")
  const [notification, setNotification] = useState<{
    task: Automation
    value: number | null
  } | null>(null)
  const [deleteTask, setDeleteTask] = useState<Automation | null>(null)
  const createKey = useRef(0)
  const selected = automations.find((task) => task.id === selectedId)
  const openDraft = useCallback((seed: AutomationDraft) => {
    setEditing({ task: null, seed, key: ++createKey.current })
    setView("editor")
  }, [])
  useConversationAutomationDraft(openDraft)
  useEffect(() => {
    let cancelled = false
    void automationTemplateList()
      .then((items) => {
        const next = items
          .map(templateFromApi)
          .filter((item): item is AutomationTemplate => item !== null)
        if (!cancelled && next.length) setTemplates(next)
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [])
  useEffect(() => {
    if (unseenFailures)
      void automationMarkSeen()
        .then(refetch)
        .catch(() => {})
  }, [unseenFailures, refetch])
  const perform = async (action: () => Promise<unknown>) => {
    if (busy) return
    setBusy(true)
    try {
      await action()
      await refetch()
    } catch (cause) {
      toast.error(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setBusy(false)
    }
  }
  const selectTask = (task: Automation) => {
    setSelectedId(task.id)
    setView("detail")
  }
  const editTask = (task: Automation) => {
    setEditing({ task, seed: null, key: task.id })
    setView("editor")
  }
  const pickTemplate = (template: AutomationTemplate | null) => {
    const seed = template
      ? templateToDraft(template, {
          name: t(template.titleKey),
          agentType: "claude_code",
          folderId: null,
        })
      : null
    setEditing({ task: null, seed, key: ++createKey.current })
    setView("editor")
  }
  const save = async (draft: AutomationDraft) => {
    const saved = editing?.task
      ? await automationUpdate(editing.task.id, draft)
      : await automationCreate(draft)
    await refetch()
    setSelectedId(saved.id)
    setEditing(null)
    setView("detail")
  }
  const setNotify = (task: Automation) =>
    setNotification({
      task,
      value: task.config?.notification_channel_id ?? null,
    })
  const saveNotify = () =>
    void perform(async () => {
      if (!notification) return
      const current = automations.find(
        (task) => task.id === notification.task.id
      )
      if (!current?.config) throw new Error(t("taskRemoved"))
      await automationUpdate(current.id, {
        ...current,
        config: {
          ...current.config,
          notification_channel_id: notification.value,
        },
      })
      setNotification(null)
      toast.success(t("notificationSaved"))
    })
  const folderName = (task: Automation) =>
    task.config?.label_snapshot?.folder_label ||
    folders.find((item) => item.id === task.root_folder_id)?.name ||
    t("folderDefault")
  const visible = automations.filter(
    (task) =>
      (folder === "all" || task.root_folder_id === Number(folder)) &&
      (filter === "all" || task.enabled === (filter === "enabled")) &&
      task.name.toLocaleLowerCase().includes(search.toLocaleLowerCase())
  )
  const back = () => setView(selected ? "detail" : "tasks")
  return {
    t,
    automations,
    folders,
    channels,
    view,
    setView,
    editing,
    templates,
    busy,
    search,
    setSearch,
    folder,
    setFolder,
    filter,
    setFilter,
    runStatus,
    setRunStatus,
    notification,
    setNotification,
    deleteTask,
    setDeleteTask,
    selected,
    perform,
    selectTask,
    editTask,
    pickTemplate,
    save,
    setNotify,
    saveNotify,
    folderName,
    visible,
    pending,
    back,
    refetch,
  }
}

export type AutomationWorkspace = ReturnType<typeof useAutomationWorkspace>
