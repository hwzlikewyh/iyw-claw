import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import type {
  SkillMarketTranslator,
  SelectedSkillMarketFolder,
} from "@/lib/skill-market"
import type { SkillMarketUploadErrors } from "@/components/skills/skill-market-upload-validation"
import { useSkillPublication } from "./use-skill-publication"
import {
  EMPTY_PUBLISH,
  type PublishForm,
  type SkillMarketUploadDialogProps,
} from "./upload-form"

export interface UploadFormState {
  t: SkillMarketTranslator
  step: number
  setStep: React.Dispatch<React.SetStateAction<number>>
  folder: SelectedSkillMarketFolder | null
  setFolder: React.Dispatch<
    React.SetStateAction<SelectedSkillMarketFolder | null>
  >
  form: PublishForm
  setForm: React.Dispatch<React.SetStateAction<PublishForm>>
  errors: SkillMarketUploadErrors
  setErrors: React.Dispatch<React.SetStateAction<SkillMarketUploadErrors>>
  name: string
  packageType: "skill" | "plugin"
  nameCheck: ReturnType<typeof useSkillPublication>
  busy: boolean
  folderMismatch: boolean
  active: React.MutableRefObject<boolean>
  running: React.MutableRefObject<boolean>
  setSubmitting: React.Dispatch<React.SetStateAction<boolean>>
}

export function useUploadFormState(
  props: SkillMarketUploadDialogProps
): UploadFormState {
  const t = useTranslations("SkillMarketV2") as unknown as SkillMarketTranslator
  const [step, setStep] = useState(1)
  const [folder, setFolder] = useState<SelectedSkillMarketFolder | null>(null)
  const [form, setForm] = useState<PublishForm>(EMPTY_PUBLISH)
  const [errors, setErrors] = useState<SkillMarketUploadErrors>({})
  const [submitting, setSubmitting] = useState(false)
  const active = useRef(true)
  const running = useRef(false)
  useEffect(() => {
    active.current = true
    return () => {
      active.current = false
    }
  }, [])
  const name = form.displayName.trim()
  const packageType = props.packageType ?? "skill"
  const nameCheck = useSkillPublication({
    folderName: folder?.name ?? "",
    displayName: name,
    packageType,
    enabled: props.open && props.mode === "publish",
  })
  return {
    t,
    step,
    setStep,
    folder,
    setFolder,
    form,
    setForm,
    errors,
    setErrors,
    name,
    packageType,
    nameCheck,
    busy: props.busy || submitting,
    folderMismatch: Boolean(
      folder &&
      (folder.packageType === "plugin") !== (props.packageType === "plugin")
    ),
    active,
    running,
    setSubmitting,
  }
}
