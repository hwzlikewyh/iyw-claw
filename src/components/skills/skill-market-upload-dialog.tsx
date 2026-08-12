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
import { SkillMarketFolderPicker } from "@/components/skills/skill-market-folder-picker"
import {
  isValidSemVer,
  isValidSkillDependencies,
  parseSkillDependencies,
} from "@/components/skills/skill-market-semver"
import {
  SkillMarketMetadataStep,
  SkillMarketReviewStep,
  type SkillMarketUploadDraft,
} from "@/components/skills/skill-market-upload-steps"
import type {
  SelectedSkillMarketFolder,
  SkillMarketCategory,
  SkillMarketPublishRequest,
} from "@/lib/skill-market"
import { cn } from "@/lib/utils"

const EMPTY_DRAFT: SkillMarketUploadDraft = {
  slug: "",
  displayName: "",
  summary: "",
  category: "",
  iconUrl: "",
  tags: "",
  visibility: "private",
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

function isDraftValid(draft: SkillMarketUploadDraft): boolean {
  return (
    /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(draft.slug) &&
    Boolean(draft.displayName.trim()) &&
    Boolean(draft.summary.trim()) &&
    Boolean(draft.category) &&
    isValidSemVer(draft.version) &&
    isValidSkillDependencies(draft.dependencies)
  )
}

function buildPublishRequest(
  draft: SkillMarketUploadDraft,
  folder: SelectedSkillMarketFolder
): SkillMarketPublishRequest {
  return {
    ...draft,
    iconUrl: draft.iconUrl.trim() || null,
    tags: draft.tags
      .split(",")
      .map((tag) => tag.trim())
      .filter(Boolean),
    slug: draft.slug.trim(),
    displayName: draft.displayName.trim(),
    summary: draft.summary.trim(),
    version: draft.version.trim(),
    changelog: draft.changelog.trim(),
    packageType: folder.packageType,
    dependencies: parseSkillDependencies(draft.dependencies),
    files: folder.files,
  }
}

function useUploadForm(busy: boolean, onOpenChange: (open: boolean) => void) {
  const [step, setStep] = useState(1)
  const [folder, setFolder] = useState<SelectedSkillMarketFolder | null>(null)
  const [draft, setDraft] = useState<SkillMarketUploadDraft>(EMPTY_DRAFT)
  const [formError, setFormError] = useState(false)
  const close = (nextOpen: boolean) => {
    if (!nextOpen && !busy) {
      setStep(1)
      setFolder(null)
      setDraft(EMPTY_DRAFT)
      setFormError(false)
    }
    onOpenChange(nextOpen)
  }
  const update = (field: keyof SkillMarketUploadDraft, value: string) => {
    setDraft((current) => ({
      ...current,
      [field]: field === "slug" ? normalizeSlug(value) : value,
    }))
    setFormError(false)
  }
  const next = () => {
    if (step === 1 && !folder) return
    if (step === 2 && !isDraftValid(draft)) return setFormError(true)
    setStep((current) => Math.min(3, current + 1))
  }
  const selectFolder = (value: SelectedSkillMarketFolder | null) => {
    setFolder(value)
    if (!value) return
    setDraft((current) => ({
      ...current,
      slug: current.slug || normalizeSlug(value.name),
      displayName: current.displayName || value.name,
    }))
  }
  return {
    step,
    folder,
    draft,
    formError,
    setStep,
    close,
    update,
    next,
    selectFolder,
  }
}

type UploadContentProps = {
  form: ReturnType<typeof useUploadForm>
  categories: SkillMarketCategory[]
  busy: boolean
}

function UploadContent({ form, categories, busy }: UploadContentProps) {
  const t = useTranslations("SkillsSettings.market")
  if (form.step === 1) {
    return (
      <section className="space-y-3">
        <h3 className="text-sm font-semibold">{t("upload.steps.folder")}</h3>
        <p className="text-xs leading-5 text-muted-foreground">
          {t("upload.folderHint")}
        </p>
        <SkillMarketFolderPicker
          folder={form.folder}
          disabled={busy}
          onChange={form.selectFolder}
        />
      </section>
    )
  }
  if (form.step === 2) {
    return (
      <SkillMarketMetadataStep
        draft={form.draft}
        categories={categories}
        invalid={form.formError}
        onChange={form.update}
      />
    )
  }
  return form.folder ? (
    <SkillMarketReviewStep draft={form.draft} folder={form.folder} />
  ) : null
}

function UploadFooter({
  form,
  busy,
  onPublish,
}: {
  form: ReturnType<typeof useUploadForm>
  busy: boolean
  onPublish: () => Promise<void>
}) {
  const t = useTranslations("SkillsSettings.market")
  return (
    <DialogFooter>
      <Button
        variant="outline"
        disabled={busy}
        onClick={() =>
          form.step === 1
            ? form.close(false)
            : form.setStep((current) => current - 1)
        }
      >
        {form.step === 1 ? t("actions.cancel") : t("actions.back")}
      </Button>
      {form.step < 3 ? (
        <Button
          disabled={busy || (form.step === 1 && !form.folder)}
          onClick={form.next}
        >
          {t("actions.next")}
        </Button>
      ) : (
        <Button
          disabled={busy}
          onClick={() => void onPublish().catch(() => {})}
        >
          {busy ? <Loader2 className="size-3.5 animate-spin" /> : null}
          {t("actions.publish")}
        </Button>
      )}
    </DialogFooter>
  )
}

export function SkillMarketUploadDialog({
  open,
  categories,
  busy,
  onOpenChange,
  onPublish,
}: {
  open: boolean
  categories: SkillMarketCategory[]
  busy: boolean
  onOpenChange: (open: boolean) => void
  onPublish: (request: SkillMarketPublishRequest) => Promise<void>
}) {
  const t = useTranslations("SkillsSettings.market")
  const form = useUploadForm(busy, onOpenChange)
  const publish = async () => {
    if (!form.folder || !isDraftValid(form.draft)) return
    await onPublish(buildPublishRequest(form.draft, form.folder))
    form.close(false)
  }

  return (
    <Dialog open={open} onOpenChange={form.close}>
      <DialogContent className="max-w-3xl rounded-lg">
        <DialogHeader>
          <DialogTitle>{t("upload.title")}</DialogTitle>
          <DialogDescription>{t("upload.description")}</DialogDescription>
        </DialogHeader>
        <div className="flex gap-1" aria-label={t("upload.progressLabel")}>
          {[1, 2, 3].map((value) => (
            <span
              key={value}
              className={cn(
                "h-1 flex-1 rounded-full",
                value <= form.step ? "bg-primary" : "bg-muted"
              )}
            />
          ))}
        </div>
        <UploadContent form={form} categories={categories} busy={busy} />
        <UploadFooter form={form} busy={busy} onPublish={publish} />
      </DialogContent>
    </Dialog>
  )
}
