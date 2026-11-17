import {
  hasSkillMarketUploadErrors,
  validateSkillMarketPublishForm,
  validateSkillMarketVersionForm,
} from "@/components/skills/skill-market-upload-validation"
import {
  pluginVersion,
  EMPTY_PUBLISH,
  type PublishForm,
  type SkillMarketUploadDialogProps,
} from "./upload-form"
import type { UploadFormState } from "./use-upload-form-state"

export function useUploadFormActions(
  props: SkillMarketUploadDialogProps,
  state: UploadFormState
) {
  const reset = () => {
    state.setStep(1)
    state.setFolder(null)
    state.setForm(EMPTY_PUBLISH)
    state.setErrors({})
  }
  const close = (open: boolean) => {
    if (state.busy || state.running.current) return
    if (!open) reset()
    props.onOpenChange(open)
  }
  const update = (field: keyof PublishForm, value: string) => {
    state.setForm((current) => ({ ...current, [field]: value }))
    state.setErrors({})
  }
  const validate = () =>
    props.mode === "publish"
      ? validateSkillMarketPublishForm({
          ...state.form,
          slug: state.nameCheck.slug,
        })
      : validateSkillMarketVersionForm(state.form)
  const next = () => {
    if (state.step === 1 && (!state.folder || state.folderMismatch)) return
    if (state.step === 2) {
      const errors = validate()
      if (hasSkillMarketUploadErrors(errors)) {
        state.setErrors(errors)
        return
      }
      if (props.mode === "publish" && state.nameCheck.status !== "available")
        return
    }
    state.setStep((step) => Math.min(3, step + 1))
  }
  const selectFolder = (folder: UploadFormState["folder"]) => {
    state.setFolder(folder)
    state.setErrors({})
    if (!folder) return
    state.setForm((current) => ({
      ...current,
      displayName:
        !current.displayName || current.displayName === state.folder?.name
          ? folder.name
          : current.displayName,
      version: pluginVersion(folder) ?? current.version,
    }))
  }
  return { reset, close, update, validate, next, selectFolder }
}

export function validUploadForm(
  state: UploadFormState,
  props: SkillMarketUploadDialogProps
) {
  const errors =
    props.mode === "publish"
      ? validateSkillMarketPublishForm({
          ...state.form,
          slug: state.nameCheck.slug,
        })
      : validateSkillMarketVersionForm(state.form)
  if (hasSkillMarketUploadErrors(errors)) state.setErrors(errors)
  return !hasSkillMarketUploadErrors(errors)
}
