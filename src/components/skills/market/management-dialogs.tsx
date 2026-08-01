"use client"

import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
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
import {
  RadioGroup,
  RadioGroupItem,
} from "@/components/ui/radio-group"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import type {
  SkillMarketAudience,
  SkillMarketCategory,
  SkillMarketTranslator,
  SkillMarketV2Detail,
} from "@/lib/skill-market"
import type { SkillMarketMetadataRequestV2 } from "@/lib/skill-market-source"

interface MetadataForm {
  displayName: string
  summary: string
  category: string
  iconUrl: string
  tags: string
  audience: SkillMarketAudience
}

function toForm(item: SkillMarketV2Detail): MetadataForm {
  return {
    displayName: item.displayName,
    summary: item.summary,
    category: item.category,
    iconUrl: item.iconUrl ?? "",
    tags: item.tags.join(", "),
    audience: item.audience,
  }
}

function EditMetadataDialog({
  target,
  categories,
  busy,
  onClose,
  onSave,
}: {
  target: SkillMarketV2Detail | null
  categories: SkillMarketCategory[]
  busy: boolean
  onClose: () => void
  onSave: (request: SkillMarketMetadataRequestV2) => Promise<void>
}) {
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
  const [form, setForm] = useState<MetadataForm | null>(null)
  useEffect(() => {
    setForm(target ? toForm(target) : null)
  }, [target])
  if (!target) return null
  if (!form) return null

  const update = (field: keyof MetadataForm, value: string) => {
    setForm((current) => (current ? { ...current, [field]: value } : current))
  }
  const valid = Boolean(form.displayName.trim()) && Boolean(form.category)
  const save = async () => {
    if (!valid) return
    await onSave({
      id: target.id,
      displayName: form.displayName.trim(),
      summary: form.summary.trim(),
      category: form.category,
      iconUrl: form.iconUrl.trim() || null,
      tags: form.tags
        .split(",")
        .map((tag) => tag.trim())
        .filter(Boolean),
      audience: form.audience,
    })
    onClose()
  }

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="max-w-xl rounded-lg">
        <DialogHeader>
          <DialogTitle>{t("manage.editMetadata")}</DialogTitle>
          <DialogDescription>{t("manage.editMetadataHint")}</DialogDescription>
        </DialogHeader>
        <div className="grid gap-3">
          <div className="grid gap-1.5">
            <Label htmlFor="manage-name">{t("upload.displayName")}</Label>
            <Input
              id="manage-name"
              value={form.displayName}
              onChange={(event) => update("displayName", event.target.value)}
            />
          </div>
          <div className="grid gap-1.5">
            <Label htmlFor="manage-summary">{t("upload.summary")}</Label>
            <Textarea
              id="manage-summary"
              value={form.summary}
              onChange={(event) => update("summary", event.target.value)}
              className="min-h-16"
            />
          </div>
          <div className="grid gap-1.5">
            <Label>{t("upload.category")}</Label>
            <Select
              value={form.category}
              onValueChange={(value) => update("category", value)}
            >
              <SelectTrigger className="w-full rounded-md">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {categories.map((category) => (
                  <SelectItem key={category.key} value={category.key}>
                    {t(`categories.${category.key}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="grid gap-1.5">
            <Label>{t("upload.audienceLabel")}</Label>
            <RadioGroup
              value={form.audience}
              onValueChange={(value) =>
                update("audience", value as SkillMarketAudience)
              }
              className="grid gap-2"
            >
              <Label className="flex items-center gap-2 rounded-md border px-3 py-2">
                <RadioGroupItem value="organization" />
                <span className="text-xs">
                  {t("upload.audienceOrganization")}
                </span>
              </Label>
              <Label className="flex items-center gap-2 rounded-md border px-3 py-2">
                <RadioGroupItem value="owner_private" />
                <span className="text-xs">
                  {t("upload.audienceOwnerPrivate")}
                </span>
              </Label>
            </RadioGroup>
          </div>
          <div className="grid gap-1.5">
            <Label htmlFor="manage-tags">{t("upload.tags")}</Label>
            <Input
              id="manage-tags"
              value={form.tags}
              onChange={(event) => update("tags", event.target.value)}
            />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" disabled={busy} onClick={onClose}>
            {t("actions.cancel")}
          </Button>
          <Button disabled={busy || !valid} onClick={() => void save()}>
            {t("actions.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel,
  busy,
  onOpenChange,
  onConfirm,
}: {
  open: boolean
  title: string
  description: string
  confirmLabel: string
  busy: boolean
  onOpenChange: (open: boolean) => void
  onConfirm: () => Promise<void>
}) {
  const t = useTranslations("SkillMarketV2")
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={busy}>
            {t("actions.cancel")}
          </AlertDialogCancel>
          <AlertDialogAction
            disabled={busy}
            onClick={() => void onConfirm()}
          >
            {confirmLabel}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

export interface ManagementDialogsProps {
  editTarget: SkillMarketV2Detail | null
  deleteTarget: SkillMarketV2Detail | null
  uninstallTarget: SkillMarketV2Detail | null
  categories: SkillMarketCategory[]
  busy: boolean
  onEditClose: () => void
  onDeleteClose: () => void
  onUninstallClose: () => void
  onSaveMetadata: (request: SkillMarketMetadataRequestV2) => Promise<void>
  onConfirmDelete: (id: string) => Promise<void>
  onConfirmUninstall: (id: string) => Promise<void>
}

export function ManagementDialogs(props: ManagementDialogsProps) {
  const t = useTranslations("SkillMarketV2")
  return (
    <>
      <EditMetadataDialog
        target={props.editTarget}
        categories={props.categories}
        busy={props.busy}
        onClose={props.onEditClose}
        onSave={props.onSaveMetadata}
      />
      <ConfirmDialog
        open={Boolean(props.deleteTarget)}
        title={t("manage.deleteTitle")}
        description={t("manage.deleteDescription", {
          name: props.deleteTarget?.displayName ?? "",
        })}
        confirmLabel={t("manage.delete")}
        busy={props.busy}
        onOpenChange={(open) => !open && props.onDeleteClose()}
        onConfirm={async () => {
          if (props.deleteTarget) {
            await props.onConfirmDelete(props.deleteTarget.id)
            props.onDeleteClose()
          }
        }}
      />
      <ConfirmDialog
        open={Boolean(props.uninstallTarget)}
        title={t("manage.uninstallTitle")}
        description={t("manage.uninstallDescription", {
          name: props.uninstallTarget?.displayName ?? "",
        })}
        confirmLabel={t("manage.uninstall")}
        busy={props.busy}
        onOpenChange={(open) => !open && props.onUninstallClose()}
        onConfirm={async () => {
          if (props.uninstallTarget) {
            await props.onConfirmUninstall(props.uninstallTarget.id)
            props.onUninstallClose()
          }
        }}
      />
    </>
  )
}
