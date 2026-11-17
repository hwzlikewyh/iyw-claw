import { useCallback } from "react"
import { toast } from "sonner"
import { parseSkillDependencies } from "@/components/skills/skill-market-semver"
import { toErrorMessage } from "@/lib/app-error"
import { prepareSkillPublication } from "@/lib/market-publishing"
import type { SkillMarketUploadDialogProps } from "./upload-form"
import { splitTags } from "./upload-form"
import type { UploadFormState } from "./use-upload-form-state"
import { validUploadForm } from "./use-upload-form-actions"

async function publishNew(
  props: SkillMarketUploadDialogProps,
  state: UploadFormState
) {
  const folder = state.folder
  if (!folder) return
  const checked = await prepareSkillPublication({
    folderName: folder.name,
    displayName: state.name,
    packageType: state.packageType,
  })
  if (!checked.nameAvailable || !checked.slugAvailable) {
    state.nameCheck.retry()
    state.setStep(2)
    throw Error(
      state.t(
        checked.nameAvailable
          ? "upload.slugConflict"
          : "upload.errors.duplicate"
      )
    )
  }
  if (!state.active.current) return
  await props.onPublish({
    slug: checked.slug,
    displayName: state.form.displayName.trim(),
    summary: state.form.summary.trim(),
    category: state.form.category,
    iconUrl: state.form.iconUrl.trim() || null,
    tags: splitTags(state.form.tags),
    audience: state.form.audience,
    version: state.form.version.trim(),
    changelog: state.form.changelog.trim(),
    packageType: folder.packageType,
    dependencies: parseSkillDependencies(state.form.dependencies),
    files: folder.files,
  })
}

async function addVersion(
  props: SkillMarketUploadDialogProps,
  state: UploadFormState
) {
  const folder = state.folder
  if (!folder) return
  await props.onAddVersion({
    id: props.targetSkillId ?? "",
    version: state.form.version.trim(),
    changelog: state.form.changelog.trim(),
    packageType: folder.packageType,
    dependencies: parseSkillDependencies(state.form.dependencies),
    files: folder.files,
  })
}

export function useUploadSubmit(
  props: SkillMarketUploadDialogProps,
  state: UploadFormState,
  reset: () => void
) {
  const runningRef = state.running
  const activeRef = state.active
  const setSubmitting = state.setSubmitting
  const packageType = state.packageType
  const submit = useCallback(() => {
    if (runningRef.current || !state.folder || state.folderMismatch) return
    if (!validUploadForm(state, props)) return
    runningRef.current = true
    setSubmitting(true)
    const request =
      props.mode === "publish"
        ? publishNew(props, state)
        : addVersion(props, state)
    void request
      .then(reset)
      .catch((error: unknown) => {
        if (!activeRef.current) return
        console.error("[market-publishing] submit failed", { packageType })
        toast.error(state.t("upload.publishFailed"), {
          description: toErrorMessage(error),
        })
      })
      .finally(() => {
        runningRef.current = false
        if (activeRef.current) setSubmitting(false)
      })
  }, [activeRef, packageType, props, reset, runningRef, setSubmitting, state])
  return submit
}
