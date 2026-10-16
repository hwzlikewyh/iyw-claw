"use client"
import { ArrowLeft } from "lucide-react"
import {
  automationDelete,
  automationRunNow,
  automationSetEnabled,
} from "@/lib/api"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { AutomationEditor } from "./automation-editor"
import { TemplateGallery } from "./template-gallery"
import { NotificationChannelPicker } from "./notification-channel-picker"
import { AutomationTaskDetail } from "./automation-task-detail"
import { AutomationHome } from "./automation-home"
import { useAutomationWorkspace } from "./use-automation-workspace"
import styles from "./automation-workspace.module.css"

export function AutomationsPage() {
  const workspace = useAutomationWorkspace()
  const {
    t,
    automations,
    view,
    setView,
    editing,
    templates,
    busy,
    notification,
    setNotification,
    deleteTask,
    setDeleteTask,
    selected,
    perform,
    editTask,
    pickTemplate,
    save,
    setNotify,
    saveNotify,
    folderName,
    back,
    refetch,
  } = workspace
  return (
    <div className={styles.workspace}>
      <div className={styles.scroll}>
        <div className={styles.content}>
          <AutomationHome workspace={workspace} />
          {view === "detail" &&
            (selected ? (
              <AutomationTaskDetail
                task={selected}
                automations={automations}
                folder={folderName(selected)}
                busy={busy}
                onBack={() => setView("tasks")}
                onEdit={() => editTask(selected)}
                onRun={() => void perform(() => automationRunNow(selected.id))}
                onToggle={() =>
                  void perform(() =>
                    automationSetEnabled(selected.id, !selected.enabled)
                  )
                }
                onNotify={() => setNotify(selected)}
                onDelete={() => setDeleteTask(selected)}
                onChanged={refetch}
              />
            ) : (
              <div className={styles.empty}>
                {t("taskRemoved")}
                <Button variant="link" onClick={() => setView("tasks")}>
                  {t("returnTasks")}
                </Button>
              </div>
            ))}
          {view === "gallery" && (
            <div className="mx-auto max-w-5xl">
              <Button className="mb-6" variant="ghost" size="sm" onClick={back}>
                <ArrowLeft className="size-3.5" />
                {t("returnList")}
              </Button>
              <h1 className="mb-2 text-xl font-semibold">{t("new")}</h1>
              <p className="mb-7 text-xs text-muted-foreground">
                {t("firstRunHint")}
              </p>
              <TemplateGallery templates={templates} onPick={pickTemplate} />
            </div>
          )}
          {view === "editor" && editing && (
            <div className="mx-auto max-w-5xl">
              <Button className="mb-5" variant="ghost" size="sm" onClick={back}>
                <ArrowLeft className="size-3.5" />
                {t("returnList")}
              </Button>
              <AutomationEditor
                key={
                  editing.task
                    ? `edit-${editing.task.id}`
                    : `create-${editing.key}`
                }
                automation={editing.task || editing.seed}
                onSubmit={save}
                onCancel={back}
                onBackToTemplates={
                  editing.task ? undefined : () => setView("gallery")
                }
              />
            </div>
          )}
        </div>
      </div>
      {notification && (
        <Dialog
          open
          onOpenChange={(open) => {
            if (!open && !busy) setNotification(null)
          }}
        >
          <DialogContent>
            <DialogHeader>
              <DialogTitle>{t("notificationTitle")}</DialogTitle>
              <DialogDescription>{notification.task.name}</DialogDescription>
            </DialogHeader>
            <NotificationChannelPicker
              value={notification.value}
              onChange={(value) => setNotification({ ...notification, value })}
            />
            <DialogFooter>
              <Button
                variant="ghost"
                disabled={busy}
                onClick={() => setNotification(null)}
              >
                {t("cancel")}
              </Button>
              <Button disabled={busy} onClick={saveNotify}>
                {t("save")}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      )}
      {deleteTask && (
        <Dialog
          open
          onOpenChange={(open) => {
            if (!open && !busy) setDeleteTask(null)
          }}
        >
          <DialogContent>
            <DialogHeader>
              <DialogTitle>{t("deleteTitle")}</DialogTitle>
              <DialogDescription>{t("deleteDescription")}</DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button
                variant="ghost"
                disabled={busy}
                onClick={() => setDeleteTask(null)}
              >
                {t("cancel")}
              </Button>
              <Button
                variant="destructive"
                disabled={busy}
                onClick={() =>
                  void perform(async () => {
                    await automationDelete(deleteTask.id)
                    setDeleteTask(null)
                    setView("tasks")
                  })
                }
              >
                {t("delete")}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      )}
    </div>
  )
}
