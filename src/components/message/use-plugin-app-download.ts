import { useCallback, useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { getTransport } from "@/lib/transport"
import {
  parsePluginDownload,
  savePluginDownload,
  type PluginDownloadFile,
  type PluginDownloadHandler,
  type PluginDownloadReply,
} from "@/lib/plugin-app-download"

const DOWNLOAD_REQUEST_TTL_MS = 3 * 60 * 1000
const DOWNLOAD_ERRORS = [
  "singleFile",
  "inlineOnly",
  "invalidData",
  "invalidName",
  "tooLarge",
] as const
export interface PendingPluginDownload {
  file: PluginDownloadFile
  authorize: () => Promise<boolean>
  resolve: (reply: PluginDownloadReply) => void
  timer: ReturnType<typeof setTimeout>
  transport: ReturnType<typeof getTransport>
  saving: boolean
}
const cancelled = (): PluginDownloadReply => ({
  accepted: true,
  result: { isError: true },
})

export function usePluginAppDownload() {
  const [pending, setPending] = useState<PendingPluginDownload | null>(null)
  const [busy, setBusy] = useState(false),
    [error, setError] = useState("")
  const activeRef = useRef<PendingPluginDownload | null>(null)
  const t = useTranslations("PluginDownloads")
  const finish = useCallback((reply: PluginDownloadReply) => {
    const request = activeRef.current
    if (!request) return
    activeRef.current = null
    clearTimeout(request.timer)
    request.resolve(reply)
    setPending(null)
    setBusy(false)
  }, [])
  const download = useDownloadReceiver({
    activeRef,
    finish,
    setPending,
    setError,
    t,
  })
  useEffect(
    () => () => {
      const request = activeRef.current
      activeRef.current = null
      if (request) {
        clearTimeout(request.timer)
        request.resolve(cancelled())
      }
    },
    []
  )
  const confirm = () =>
    completeDownload({
      request: activeRef.current,
      current: () => activeRef.current,
      finish,
      setBusy,
      fail: () => setError(t("failed")),
    })
  return {
    pending,
    busy,
    error,
    download,
    confirm,
    cancel: () => finish(cancelled()),
  }
}

function useDownloadReceiver(input: {
  activeRef: React.RefObject<PendingPluginDownload | null>
  finish: (reply: PluginDownloadReply) => void
  setPending: (value: PendingPluginDownload | null) => void
  setError: (value: string) => void
  t: ReturnType<typeof useTranslations<"PluginDownloads">>
}) {
  const { activeRef, finish, setPending, setError, t } = input
  const download: PluginDownloadHandler = useCallback(
    async ({ params, authorize, transport }) => {
      if (activeRef.current)
        return { accepted: false, error: { code: -32000, message: t("busy") } }
      let file
      try {
        file = parsePluginDownload(params)
      } catch (cause) {
        const key =
          DOWNLOAD_ERRORS.find(
            (key) =>
              cause instanceof Error &&
              cause.message === `PluginDownloads.${key}`
          ) ?? "invalidData"
        return {
          accepted: false,
          error: {
            code: -32602,
            message: t(key),
          },
        }
      }
      setError("")
      return new Promise((resolve) => {
        const timer = setTimeout(
          () => finish(cancelled()),
          DOWNLOAD_REQUEST_TTL_MS
        )
        const request = {
          file,
          authorize,
          transport,
          resolve,
          timer,
          saving: false,
        }
        activeRef.current = request
        setPending(request)
      })
    },
    [activeRef, finish, setPending, setError, t]
  )
  return download
}

async function completeDownload(input: {
  request: PendingPluginDownload | null
  current: () => PendingPluginDownload | null
  finish: (reply: PluginDownloadReply) => void
  setBusy: (busy: boolean) => void
  fail: () => void
}) {
  const { request } = input
  if (!request || request.saving) return
  request.saving = true
  clearTimeout(request.timer)
  input.setBusy(true)
  const authorize = async () =>
    input.current() === request &&
    request.transport === getTransport() &&
    (await request.authorize()) &&
    input.current() === request
  try {
    if (!(await authorize())) {
      input.finish(cancelled())
      return
    }
    const status = await savePluginDownload(request.file, authorize)
    if (input.current() !== request) return
    input.finish({
      accepted: true,
      result: {
        isError: status === "cancelled",
        _meta: { "iyw/download": { status } },
      },
    })
  } catch {
    if (input.current() === request) {
      request.saving = false
      input.setBusy(false)
      input.fail()
    }
  }
}
