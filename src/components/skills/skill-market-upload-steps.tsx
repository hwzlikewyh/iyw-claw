"use client"

import { FileText } from "lucide-react"
import { useTranslations } from "next-intl"
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
  SelectedSkillMarketFolder,
  SkillMarketCategory,
} from "@/lib/skill-market"

type Translator = (
  key: string,
  values?: Record<string, string | number>
) => string

export interface SkillMarketUploadDraft {
  slug: string
  displayName: string
  summary: string
  category: string
  iconUrl: string
  tags: string
  visibility: "public" | "private"
  version: string
  changelog: string
  dependencies: string
}

type MetadataStepProps = {
  draft: SkillMarketUploadDraft
  categories: SkillMarketCategory[]
  invalid: boolean
  onChange: (field: keyof SkillMarketUploadDraft, value: string) => void
}

function DraftInput({
  field,
  draft,
  onChange,
  type,
  placeholder,
}: {
  field: keyof SkillMarketUploadDraft
  draft: SkillMarketUploadDraft
  onChange: MetadataStepProps["onChange"]
  type?: string
  placeholder?: string
}) {
  const t = useTranslations("SkillsSettings.market") as unknown as Translator
  return (
    <label className="space-y-2">
      <Label>{t(`fields.${field}`)}</Label>
      <Input
        type={type}
        className={field === "version" ? "font-mono" : undefined}
        value={draft[field]}
        placeholder={placeholder}
        onChange={(event) => onChange(field, event.target.value)}
      />
    </label>
  )
}

function DraftTextarea({
  field,
  draft,
  onChange,
}: {
  field: "summary" | "changelog" | "dependencies"
  draft: SkillMarketUploadDraft
  onChange: MetadataStepProps["onChange"]
}) {
  const t = useTranslations("SkillsSettings.market") as unknown as Translator
  return (
    <label className="col-span-full space-y-2">
      <Label>{t(`fields.${field}`)}</Label>
      <Textarea
        className="min-h-20"
        value={draft[field]}
        onChange={(event) => onChange(field, event.target.value)}
      />
    </label>
  )
}

function CategoryField({ draft, categories, onChange }: MetadataStepProps) {
  const t = useTranslations("SkillsSettings.market") as unknown as Translator
  return (
    <label className="space-y-2">
      <Label>{t("fields.category")}</Label>
      <Select
        value={draft.category}
        onValueChange={(value) => onChange("category", value)}
      >
        <SelectTrigger className="w-full rounded-md">
          <SelectValue placeholder={t("fields.selectCategory")} />
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
  )
}

function VisibilityField({ draft, onChange }: MetadataStepProps) {
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
        checked={draft.visibility === "public"}
        onCheckedChange={(checked) =>
          onChange("visibility", checked ? "public" : "private")
        }
      />
    </div>
  )
}

export function SkillMarketMetadataStep({
  draft,
  categories,
  invalid,
  onChange,
}: MetadataStepProps) {
  const t = useTranslations("SkillsSettings.market") as unknown as Translator
  return (
    <section className="grid gap-4 sm:grid-cols-2">
      <h3 className="col-span-full text-sm font-semibold">
        {t("upload.steps.metadata")}
      </h3>
      <DraftInput field="slug" draft={draft} onChange={onChange} />
      <DraftInput field="displayName" draft={draft} onChange={onChange} />
      <DraftTextarea field="summary" draft={draft} onChange={onChange} />
      <CategoryField
        draft={draft}
        categories={categories}
        invalid={invalid}
        onChange={onChange}
      />
      <DraftInput field="version" draft={draft} onChange={onChange} />
      <DraftInput
        field="tags"
        draft={draft}
        onChange={onChange}
        placeholder={t("fields.tagsPlaceholder")}
      />
      <DraftInput
        field="iconUrl"
        draft={draft}
        onChange={onChange}
        type="url"
      />
      <DraftTextarea field="changelog" draft={draft} onChange={onChange} />
      <DraftTextarea field="dependencies" draft={draft} onChange={onChange} />
      <VisibilityField
        draft={draft}
        categories={categories}
        invalid={invalid}
        onChange={onChange}
      />
      {invalid ? (
        <p className="col-span-full text-xs text-destructive">
          {t("upload.formInvalid")}
        </p>
      ) : null}
    </section>
  )
}

export function SkillMarketReviewStep({
  draft,
  folder,
}: {
  draft: SkillMarketUploadDraft
  folder: SelectedSkillMarketFolder
}) {
  const t = useTranslations("SkillsSettings.market") as unknown as Translator
  return (
    <section className="space-y-3">
      <h3 className="text-sm font-semibold">{t("upload.steps.review")}</h3>
      <dl className="grid gap-2 text-xs sm:grid-cols-2">
        <div>
          <dt className="text-muted-foreground">{t("fields.slug")}</dt>
          <dd className="mt-1 break-all font-mono">{draft.slug}</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">{t("fields.version")}</dt>
          <dd className="mt-1 font-mono">{draft.version}</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">{t("fields.visibility")}</dt>
          <dd className="mt-1">{t(`visibility.${draft.visibility}`)}</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">{t("detail.fileTree")}</dt>
          <dd className="mt-1">
            {t("detail.fileCount", { count: folder.files.length })}
          </dd>
        </div>
        <div className="sm:col-span-2">
          <dt className="text-muted-foreground">{t("fields.dependencies")}</dt>
          <dd className="mt-1 whitespace-pre-wrap break-all font-mono">
            {draft.dependencies.trim() || t("detail.noDependencies")}
          </dd>
        </div>
      </dl>
      <div className="max-h-52 overflow-auto rounded-md border bg-muted/10 p-2">
        {folder.files.map((file) => (
          <div
            key={file.path}
            className="flex items-center gap-2 px-1 py-1 text-[11px]"
          >
            <FileText className="size-3 shrink-0 text-muted-foreground" />
            <span className="break-all font-mono">{file.path}</span>
          </div>
        ))}
      </div>
    </section>
  )
}
