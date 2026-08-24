"use client"

import { useState } from "react"
import { Loader2 } from "lucide-react"
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
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import { SkillMarketFolderPicker } from "@/components/skills/skill-market-folder-picker"
import { parseSkillDependencies } from "@/components/skills/skill-market-semver"
import {
  hasSkillMarketUploadErrors,
  type SkillMarketUploadErrors,
  type SkillMarketUploadFieldError,
  validateSkillMarketPublishForm,
  validateSkillMarketVersionForm,
} from "@/components/skills/skill-market-upload-validation"
import type {
  SelectedSkillMarketFolder,
  SkillMarketAudience,
  SkillMarketCategory,
  SkillMarketTranslator,
} from "@/lib/skill-market"
import type {
  SkillMarketAddVersionRequestV2,
  SkillMarketPublishRequestV2,
} from "@/lib/skill-market-source"
import { cn } from "@/lib/utils"

export type SkillMarketUploadMode = "publish" | "addVersion"

interface PublishForm {
  slug: string
  displayName: string
  summary: string
  category: string
  tags: string
  iconUrl: string
  audience: SkillMarketAudience
  version: string
  changelog: string
  dependencies: string
}

const EMPTY_PUBLISH: PublishForm = {
  slug: "",
  displayName: "",
  summary: "",
  category: "",
  tags: "",
  iconUrl: "",
  audience: "organization",
  version: "1.0.0",
  changelog: "",
  dependencies: "",
}

function normalizeSlug(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, "-")
    .replace(/^-+|-+$/g, "")
}

export interface SkillMarketUploadDialogProps {
  open: boolean
  mode: SkillMarketUploadMode
  categories: SkillMarketCategory[]
  targetSkillId: string | null
  busy: boolean
  onOpenChange: (open: boolean) => void
  onPublish: (request: SkillMarketPublishRequestV2) => Promise<void>
  onAddVersion: (request: SkillMarketAddVersionRequestV2) => Promise<void>
}

function fieldErrorText(
  t: SkillMarketTranslator,
  error: SkillMarketUploadFieldError
): string {
  if (error.code === "dependencies" && error.line) {
    return t("upload.errors.dependenciesLine", { line: error.line })
  }
  return t(`upload.errors.${error.code}`)
}

function FieldError({
  t,
  error,
}: {
  t: SkillMarketTranslator
  error?: SkillMarketUploadFieldError
}) {
  return error ? (
    <p className="text-xs text-destructive">{fieldErrorText(t, error)}</p>
  ) : null
}

export function SkillMarketUploadDialog(props: SkillMarketUploadDialogProps) {
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
  const [step, setStep] = useState(1)
  const [folder, setFolder] = useState<SelectedSkillMarketFolder | null>(null)
  const [form, setForm] = useState<PublishForm>(EMPTY_PUBLISH)
  const [errors, setErrors] = useState<SkillMarketUploadErrors>({})

  const reset = () => {
    setStep(1)
    setFolder(null)
    setForm(EMPTY_PUBLISH)
    setErrors({})
  }

  const close = (nextOpen: boolean) => {
    if (!nextOpen && !props.busy) reset()
    props.onOpenChange(nextOpen)
  }

  const update = (field: keyof PublishForm, value: string) => {
    setForm((current) => ({
      ...current,
      [field]: field === "slug" ? normalizeSlug(value) : value,
    }))
    setErrors({})
  }

  const validate = () =>
    props.mode === "publish"
      ? validateSkillMarketPublishForm(form)
      : validateSkillMarketVersionForm(form)

  const next = () => {
    if (step === 1 && !folder) return
    if (step === 2) {
      const validationErrors = validate()
      if (hasSkillMarketUploadErrors(validationErrors)) {
        setErrors(validationErrors)
        return
      }
    }
    setStep((current) => Math.min(3, current + 1))
  }

  const submit = async () => {
    if (!folder) return
    const validationErrors = validate()
    if (hasSkillMarketUploadErrors(validationErrors)) {
      setErrors(validationErrors)
      return
    }
    if (props.mode === "publish") {
      await props.onPublish({
        slug: form.slug.trim(),
        displayName: form.displayName.trim(),
        summary: form.summary.trim(),
        category: form.category,
        iconUrl: form.iconUrl.trim() || null,
        tags: splitTags(form.tags),
        audience: form.audience,
        version: form.version.trim(),
        changelog: form.changelog.trim(),
        packageType: folder.packageType,
        dependencies: parseSkillDependencies(form.dependencies),
        files: folder.files,
      })
    } else {
      await props.onAddVersion({
        id: props.targetSkillId ?? "",
        version: form.version.trim(),
        changelog: form.changelog.trim(),
        packageType: folder.packageType,
        dependencies: parseSkillDependencies(form.dependencies),
        files: folder.files,
      })
    }
    reset()
  }

  const isPublish = props.mode === "publish"

  return (
    <Dialog open={props.open} onOpenChange={close}>
      <DialogContent className="max-w-3xl rounded-lg">
        <DialogHeader>
          <DialogTitle>
            {isPublish ? t("upload.title") : t("upload.addVersionTitle")}
          </DialogTitle>
          <DialogDescription>
            {isPublish ? t("upload.subtitle") : t("upload.addVersionSubtitle")}
          </DialogDescription>
        </DialogHeader>
        <div className="flex gap-1" aria-label={t("upload.progressLabel")}>
          {[1, 2, 3].map((value) => (
            <span
              key={value}
              className={cn(
                "h-1 flex-1 rounded-full",
                value <= step ? "bg-primary" : "bg-muted"
              )}
            />
          ))}
        </div>

        {step === 1 ? (
          <section className="space-y-3">
            <h3 className="text-sm font-semibold">{t("upload.folderStep")}</h3>
            <p className="text-xs leading-5 text-muted-foreground">
              {t("upload.folderHint")}
            </p>
            <SkillMarketFolderPicker
              folder={folder}
              disabled={props.busy}
              onChange={setFolder}
            />
          </section>
        ) : null}

        {step === 2 ? (
          <section className="grid gap-3">
            {isPublish ? (
              <>
                <div className="grid gap-1.5">
                  <Label htmlFor="market-upload-slug">{t("upload.slug")}</Label>
                  <Input
                    id="market-upload-slug"
                    aria-invalid={Boolean(errors.slug)}
                    className={cn(errors.slug && "border-destructive")}
                    value={form.slug}
                    onChange={(event) => update("slug", event.target.value)}
                    placeholder="my-skill"
                  />
                  <FieldError t={t} error={errors.slug} />
                </div>
                <div className="grid gap-1.5">
                  <Label htmlFor="market-upload-name">
                    {t("upload.displayName")}
                  </Label>
                  <Input
                    id="market-upload-name"
                    aria-invalid={Boolean(errors.displayName)}
                    className={cn(errors.displayName && "border-destructive")}
                    value={form.displayName}
                    onChange={(event) =>
                      update("displayName", event.target.value)
                    }
                  />
                  <FieldError t={t} error={errors.displayName} />
                </div>
                <div className="grid gap-1.5">
                  <Label htmlFor="market-upload-summary">
                    {t("upload.summary")}
                  </Label>
                  <Textarea
                    id="market-upload-summary"
                    value={form.summary}
                    onChange={(event) => update("summary", event.target.value)}
                    className="min-h-16"
                  />
                </div>
                <div className="grid gap-1.5">
                  <Label>{t("upload.category")}</Label>
                  <Select
                    value={form.category || "none"}
                    onValueChange={(value) =>
                      update("category", value === "none" ? "" : value)
                    }
                  >
                    <SelectTrigger
                      className={cn(
                        "w-full rounded-md",
                        errors.category && "border-destructive"
                      )}
                      aria-invalid={Boolean(errors.category)}
                    >
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="none">
                        {t("filters.categoryAll")}
                      </SelectItem>
                      {props.categories.map((category) => (
                        <SelectItem key={category.key} value={category.key}>
                          {t(`categories.${category.key}`)}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  <FieldError t={t} error={errors.category} />
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
                  <p className="text-[10px] text-muted-foreground">
                    {t("upload.audienceHint")}
                  </p>
                </div>
              </>
            ) : null}
            <div className="grid gap-1.5">
              <Label htmlFor="market-upload-version">
                {t("upload.version")}
              </Label>
              <Input
                id="market-upload-version"
                aria-invalid={Boolean(errors.version)}
                className={cn(
                  "font-mono",
                  errors.version && "border-destructive"
                )}
                value={form.version}
                onChange={(event) => update("version", event.target.value)}
                placeholder="1.0.0"
              />
              <FieldError t={t} error={errors.version} />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="market-upload-changelog">
                {t("upload.changelog")}
              </Label>
              <Textarea
                id="market-upload-changelog"
                value={form.changelog}
                onChange={(event) => update("changelog", event.target.value)}
                className="min-h-16"
              />
            </div>
            <div className="grid gap-1.5">
              <Label htmlFor="market-upload-deps">
                {t("upload.dependencies")}
              </Label>
              <Textarea
                id="market-upload-deps"
                aria-invalid={Boolean(errors.dependencies)}
                value={form.dependencies}
                onChange={(event) => update("dependencies", event.target.value)}
                placeholder="slug@1.0.0"
                className={cn(
                  "min-h-16 font-mono text-xs",
                  errors.dependencies && "border-destructive"
                )}
              />
              <p className="text-[10px] text-muted-foreground">
                {t("upload.dependenciesHint")}
              </p>
              <FieldError t={t} error={errors.dependencies} />
            </div>
            {isPublish ? (
              <div className="grid gap-1.5">
                <Label htmlFor="market-upload-tags">{t("upload.tags")}</Label>
                <Input
                  id="market-upload-tags"
                  value={form.tags}
                  onChange={(event) => update("tags", event.target.value)}
                  placeholder="tag1, tag2"
                />
              </div>
            ) : null}
          </section>
        ) : null}

        {step === 3 && folder ? (
          <section className="space-y-2 rounded-md border bg-muted/10 p-3 text-xs">
            <p className="font-medium">
              {isPublish ? form.displayName : folder.name}
            </p>
            <p className="text-muted-foreground">
              {t("upload.reviewLine", {
                version: form.version,
                files: folder.files.length,
              })}
            </p>
            {isPublish ? (
              <p className="text-muted-foreground">
                {t(`audience.${form.audience}`)}
              </p>
            ) : null}
            <p className="text-muted-foreground">{t("upload.buildHint")}</p>
          </section>
        ) : null}

        <DialogFooter>
          <Button
            variant="outline"
            disabled={props.busy}
            onClick={() =>
              step === 1 ? close(false) : setStep((current) => current - 1)
            }
          >
            {step === 1 ? t("actions.cancel") : t("actions.back")}
          </Button>
          {step < 3 ? (
            <Button
              disabled={props.busy || (step === 1 && !folder)}
              onClick={next}
            >
              {t("actions.next")}
            </Button>
          ) : (
            <Button
              disabled={props.busy}
              onClick={() => void submit().catch(() => {})}
            >
              {props.busy ? (
                <Loader2 className="size-3.5 animate-spin" aria-hidden="true" />
              ) : null}
              {t(`upload.${isPublish ? "publish" : "addVersionSubmit"}`)}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function splitTags(value: string): string[] {
  return value
    .split(",")
    .map((tag) => tag.trim())
    .filter(Boolean)
}
