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
import { cn } from "@/lib/utils"
import { UploadFieldError } from "./upload-field-error"
import type { UploadDetailsProps } from "./upload-form"

function GeneratedSlugField({
  t,
  errors,
  nameCheck,
}: Pick<UploadDetailsProps, "t" | "errors" | "nameCheck">) {
  return (
    <div className="grid gap-1.5">
      <Label>{t("upload.slug")}</Label>
      <p
        className="break-all font-mono text-xs text-muted-foreground"
        aria-live="polite"
      >
        {nameCheck.slug || t("upload.slugGenerating")}
      </p>
      <p className="text-xs text-muted-foreground">{t("upload.slugHint")}</p>
      <UploadFieldError t={t} error={errors.slug} />
    </div>
  )
}

function NameCheckStatus({
  t,
  status,
  retry,
}: Pick<UploadDetailsProps, "t"> & { status: string; retry: () => void }) {
  if (status === "checking")
    return (
      <p className="text-xs text-muted-foreground">
        {t("upload.nameChecking")}
      </p>
    )
  if (status === "available")
    return (
      <p className="text-xs text-emerald-600">{t("upload.nameAvailable")}</p>
    )
  if (status === "duplicate")
    return (
      <p role="alert" className="text-xs text-destructive">
        {t("upload.errors.duplicate")}
      </p>
    )
  if (status === "slugConflict")
    return (
      <p role="alert" className="text-xs text-destructive">
        {t("upload.slugConflict")}
      </p>
    )
  if (status === "error")
    return (
      <p className="text-xs text-destructive">
        {t("upload.nameCheckFailed")}{" "}
        <button type="button" className="underline" onClick={retry}>
          {t("list.retry")}
        </button>
      </p>
    )
  return null
}

function DisplayNameField({
  t,
  form,
  errors,
  nameCheck,
  update,
}: Pick<UploadDetailsProps, "t" | "form" | "errors" | "nameCheck" | "update">) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor="market-upload-name">{t("upload.displayName")}</Label>
      <Input
        id="market-upload-name"
        aria-invalid={
          Boolean(errors.displayName) || nameCheck.status === "duplicate"
        }
        className={cn(errors.displayName && "border-destructive")}
        value={form.displayName}
        onChange={(event) => update("displayName", event.target.value)}
      />
      <UploadFieldError t={t} error={errors.displayName} />
      <NameCheckStatus
        t={t}
        status={nameCheck.status}
        retry={nameCheck.retry}
      />
    </div>
  )
}

function SummaryField({
  t,
  form,
  update,
}: Pick<UploadDetailsProps, "t" | "form" | "update">) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor="market-upload-summary">{t("upload.summary")}</Label>
      <Textarea
        id="market-upload-summary"
        value={form.summary}
        onChange={(event) => update("summary", event.target.value)}
        className="min-h-16"
      />
    </div>
  )
}

function CategoryField({
  t,
  form,
  errors,
  categories,
  update,
}: Pick<
  UploadDetailsProps,
  "t" | "form" | "errors" | "categories" | "update"
>) {
  return (
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
          <SelectItem value="none">{t("filters.categoryAll")}</SelectItem>
          {categories.map((category) => (
            <SelectItem key={category.key} value={category.key}>
              {t(`categories.${category.key}`)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <UploadFieldError t={t} error={errors.category} />
    </div>
  )
}

function AudienceField({
  t,
  form,
  update,
}: Pick<UploadDetailsProps, "t" | "form" | "update">) {
  return (
    <div className="grid gap-1.5">
      <Label>{t("upload.audienceLabel")}</Label>
      <RadioGroup
        value={form.audience}
        onValueChange={(value) => update("audience", value)}
        className="grid gap-2"
      >
        <Label className="flex items-center gap-2 rounded-md border px-3 py-2">
          <RadioGroupItem value="organization" />
          <span className="text-xs">{t("upload.audienceOrganization")}</span>
        </Label>
        <Label className="flex items-center gap-2 rounded-md border px-3 py-2">
          <RadioGroupItem value="owner_private" />
          <span className="text-xs">{t("upload.audienceOwnerPrivate")}</span>
        </Label>
      </RadioGroup>
      <p className="text-[10px] text-muted-foreground">
        {t("upload.audienceHint")}
      </p>
    </div>
  )
}

export function UploadPublishFields(props: UploadDetailsProps) {
  return (
    <>
      <GeneratedSlugField {...props} />
      <DisplayNameField {...props} />
      <SummaryField {...props} />
      <CategoryField {...props} />
      <AudienceField {...props} />
    </>
  )
}
