"use client"

import { useState } from "react"
import { FolderOpen, FolderPlus, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { DirectoryBrowserDialog } from "@/components/shared/directory-browser-dialog"
import { NewFolderDropdown } from "./new-folder-dropdown"
import { useNewProject } from "./use-new-project"
import { useIsMobile } from "@/hooks/use-mobile"
import { useSidebarContext } from "@/contexts/sidebar-context"
import { useTabActions } from "@/contexts/tab-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"

export function SidebarProjectActions({
  compact = false,
}: {
  compact?: boolean
}) {
  const t = useTranslations("SidebarDesign")
  const [open, setOpen] = useState(false)
  const mobile = useIsMobile()
  const { toggle } = useSidebarContext()
  const { openConversations } = useWorkbenchRoute()
  const { openNewConversationTab } = useTabActions()
  const finish = () => {
    setOpen(false)
    if (mobile) toggle()
  }
  return (
    <>
      <div
        className={
          compact ? "flex flex-col gap-1" : "grid min-w-0 grid-cols-2 gap-1.5"
        }
      >
        <Button
          variant="outline"
          size="sm"
          title={t("newProject")}
          aria-label={t("newProject")}
          onClick={() => setOpen(true)}
          className={
            compact ? "size-9 p-0" : "h-8 min-w-0 gap-1 px-1 text-[0.6875rem]"
          }
        >
          <FolderPlus className="size-3.5 shrink-0" />
          {!compact && <span className="truncate">{t("newProject")}</span>}
        </Button>
        <NewFolderDropdown
          showLabel={!compact}
          label={t("chooseFolder")}
          onOpened={(folder) => {
            openConversations()
            openNewConversationTab(folder.id, folder.path)
            if (mobile) toggle()
          }}
          buttonClassName={
            compact
              ? "size-9 p-0"
              : "h-8 min-w-0 justify-center gap-1 rounded-md border px-1 text-[0.6875rem]"
          }
        />
      </div>
      {open && (
        <NewProjectDialog onClose={() => setOpen(false)} onCreated={finish} />
      )}
    </>
  )
}

function NewProjectDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void
  onCreated: () => void
}) {
  const t = useTranslations("SidebarDesign")
  const common = useTranslations("Folder.common")
  const form = useNewProject(onCreated)
  const [browsing, setBrowsing] = useState(false)
  return (
    <>
      <Dialog
        open
        onOpenChange={(open) => {
          if (!open && !form.pending && !browsing) onClose()
        }}
      >
        <DialogContent
          className="max-w-md rounded-lg"
          onInteractOutside={(event) => {
            if (browsing || form.pending) event.preventDefault()
          }}
        >
          <DialogHeader>
            <DialogTitle>{t("newProject")}</DialogTitle>
            <DialogDescription>{t("projectHint")}</DialogDescription>
          </DialogHeader>
          <form
            onSubmit={(event) => {
              event.preventDefault()
              void form.create()
            }}
            className="grid min-w-0 gap-4"
          >
            <label className="grid gap-2 text-sm">
              {t("projectName")}
              <Input
                value={form.name}
                onChange={(event) => form.setName(event.target.value)}
                disabled={form.pending}
                autoFocus
                required
              />
            </label>
            <div className="grid gap-2 text-sm">
              <span>{t("parentDirectory")}</span>
              <Button
                type="button"
                variant="outline"
                disabled={form.pending}
                onClick={() => setBrowsing(true)}
                className="min-w-0 justify-start"
              >
                <FolderOpen className="size-4 shrink-0" />
                <span className="truncate">
                  {form.parent || t("chooseFolder")}
                </span>
              </Button>
            </div>
            {form.error && (
              <p role="alert" className="break-words text-sm text-destructive">
                {form.error}
              </p>
            )}
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                disabled={form.pending}
                onClick={onClose}
              >
                {common("cancel")}
              </Button>
              <Button type="submit" disabled={form.pending}>
                {form.pending && <Loader2 className="size-4 animate-spin" />}
                {t(form.pending ? "creating" : "create")}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
      <DirectoryBrowserDialog
        open={browsing}
        onOpenChange={setBrowsing}
        initialPath={form.parent || undefined}
        onSelect={(path) => {
          form.setParent(path)
          setBrowsing(false)
        }}
      />
    </>
  )
}
