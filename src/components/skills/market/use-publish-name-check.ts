import { useEffect, useState } from "react"
import { toErrorMessage } from "@/lib/app-error"

const NAME_CHECK_DELAY_MS = 300
type CheckResult = { key: string; duplicate: boolean; error: string }

export function usePublishNameCheck(
  key: string,
  check: () => Promise<boolean>
) {
  const [result, setResult] = useState<CheckResult | null>(null)
  const [revision, setRevision] = useState(0)
  const requestKey = JSON.stringify([key, revision])
  useEffect(() => {
    if (!key) return
    let active = true
    const timer = window.setTimeout(() => {
      void check().then(
        (duplicate) => {
          if (active) setResult({ key: requestKey, duplicate, error: "" })
        },
        (cause: unknown) => {
          if (active)
            setResult({
              key: requestKey,
              duplicate: false,
              error: toErrorMessage(cause),
            })
        }
      )
    }, NAME_CHECK_DELAY_MS)
    return () => {
      active = false
      window.clearTimeout(timer)
    }
  }, [check, key, requestKey])
  const current = result?.key === requestKey ? result : null
  const status = !key
    ? "idle"
    : !current
      ? "checking"
      : current.error
        ? "error"
        : current.duplicate
          ? "duplicate"
          : "available"
  return {
    status,
    error: current?.error ?? "",
    retry: () => setRevision((value) => value + 1),
  }
}
