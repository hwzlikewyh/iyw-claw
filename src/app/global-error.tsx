"use client"

import {
  AppErrorFallback,
  type AppBoundaryError,
} from "@/components/app-error-fallback"

export default function GlobalError({
  error,
  reset,
}: {
  error: AppBoundaryError
  reset: () => void
}) {
  return (
    <html lang="zh-CN">
      <body>
        <AppErrorFallback error={error} reset={reset} />
      </body>
    </html>
  )
}
