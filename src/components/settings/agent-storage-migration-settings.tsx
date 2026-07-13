"use client"

import { useState } from "react"
import { FolderOpen, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { migrateAgentStorage, validateAgentStorageRoot } from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import { openFileDialog } from "@/lib/platform"
import type { AgentStorageStatus } from "@/lib/types"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { AgentStorageConfirmationDialog } from "./agent-storage-confirmation-dialog"

interface AgentStorageMigrationSettingsProps {
  status: AgentStorageStatus
  onStatusChange: (status: AgentStorageStatus) => void
}

export function AgentStorageMigrationSettings({
  status,
  onStatusChange,
}: AgentStorageMigrationSettingsProps) {
  const t = useTranslations("AcpAgentSettings")
  const [path, setPath] = useState("")
  const [busy, setBusy] = useState(false)
  const [confirmSystemDrive, setConfirmSystemDrive] = useState(false)

  const migrate = async (allowSystemDrive: boolean) => {
    const destination = path.trim()
    if (!destination) return
    setBusy(true)
    try {
      const validation = await validateAgentStorageRoot(destination)
      if (!validation.writable) {
        throw new Error(validation.error || t("storage.invalidPath"))
      }
      if (validation.onSystemDrive && !allowSystemDrive) {
        setConfirmSystemDrive(true)
        return
      }
      const next = await migrateAgentStorage({
        root: validation.absolutePath,
        allowSystemDrive,
      })
      onStatusChange(next)
      setPath("")
      toast.success(t("storage.migrationComplete"))
    } catch (error) {
      toast.error(t("storage.migrationFailed"), {
        description: toErrorMessage(error),
      })
    } finally {
      setBusy(false)
    }
  }

  const chooseDirectory = async () => {
    const selected = await openFileDialog({
      directory: true,
      multiple: false,
      defaultPath: path || status.activeRoot || undefined,
      title: t("storage.migrationChooseTitle"),
    })
    if (typeof selected === "string") setPath(selected)
  }

  return (
    <div className="space-y-2 border-t pt-3">
      <div className="grid gap-2 md:grid-cols-[minmax(0,1fr)_auto_auto]">
        <Input
          aria-label={t("storage.migrationPathLabel")}
          value={path}
          placeholder={t("storage.migrationPlaceholder")}
          onChange={(event) => setPath(event.target.value)}
        />
        <Button
          size="icon"
          variant="outline"
          aria-label={t("storage.chooseDirectory")}
          onClick={chooseDirectory}
        >
          <FolderOpen className="h-4 w-4" />
        </Button>
        <Button disabled={busy || !path.trim()} onClick={() => migrate(false)}>
          {busy && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
          {busy ? t("storage.migrating") : t("storage.migrate")}
        </Button>
      </div>
      {status.previousRoot && (
        <p className="text-xs text-muted-foreground">
          {t("storage.previousRoot", { path: status.previousRoot })}
        </p>
      )}
      <AgentStorageConfirmationDialog
        kind={confirmSystemDrive ? "system" : null}
        onCancel={() => setConfirmSystemDrive(false)}
        onConfirm={() => {
          setConfirmSystemDrive(false)
          migrate(true)
        }}
      />
    </div>
  )
}
