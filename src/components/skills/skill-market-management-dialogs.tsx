"use client"

import { Loader2 } from "lucide-react"
import { useState } from "react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { SkillMarketFolderPicker } from "@/components/skills/skill-market-folder-picker"
import {
  compareSemVer,
  isValidSemVer,
} from "@/components/skills/skill-market-semver"
import type {
  SelectedSkillMarketFolder,
  SkillMarketAddVersionRequest,
  SkillMarketDetail,
} from "@/lib/skill-market"

type VersionDialogProps = {
  open: boolean
  detail: SkillMarketDetail | null
  busy: boolean
  onOpenChange: (open: boolean) => void
  onPublish: (request: SkillMarketAddVersionRequest) => Promise<void>
}

function useVersionForm(props: VersionDialogProps) {
  const [folder, setFolder] = useState<SelectedSkillMarketFolder | null>(null)
  const [version, setVersion] = useState("")
  const [changelog, setChangelog] = useState("")
  const valid = Boolean(
    props.detail &&
    folder &&
    isValidSemVer(version) &&
    compareSemVer(version, props.detail.currentVersion.version) > 0
  )
  const close = (nextOpen: boolean) => {
    if (!nextOpen && !props.busy) {
      setFolder(null)
      setVersion("")
      setChangelog("")
    }
    props.onOpenChange(nextOpen)
  }
  const publish = async () => {
    if (!props.detail || !folder || !valid) return
    await props.onPublish({
      id: props.detail.id,
      version: version.trim(),
      changelog: changelog.trim(),
      files: folder.files,
    })
    close(false)
  }
  return {
    folder,
    setFolder,
    version,
    setVersion,
    changelog,
    setChangelog,
    valid,
    close,
    publish,
  }
}

type VersionForm = ReturnType<typeof useVersionForm>

function VersionFields({
  form,
  detail,
  busy,
}: {
  form: VersionForm
  detail: SkillMarketDetail | null
  busy: boolean
}) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <div className="space-y-4">
      <SkillMarketFolderPicker
        folder={form.folder}
        disabled={busy}
        onChange={form.setFolder}
      />
      <label className="space-y-2">
        <Label>{t("fields.version")}</Label>
        <Input
          className="font-mono"
          value={form.version}
          placeholder={t("version.placeholder")}
          onChange={(event) => form.setVersion(event.target.value)}
        />
        {form.version && !form.valid ? (
          <p className="text-xs text-destructive">
            {t("version.invalid", {
              version: detail?.currentVersion.version ?? "",
            })}
          </p>
        ) : null}
      </label>
      <label className="space-y-2">
        <Label>{t("fields.changelog")}</Label>
        <Textarea
          className="min-h-24"
          value={form.changelog}
          onChange={(event) => form.setChangelog(event.target.value)}
        />
      </label>
    </div>
  )
}

function VersionFooter({ form, busy }: { form: VersionForm; busy: boolean }) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <DialogFooter>
      <Button
        variant="outline"
        disabled={busy}
        onClick={() => form.close(false)}
      >
        {t("actions.cancel")}
      </Button>
      <Button
        disabled={busy || !form.valid}
        onClick={() => void form.publish().catch(() => {})}
      >
        {busy ? <Loader2 className="size-3.5 animate-spin" /> : null}
        {t("actions.publishVersion")}
      </Button>
    </DialogFooter>
  )
}

export function SkillMarketVersionDialog(props: VersionDialogProps) {
  const t = useTranslations("SkillsSettings.market")
  const form = useVersionForm(props)
  return (
    <Dialog open={props.open} onOpenChange={form.close}>
      <DialogContent className="max-w-2xl rounded-lg">
        <DialogHeader>
          <DialogTitle>{t("version.title")}</DialogTitle>
          <DialogDescription>
            {props.detail
              ? t("version.description", {
                  version: props.detail.currentVersion.version,
                })
              : ""}
          </DialogDescription>
        </DialogHeader>
        <VersionFields form={form} detail={props.detail} busy={props.busy} />
        <VersionFooter form={form} busy={props.busy} />
      </DialogContent>
    </Dialog>
  )
}
