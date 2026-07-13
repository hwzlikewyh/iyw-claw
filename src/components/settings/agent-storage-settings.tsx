import { useEffect, useMemo, useState } from "react"
import { AlertTriangle, FolderOpen, Loader2, RotateCcw } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import {
  initializeAgentStorage,
  updateAgentProfileOverride,
  validateAgentStorageRoot,
} from "@/lib/api"
import { toErrorMessage } from "@/lib/app-error"
import { openFileDialog } from "@/lib/platform"
import type { AgentStorageStatus, AgentType } from "@/lib/types"
import { relaunchApp } from "@/lib/updater"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { AgentStorageConfirmationDialog } from "./agent-storage-confirmation-dialog"
import { AgentStorageMigrationSettings } from "./agent-storage-migration-settings"

interface AgentStorageSettingsProps {
  status: AgentStorageStatus | null
  selectedAgent: { agentType: AgentType; name: string } | null
  onStatusChange: (status: AgentStorageStatus) => void
}

type Confirmation =
  | { kind: "root-system" }
  | { kind: "profile-system"; path: string }
  | { kind: "profile-global"; path: string; allowSystemDrive: boolean }

export function AgentStorageSettings({
  status,
  selectedAgent,
  onStatusChange,
}: AgentStorageSettingsProps) {
  const t = useTranslations("AcpAgentSettings")
  const [root, setRoot] = useState("")
  const [profilePath, setProfilePath] = useState("")
  const [importExisting, setImportExisting] = useState(true)
  const [busy, setBusy] = useState(false)
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null)

  const profile = useMemo(
    () =>
      status?.profilePaths.find(
        (item) => item.agentType === selectedAgent?.agentType
      ) ?? null,
    [selectedAgent?.agentType, status?.profilePaths]
  )

  useEffect(() => {
    setRoot(status?.activeRoot ?? status?.suggestedRoot ?? "")
  }, [status?.activeRoot, status?.suggestedRoot])

  useEffect(() => {
    setProfilePath(profile?.path ?? "")
  }, [profile?.path])

  if (!status) return null

  const pickDirectory = async (
    current: string,
    apply: (path: string) => void
  ) => {
    const selected = await openFileDialog({
      directory: true,
      multiple: false,
      defaultPath: current || undefined,
      title: t("storage.chooseDirectoryTitle"),
    })
    if (typeof selected === "string") apply(selected)
  }

  const initialize = async (allowSystemDrive: boolean) => {
    const candidate = root.trim()
    if (!candidate) return
    setBusy(true)
    try {
      const validation = await validateAgentStorageRoot(candidate)
      if (!validation.writable) {
        throw new Error(validation.error || t("storage.invalidPath"))
      }
      if (validation.onSystemDrive && !allowSystemDrive) {
        setConfirmation({ kind: "root-system" })
        return
      }
      const next = await initializeAgentStorage({
        root: validation.absolutePath,
        allowSystemDrive,
        importExistingSettings: importExisting,
      })
      onStatusChange(next)
      toast.success(t("storage.initialized"))
    } catch (error) {
      toast.error(t("storage.saveFailed"), {
        description: toErrorMessage(error),
      })
    } finally {
      setBusy(false)
    }
  }

  const saveProfile = async (
    path: string,
    allowSystemDrive: boolean,
    allowUserGlobalProfile: boolean
  ) => {
    if (!selectedAgent) return
    setBusy(true)
    try {
      const validation = await validateAgentStorageRoot(path)
      if (!validation.writable) {
        throw new Error(validation.error || t("storage.invalidPath"))
      }
      if (validation.onSystemDrive && !allowSystemDrive) {
        setConfirmation({ kind: "profile-system", path })
        return
      }
      const next = await updateAgentProfileOverride({
        agentType: selectedAgent.agentType,
        path: validation.absolutePath,
        allowSystemDrive,
        allowUserGlobalProfile,
      })
      onStatusChange(next)
      toast.success(t("storage.profileSaved"))
    } catch (error) {
      const message = toErrorMessage(error)
      if (
        !allowUserGlobalProfile &&
        message.toLowerCase().includes("user-global")
      ) {
        setConfirmation({ kind: "profile-global", path, allowSystemDrive })
      } else {
        toast.error(t("storage.saveFailed"), { description: message })
      }
    } finally {
      setBusy(false)
    }
  }

  const resetProfile = async () => {
    if (!selectedAgent) return
    setBusy(true)
    try {
      const next = await updateAgentProfileOverride({
        agentType: selectedAgent.agentType,
        path: null,
        allowSystemDrive: false,
        allowUserGlobalProfile: false,
      })
      onStatusChange(next)
      toast.success(t("storage.profileReset"))
    } catch (error) {
      toast.error(t("storage.saveFailed"), {
        description: toErrorMessage(error),
      })
    } finally {
      setBusy(false)
    }
  }

  const confirm = async () => {
    const pending = confirmation
    setConfirmation(null)
    if (!pending) return
    if (pending.kind === "root-system") await initialize(true)
    if (pending.kind === "profile-system") {
      await saveProfile(pending.path, true, false)
    }
    if (pending.kind === "profile-global") {
      await saveProfile(pending.path, pending.allowSystemDrive, true)
    }
  }

  return (
    <>
      <section className="mb-3 border-y bg-muted/20 px-3 py-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h3 className="text-sm font-medium">{t("storage.title")}</h3>
            <p className="mt-0.5 text-xs text-muted-foreground">
              {t("storage.description")}
            </p>
          </div>
          {status.restartRequired && (
            <Button size="sm" variant="outline" onClick={() => relaunchApp()}>
              {t("storage.restartNow")}
            </Button>
          )}
        </div>

        {!status.initialized ? (
          <div className="mt-3 grid gap-2 md:grid-cols-[1fr_auto]">
            <div className="flex gap-2">
              <Input
                aria-label={t("storage.rootLabel")}
                value={root}
                onChange={(event) => setRoot(event.target.value)}
              />
              <Button
                size="icon"
                variant="outline"
                aria-label={t("storage.chooseDirectory")}
                onClick={() => pickDirectory(root, setRoot)}
              >
                <FolderOpen className="h-4 w-4" />
              </Button>
            </div>
            <Button
              disabled={busy || !root.trim()}
              onClick={() => initialize(false)}
            >
              {busy && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
              {t("storage.initialize")}
            </Button>
            <label className="flex items-center gap-2 text-xs text-muted-foreground md:col-span-2">
              <Checkbox
                checked={importExisting}
                onCheckedChange={(checked) =>
                  setImportExisting(checked === true)
                }
              />
              {t("storage.importExisting")}
            </label>
          </div>
        ) : (
          <div className="mt-3 space-y-3">
            <p
              className="truncate font-mono text-xs text-muted-foreground"
              title={root}
            >
              {t("storage.rootLabel")}: {root}
            </p>
            {selectedAgent && profile && (
              <div className="grid gap-2 md:grid-cols-[minmax(0,1fr)_auto_auto]">
                <Input
                  aria-label={t("storage.profilePathLabel", {
                    name: selectedAgent.name,
                  })}
                  value={profilePath}
                  onChange={(event) => setProfilePath(event.target.value)}
                />
                <Button
                  size="icon"
                  variant="outline"
                  aria-label={t("storage.chooseDirectory")}
                  onClick={() => pickDirectory(profilePath, setProfilePath)}
                >
                  <FolderOpen className="h-4 w-4" />
                </Button>
                <div className="flex gap-2">
                  <Button
                    size="sm"
                    disabled={busy || !profilePath.trim()}
                    onClick={() =>
                      saveProfile(profilePath.trim(), false, false)
                    }
                  >
                    {t("storage.saveProfile")}
                  </Button>
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={busy || !profile.overridden}
                    onClick={resetProfile}
                  >
                    <RotateCcw className="mr-1 h-3.5 w-3.5" />
                    {t("storage.resetProfile")}
                  </Button>
                </div>
              </div>
            )}
            {status.restartRequired && (
              <p className="flex items-center gap-1.5 text-xs text-amber-600 dark:text-amber-400">
                <AlertTriangle className="h-3.5 w-3.5" />
                {t("storage.restartRequired")}
              </p>
            )}
            <AgentStorageMigrationSettings {...{ status, onStatusChange }} />
          </div>
        )}
      </section>

      <AgentStorageConfirmationDialog
        kind={
          confirmation?.kind === "profile-global"
            ? "global"
            : confirmation
              ? "system"
              : null
        }
        onCancel={() => setConfirmation(null)}
        onConfirm={confirm}
      />
    </>
  )
}
