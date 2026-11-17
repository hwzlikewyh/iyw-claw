"use client"

import { Loader2 } from "lucide-react"
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
import { cn } from "@/lib/utils"
import type { SkillMarketUploadDialogProps } from "./upload-form"
import { useSkillUploadForm } from "./use-skill-upload-form"
import { UploadDetailsStep } from "./upload-details-step"

export type {
  SkillMarketUploadMode,
  SkillMarketUploadDialogProps,
} from "./upload-form"

export function SkillMarketUploadDialog(props: SkillMarketUploadDialogProps) {
  const {
    t,
    step,
    setStep,
    folder,
    form,
    errors,
    nameCheck,
    busy,
    folderMismatch,
    close,
    update,
    isPublish,
    next,
    submit,
    selectFolder,
  } = useSkillUploadForm(props)

  return (
    <Dialog open={props.open} onOpenChange={close}>
      <DialogContent className="max-w-3xl rounded-lg">
        <DialogHeader>
          <DialogTitle>
            {isPublish
              ? t(
                  props.packageType === "plugin"
                    ? "upload.pluginTitle"
                    : "upload.title"
                )
              : t("upload.addVersionTitle")}
          </DialogTitle>
          <DialogDescription>
            {isPublish
              ? t(
                  props.packageType === "plugin"
                    ? "upload.pluginSubtitle"
                    : "upload.subtitle"
                )
              : t("upload.addVersionSubtitle")}
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
              {t(
                props.packageType === "plugin"
                  ? "upload.pluginFolderHint"
                  : "upload.folderHint"
              )}
            </p>
            <SkillMarketFolderPicker
              folder={folder}
              disabled={busy}
              selectLabel={
                props.packageType === "plugin"
                  ? t("upload.pluginSelectFolder")
                  : undefined
              }
              onChange={selectFolder}
            />
            {folderMismatch ? (
              <p role="alert" className="text-xs text-destructive">
                {t("upload.errors.wrongPackageType")}
              </p>
            ) : null}
          </section>
        ) : null}

        {step === 2 && (
          <UploadDetailsStep
            t={t}
            busy={busy}
            isPublish={isPublish}
            form={form}
            errors={errors}
            nameCheck={nameCheck}
            categories={props.categories}
            update={update}
          />
        )}

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
            disabled={busy}
            onClick={() =>
              step === 1 ? close(false) : setStep((current) => current - 1)
            }
          >
            {step === 1 ? t("actions.cancel") : t("actions.back")}
          </Button>
          {step < 3 ? (
            <Button
              disabled={
                busy ||
                (step === 1 && (!folder || folderMismatch)) ||
                (step === 2 && isPublish && nameCheck.status !== "available")
              }
              onClick={next}
            >
              {t("actions.next")}
            </Button>
          ) : (
            <Button disabled={busy} onClick={submit}>
              {busy ? (
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
