"use client"

import {
  AppErrorFallback,
  type AppBoundaryError,
} from "@/components/app-error-fallback"

export default function Error({
  error,
  reset,
}: {
  error: AppBoundaryError
  reset: () => void
}) {
  return <AppErrorFallback error={error} reset={reset} />
}
