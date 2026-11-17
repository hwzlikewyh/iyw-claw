import { useUploadFormActions } from "./use-upload-form-actions"
import { useUploadFormState } from "./use-upload-form-state"
import { useUploadSubmit } from "./use-upload-submit"
import type { SkillMarketUploadDialogProps } from "./upload-form"

export function useSkillUploadForm(props: SkillMarketUploadDialogProps) {
  const state = useUploadFormState(props)
  const actions = useUploadFormActions(props, state)
  const submit = useUploadSubmit(props, state, actions.reset)
  return {
    ...state,
    ...actions,
    submit,
    isPublish: props.mode === "publish",
  }
}
