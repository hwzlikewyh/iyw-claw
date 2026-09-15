"use client"

import { useEffect, useMemo, useState } from "react"
import { useTranslations } from "next-intl"
import { FileWarning, Loader2 } from "lucide-react"

import { openSettingsWindow } from "@/lib/api"
import { usePreviewVisibility } from "./use-preview-resource"
import { PreviewToolbar } from "./preview-toolbar"
import {
  isDesktop,
  isRemoteDesktopMode,
  getServerBaseUrl,
  getTransport,
} from "@/lib/transport"
import { extractAppCommandError } from "@/lib/app-error"

// Machine code the backend stamps into `i18n_params.watchCode` for a missing
// officecli (mirrors `WatchError::NotInstalled.code()` in office_watch/mod.rs).
const NOT_INSTALLED = "NOT_INSTALLED"
const FILE_NOT_READY = "FILE_NOT_READY"

// One-liner that installs OfficeCLI on the *server* host — shown to web/remote
// users, for whom an "open Settings" desktop link would point at the wrong
// machine. Mirrors the command the backend's own installer runs
// (`commands/office_tools.rs`).
const SERVER_INSTALL_CMD =
  "curl -fsSL https://raw.githubusercontent.com/iOfficeAI/OfficeCLI/main/install.sh | bash"

function watchCodeOf(err: unknown): string | null {
  return extractAppCommandError(err)?.i18n_params?.watchCode ?? null
}

// 可见预览持有会话；最后一个使用者退出后释放 OfficeCLI 进程。
export function OfficePreview(props: {
  rootPath: string | null
  relPath: string | null
}) {
  const { ref, visible } = usePreviewVisibility()
  const [zoom, setZoom] = useState(1)
  return (
    <div ref={ref} className="flex h-full min-h-0 flex-col">
      <PreviewToolbar
        zoom={zoom}
        onZoom={setZoom}
        onFullscreen={() =>
          void ref.current?.requestFullscreen().catch(() => {})
        }
      />
      <div className="min-h-0 flex-1 overflow-auto">
        <div className="h-full w-full" style={{ zoom }}>
          {visible && <ActiveOfficePreview {...props} />}
        </div>
      </div>
    </div>
  )
}

function ActiveOfficePreview({
  rootPath,
  relPath,
}: {
  // Backend watch target as a (directory, relative file) pair — the file
  // tab's absolute path split by the panel. No workspace folder needed.
  rootPath: string | null
  relPath: string | null
}) {
  const t = useTranslations("Folder.fileWorkspacePanel")
  const [port, setPort] = useState<number | null>(null)
  const [cap, setCap] = useState<string>("")
  const [errorCode, setErrorCode] = useState<string | null>(null)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [retryKey, setRetryKey] = useState(0)

  const path = relPath ?? ""

  // True only for a local desktop window: it loads the watch's loopback URL
  // directly. Web windows go through the proxy.
  const loopbackDirect = isDesktop() && !isRemoteDesktopMode()
  // A Tauri window bound to a remote server can't load the preview at all: its
  // webview is a secure context that mixed-content-blocks the remote `http://`
  // proxy URL, and (unlike JSON calls) a raw iframe can't be tunnelled through
  // Rust. Rather than spawn a remote watch nobody can see, we show a hint to
  // open the preview in the server's web UI instead.
  const remoteDesktop = isDesktop() && isRemoteDesktopMode()

  useEffect(() => {
    const root = rootPath ?? ""
    if (!root || !path || remoteDesktop) return
    const transport = getTransport()
    const id = crypto.randomUUID()
    let disposed = false
    const release = () =>
      void transport.call("close_office_preview", { id }).catch(() => {
        console.warn(
          "[office-preview] release failed; session lease will expire"
        )
      })
    const timer = setInterval(() => {
      void transport.call("renew_office_preview", { id }).catch(() => {
        if (!disposed) setErrorCode("SESSION_EXPIRED")
      })
    }, 30_000)
    void transport
      .call<{ port: number; cap: string }>("open_office_preview", {
        id,
        rootPath: root,
        path,
      })
      .then((res) => {
        if (disposed) {
          release()
          return
        }
        setPort(res.port)
        setCap(res.cap)
        setErrorCode(null)
        setErrorMessage(null)
      })
      .catch((err) => {
        if (disposed) return
        clearInterval(timer)
        release()
        setErrorCode(watchCodeOf(err) ?? "START_FAILED")
        setErrorMessage(extractAppCommandError(err)?.message ?? String(err))
      })
    return () => {
      disposed = true
      clearInterval(timer)
      release()
    }
  }, [path, rootPath, retryKey, remoteDesktop])

  const watchUrl = useMemo(() => {
    if (port == null) return null
    if (loopbackDirect) {
      // The Tauri webview loads loopback directly (no mixed-content: the host
      // page is tauri://localhost, also loopback).
      return `http://127.0.0.1:${port}/`
    }
    // Web: route through the reverse proxy on the server that backs this
    // window, carrying the per-watch capability the proxy validates.
    const base = getServerBaseUrl()
    return `${base}/api/office-watch-proxy/${port}/?cap=${encodeURIComponent(cap)}`
  }, [port, cap, loopbackDirect])

  const retry = () => {
    setErrorCode(null)
    setErrorMessage(null)
    setPort(null)
    setCap("")
    setRetryKey((k) => k + 1)
  }

  if (remoteDesktop) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
        <FileWarning className="h-8 w-8 text-muted-foreground" />
        <div className="text-sm font-medium text-foreground">
          {t("officePreviewTitle")}
        </div>
        <div className="max-w-sm text-xs text-muted-foreground">
          {t("officeRemoteDesktopUnsupported")}
        </div>
      </div>
    )
  }

  if (errorCode === NOT_INSTALLED) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
        <FileWarning className="h-8 w-8 text-muted-foreground" />
        <div className="text-sm font-medium text-foreground">
          {t("officeNotInstalled")}
        </div>
        {loopbackDirect ? (
          <>
            <div className="max-w-sm text-xs text-muted-foreground">
              {t("officeNotInstalledHint")}
            </div>
            <button
              type="button"
              onClick={() => {
                openSettingsWindow().catch(() => {})
              }}
              className="mt-1 rounded-md border border-border bg-card px-3 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-primary/8"
            >
              {t("officeOpenSettings")}
            </button>
          </>
        ) : (
          <>
            {/* Web/remote: officecli must be installed on the server host, not
                the user's machine — show the exact command to run there. */}
            <div className="max-w-sm text-xs text-muted-foreground">
              {t("officeServerInstallHint")}
            </div>
            <code className="block max-w-sm select-all rounded-md bg-muted px-3 py-2 text-left text-[11px] text-foreground">
              {SERVER_INSTALL_CMD}
            </code>
            <button
              type="button"
              onClick={retry}
              className="mt-1 rounded-md border border-border bg-card px-3 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-primary/8"
            >
              {t("officeWatchRetry")}
            </button>
          </>
        )}
      </div>
    )
  }

  if (errorCode === FILE_NOT_READY) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
        <FileWarning className="h-8 w-8 text-muted-foreground" />
        <div className="text-sm font-medium text-foreground">
          {t("officeFileNotReady")}
        </div>
        <button
          type="button"
          onClick={retry}
          className="mt-1 rounded-md border border-border bg-card px-3 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-primary/8"
        >
          {t("officeWatchRetry")}
        </button>
      </div>
    )
  }

  if (errorCode) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
        <FileWarning className="h-8 w-8 text-muted-foreground" />
        <div className="text-sm font-medium text-foreground">
          {t("officeWatchFailed")}
        </div>
        {errorMessage && (
          <div className="max-w-sm break-words text-xs text-muted-foreground">
            {errorMessage}
          </div>
        )}
        <button
          type="button"
          onClick={retry}
          className="mt-1 rounded-md border border-border bg-card px-3 py-1.5 text-xs font-medium text-foreground transition-colors hover:bg-primary/8"
        >
          {t("officeWatchRetry")}
        </button>
      </div>
    )
  }

  // No filename header here — the workspace tab already shows the file name.
  return (
    <div className="relative h-full min-h-0">
      {watchUrl == null ? (
        <div className="flex h-full items-center justify-center gap-2 text-xs text-muted-foreground">
          <Loader2 className="h-4 w-4 animate-spin" />
          {t("loading")}
        </div>
      ) : (
        <iframe
          title={t("officePreviewTitle")}
          src={watchUrl}
          // Desktop loopback keeps its real (loopback) origin so officecli's
          // own same-origin SSE works. Web/proxy mode runs opaque-origin (no
          // allow-same-origin) so the page can't read the app's storage; its
          // sub-requests are rewritten + CORS-allowed by the proxy.
          sandbox={
            loopbackDirect
              ? "allow-scripts allow-same-origin allow-popups allow-forms"
              : "allow-scripts allow-popups allow-forms"
          }
          // The proxy URL carries the watch capability; never let it ride a
          // Referer to any external resource officecli's page might load. The
          // injected shim routes officecli's own requests by absolute path, so
          // it doesn't depend on Referer.
          referrerPolicy="no-referrer"
          className="absolute inset-0 h-full w-full border-0 bg-white"
        />
      )}
    </div>
  )
}
