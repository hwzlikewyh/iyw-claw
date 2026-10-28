"use client"

import { useEffect, useMemo, useState, type CSSProperties } from "react"

import { hasArrayFindLast } from "@/lib/array-find-last-compat"

export type AppBoundaryError = Error & { digest?: string }

interface DiagnosticContext {
  appVersion: string
  correlationId: string
  errorName: string
  errorMessage: string | null
  nextDigest: string | null
  route: string
  runtime: ReturnType<typeof readRuntimeDiagnostics>
  stackFrames: string[]
  windowLabel: string
}

interface AppErrorFallbackProps {
  error: AppBoundaryError
  reset: () => void
}

const styles = {
  copyButton: {
    background: "transparent",
    border: "1px solid #94a3b8",
    borderRadius: "6px",
    color: "#0f172a",
    cursor: "pointer",
    fontSize: "14px",
    padding: "8px 12px",
  },
  diagnostics: {
    background: "#e2e8f0",
    fontSize: "12px",
    marginTop: "12px",
    overflowX: "auto",
    padding: "12px",
    whiteSpace: "pre-wrap",
  },
  main: {
    alignItems: "center",
    background: "#f8fafc",
    color: "#0f172a",
    display: "flex",
    fontFamily: "system-ui, sans-serif",
    justifyContent: "center",
    minHeight: "100vh",
    padding: "24px",
  },
  retryButton: {
    background: "#0f172a",
    border: 0,
    borderRadius: "6px",
    color: "#fff",
    cursor: "pointer",
    fontSize: "14px",
    padding: "10px 16px",
  },
  section: { maxWidth: "520px", width: "100%" },
  summary: { cursor: "pointer", fontSize: "14px" },
  supportingText: { color: "#475569", lineHeight: 1.6, margin: "0 0 20px" },
  title: { fontSize: "24px", margin: "0 0 12px" },
  overline: { color: "#64748b", fontSize: "14px", margin: "0 0 8px" },
} satisfies Record<string, CSSProperties>

function currentRoute(): string {
  if (typeof window === "undefined") return "unknown"
  return window.location.pathname || "/"
}

const MAX_DIAGNOSTIC_IDENTIFIER_LENGTH = 80
const MAX_STACK_FRAMES = 12

function sanitizeIdentifier(value: string | undefined): string | null {
  if (!value) return null
  const sanitized = value
    .replace(/[^a-zA-Z0-9_.:-]/g, "_")
    .slice(0, MAX_DIAGNOSTIC_IDENTIFIER_LENGTH)
  return sanitized || null
}

function sanitizeStackFrames(stack: string | undefined): string[] {
  if (!stack) return []
  return stack
    .split(/\r?\n/)
    .filter((line) => /\bat\b|@/.test(line))
    .slice(0, MAX_STACK_FRAMES)
    .map((line) => line.replace(/[?#][^\s)]*/g, "?[redacted]").trim())
}

function safeErrorMessage(error: Error): string | null {
  // 仅输出已知兼容性错误的固定描述，不复制可能含输入内容或凭证的原文。
  if (
    error.name === "TypeError" &&
    /\.findLast is not a function\b/.test(error.message)
  ) {
    return "findLast is not a function"
  }
  return null
}

function readRuntimeDiagnostics() {
  const userAgent = typeof navigator === "undefined" ? "" : navigator.userAgent
  return {
    chromiumVersion:
      /\b(?:Chrome|Chromium)\/([\d.]+)/.exec(userAgent)?.[1] ?? null,
    arrayFindLastAvailable: hasArrayFindLast(),
  }
}

function isTauriDesktop(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window
}

function createCorrelationId(digest: string | undefined): string {
  const safeDigest = sanitizeIdentifier(digest)
  if (safeDigest) return `next-${safeDigest}`
  return `client-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
}

async function readAppVersion(): Promise<string> {
  if (!isTauriDesktop()) return "web"
  try {
    const { getVersion } = await import("@tauri-apps/api/app")
    return await getVersion()
  } catch {
    return "unknown"
  }
}

async function readWindowLabel(): Promise<string> {
  if (!isTauriDesktop()) return "web"
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window")
    return getCurrentWindow().label
  } catch {
    return "unknown"
  }
}

function useDiagnostics(error: AppBoundaryError): DiagnosticContext {
  const details = useMemo(
    () => ({
      correlationId: createCorrelationId(error.digest),
      errorName: sanitizeIdentifier(error.name) ?? "Error",
      errorMessage: safeErrorMessage(error),
      nextDigest: sanitizeIdentifier(error.digest),
      route: currentRoute(),
      runtime: readRuntimeDiagnostics(),
      stackFrames: sanitizeStackFrames(error.stack),
    }),
    [error]
  )
  const [appVersion, setAppVersion] = useState("unknown")
  const [windowLabel, setWindowLabel] = useState("unknown")

  useEffect(() => {
    let disposed = false

    void Promise.all([readAppVersion(), readWindowLabel()]).then(
      ([version, label]) => {
        if (disposed) return
        setAppVersion(version)
        setWindowLabel(label)
        console.error("[app-error-boundary] render failure", {
          ...details,
          appVersion: version,
          event: "render_failure",
          windowLabel: label,
        })
      }
    )

    return () => {
      disposed = true
    }
  }, [details])

  return useMemo<DiagnosticContext>(
    () => ({
      ...details,
      appVersion,
      windowLabel,
    }),
    [appVersion, details, windowLabel]
  )
}

function useDiagnosticCopy(diagnostics: DiagnosticContext) {
  const [copyStatus, setCopyStatus] = useState<"idle" | "copied" | "failed">(
    "idle"
  )

  const copyDiagnostics = () => {
    if (!navigator.clipboard?.writeText) {
      setCopyStatus("failed")
      return
    }
    void navigator.clipboard
      .writeText(JSON.stringify(diagnostics, null, 2))
      .then(() => setCopyStatus("copied"))
      .catch(() => setCopyStatus("failed"))
  }

  return { copyDiagnostics, copyStatus }
}

function DiagnosticDetails({
  diagnostics,
}: {
  diagnostics: DiagnosticContext
}) {
  const { copyDiagnostics, copyStatus } = useDiagnosticCopy(diagnostics)

  return (
    <details style={{ marginTop: "24px" }}>
      <summary style={styles.summary}>诊断信息</summary>
      <pre style={styles.diagnostics}>
        {JSON.stringify(diagnostics, null, 2)}
      </pre>
      <button onClick={copyDiagnostics} style={styles.copyButton} type="button">
        {copyStatus === "copied"
          ? "已复制"
          : copyStatus === "failed"
            ? "复制失败，请手动复制"
            : "复制诊断信息"}
      </button>
    </details>
  )
}

function ErrorFallbackContent({
  diagnostics,
  reset,
}: {
  diagnostics: DiagnosticContext
  reset: () => void
}) {
  return (
    <main style={styles.main}>
      <section aria-labelledby="app-error-title" style={styles.section}>
        <p style={styles.overline}>应用出现异常</p>
        <h1 id="app-error-title" style={styles.title}>
          此页面暂时无法显示
        </h1>
        <p style={styles.supportingText}>
          可以重试加载页面；若问题持续，请使用诊断信息定位。
        </p>
        <button onClick={reset} style={styles.retryButton} type="button">
          重试
        </button>
        <DiagnosticDetails diagnostics={diagnostics} />
      </section>
    </main>
  )
}

export function AppErrorFallback({ error, reset }: AppErrorFallbackProps) {
  const diagnostics = useDiagnostics(error)

  return <ErrorFallbackContent diagnostics={diagnostics} reset={reset} />
}
