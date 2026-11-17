import { useEffect, useState } from "react"
import { toErrorMessage } from "@/lib/app-error"
import {
  prepareSkillPublication,
  type SkillPublicationCheck,
  type SkillPublicationInput,
} from "@/lib/market-publishing"

const CHECK_DELAY_MS = 300
interface CheckResult {
  key: string
  value: SkillPublicationCheck | null
  error: string
}

export interface UseSkillPublicationResult {
  status: string
  slug: string
  error: string
  retry: () => void
}

function loadPublication(
  input: SkillPublicationInput,
  key: string,
  receive: (result: CheckResult) => void
) {
  let active = true
  const timer = window.setTimeout(() => {
    void prepareSkillPublication(input).then(
      (value) => {
        if (active) receive({ key, value, error: "" })
      },
      (cause: unknown) => {
        if (active) receive({ key, value: null, error: toErrorMessage(cause) })
      }
    )
  }, CHECK_DELAY_MS)
  return () => {
    active = false
    window.clearTimeout(timer)
  }
}

function checkStatus(enabled: boolean, current: CheckResult | null) {
  if (!enabled) return "idle"
  if (!current) return "checking"
  if (current.error) return "error"
  if (!current.value?.nameAvailable) return "duplicate"
  if (!current.value.slugAvailable) return "slugConflict"
  return "available"
}

export function useSkillPublication(
  input: SkillPublicationInput & { enabled: boolean }
) {
  const { folderName, displayName, packageType, enabled } = input
  const [revision, setRevision] = useState(0)
  const [result, setResult] = useState<CheckResult | null>(null)
  const key = JSON.stringify([folderName, displayName, packageType, revision])
  useEffect(() => {
    if (!enabled || !folderName) return
    return loadPublication(
      { folderName, displayName, packageType },
      key,
      setResult
    )
  }, [displayName, enabled, folderName, key, packageType])
  const current = result?.key === key ? result : null
  return {
    status: checkStatus(
      enabled && Boolean(folderName && displayName.trim()),
      current
    ),
    slug: current?.value?.slug ?? "",
    error: current?.error ?? "",
    retry: () => setRevision((value) => value + 1),
  }
}
