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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import type {
  SkillMarketCategory,
  SkillMarketDetail,
  SkillMarketMetadataRequest,
} from "@/lib/skill-market"

type Translator = (
  key: string,
  values?: Record<string, string | number>
) => string

type MetadataFormProps = {
  detail: SkillMarketDetail
  categories: SkillMarketCategory[]
  busy: boolean
  onCancel: () => void
  onSave: (request: SkillMarketMetadataRequest) => Promise<void>
}

function useMetadataForm(props: MetadataFormProps) {
  const [name, setName] = useState(props.detail.displayName)
  const [summary, setSummary] = useState(props.detail.summary)
  const [category, setCategory] = useState(props.detail.category)
  const [iconUrl, setIconUrl] = useState(props.detail.iconUrl ?? "")
  const [tags, setTags] = useState(props.detail.tags.join(", "))
  const [visibility, setVisibility] = useState(props.detail.visibility)
  const valid = Boolean(name.trim() && summary.trim() && category)
  const save = async () => {
    if (!valid) return
    await props.onSave({
      id: props.detail.id,
      displayName: name.trim(),
      summary: summary.trim(),
      category,
      iconUrl: iconUrl.trim() || null,
      tags: tags
        .split(",")
        .map((tag) => tag.trim())
        .filter(Boolean),
      visibility,
    })
    props.onCancel()
  }
  return {
    name,
    setName,
    summary,
    setSummary,
    category,
    setCategory,
    iconUrl,
    setIconUrl,
    tags,
    setTags,
    visibility,
    setVisibility,
    valid,
    save,
  }
}

type MetadataFormState = ReturnType<typeof useMetadataForm>

function MetadataPrimaryFields({
  form,
  categories,
}: {
  form: MetadataFormState
  categories: SkillMarketCategory[]
}) {
  const t = useTranslations("SkillsSettings.market") as unknown as Translator
  return (
    <>
      <label className="space-y-2">
        <Label>{t("fields.displayName")}</Label>
        <Input
          value={form.name}
          onChange={(event) => form.setName(event.target.value)}
        />
      </label>
      <label className="space-y-2">
        <Label>{t("fields.category")}</Label>
        <Select value={form.category} onValueChange={form.setCategory}>
          <SelectTrigger className="w-full rounded-md">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {categories.map((item) => (
              <SelectItem key={item.key} value={item.key}>
                {t(`categories.${item.key}`)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </label>
      <label className="col-span-full space-y-2">
        <Label>{t("fields.summary")}</Label>
        <Textarea
          className="min-h-24"
          value={form.summary}
          onChange={(event) => form.setSummary(event.target.value)}
        />
      </label>
    </>
  )
}

function MetadataSecondaryFields({ form }: { form: MetadataFormState }) {
  const t = useTranslations("SkillsSettings.market") as unknown as Translator
  return (
    <>
      <label className="space-y-2">
        <Label>{t("fields.tags")}</Label>
        <Input
          value={form.tags}
          onChange={(event) => form.setTags(event.target.value)}
        />
      </label>
      <label className="space-y-2">
        <Label>{t("fields.iconUrl")}</Label>
        <Input
          type="url"
          value={form.iconUrl}
          onChange={(event) => form.setIconUrl(event.target.value)}
        />
      </label>
    </>
  )
}

function MetadataVisibility({
  form,
  official,
}: {
  form: MetadataFormState
  official: boolean
}) {
  const t = useTranslations("SkillsSettings.market") as unknown as Translator
  return (
    <div className="col-span-full flex items-center justify-between gap-4 rounded-md border px-3 py-2.5">
      <div>
        <Label>{t("fields.public")}</Label>
        <p className="mt-1 text-xs text-muted-foreground">
          {t("fields.visibilityHint")}
        </p>
      </div>
      <Switch
        disabled={official}
        checked={form.visibility === "public"}
        onCheckedChange={(checked) =>
          form.setVisibility(checked ? "public" : "private")
        }
      />
    </div>
  )
}

function MetadataForm(props: MetadataFormProps) {
  const t = useTranslations("SkillsSettings.market") as unknown as Translator
  const form = useMetadataForm(props)
  return (
    <DialogContent className="max-w-xl rounded-lg">
      <DialogHeader>
        <DialogTitle>{t("metadata.title")}</DialogTitle>
        <DialogDescription>{t("metadata.description")}</DialogDescription>
      </DialogHeader>
      <div className="grid gap-4 sm:grid-cols-2">
        <MetadataPrimaryFields form={form} categories={props.categories} />
        <MetadataSecondaryFields form={form} />
        <MetadataVisibility
          form={form}
          official={props.detail.publisherType === "official"}
        />
      </div>
      <DialogFooter>
        <Button
          variant="outline"
          disabled={props.busy}
          onClick={props.onCancel}
        >
          {t("actions.cancel")}
        </Button>
        <Button
          disabled={props.busy || !form.valid}
          onClick={() => void form.save().catch(() => {})}
        >
          {props.busy ? <Loader2 className="size-3.5 animate-spin" /> : null}
          {t("actions.save")}
        </Button>
      </DialogFooter>
    </DialogContent>
  )
}

export function SkillMarketMetadataDialog({
  open,
  detail,
  categories,
  busy,
  onOpenChange,
  onSave,
}: {
  open: boolean
  detail: SkillMarketDetail | null
  categories: SkillMarketCategory[]
  busy: boolean
  onOpenChange: (open: boolean) => void
  onSave: (request: SkillMarketMetadataRequest) => Promise<void>
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {open && detail ? (
        <MetadataForm
          key={detail.id}
          detail={detail}
          categories={categories}
          busy={busy}
          onCancel={() => onOpenChange(false)}
          onSave={onSave}
        />
      ) : null}
    </Dialog>
  )
}
