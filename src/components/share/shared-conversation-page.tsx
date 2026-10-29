"use client"

import { useCallback, useEffect, useState } from "react"
import { ExternalLink, RefreshCw } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  parseShareLocation,
  readShare,
  shareFileURL,
} from "@/lib/conversation-share/client"
import type { ShareLocation, ShareRecord } from "@/lib/conversation-share/types"
import { ShareThread } from "./share-thread"

export function SharedConversationPage() {
  const [share, setShare] = useState<ShareRecord | null>(null)
  const [location, setLocation] = useState<ShareLocation | null>(null)
  const [error, setError] = useState("")
  const [revision, setRevision] = useState(0)
  useEffect(() => {
    const changed = () => {
      setShare(null)
      setError("")
      setRevision((value) => value + 1)
    }
    window.addEventListener("hashchange", changed)
    return () => window.removeEventListener("hashchange", changed)
  }, [])
  useEffect(() => {
    const controller = new AbortController()
    void (async () => {
      try {
        const target = parseShareLocation()
        await Promise.resolve()
        if (controller.signal.aborted) return
        setLocation(target)
        readShare(target, controller.signal)
          .then((value) => {
            if (!controller.signal.aborted) setShare(value)
          })
          .catch((err) => {
            if (!controller.signal.aborted) setError(err.message)
          })
      } catch (err) {
        if (!controller.signal.aborted)
          setError(err instanceof Error ? err.message : "分享链接无效")
      }
    })()
    return () => controller.abort()
  }, [revision])
  useShareValidity(share, location, () => {
    setShare(null)
    setError("分享已过期或被撤销")
  })
  return (
    <main className="h-dvh overflow-y-auto overscroll-contain bg-background text-foreground">
      <header className="sticky top-0 z-10 border-b bg-background px-4 py-3">
        <div className="mx-auto flex max-w-4xl flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <p className="text-xs text-muted-foreground">
              原助理 · 只读会话分享
            </p>
            <h1 className="truncate text-lg font-semibold">
              {share?.title || "会话分享"}
            </h1>
          </div>
          {share && location && <OpenAssistant location={location} />}
        </div>
      </header>
      {error ? (
        <div className="mx-auto max-w-4xl space-y-4 px-4 py-12">
          <p role="alert">{error}</p>
          <Button
            variant="outline"
            onClick={() => {
              setError("")
              setShare(null)
              setRevision((value) => value + 1)
            }}
          >
            <RefreshCw className="mr-2 size-4" />
            重试
          </Button>
        </div>
      ) : share && location ? (
        <>
          <p className="mx-auto max-w-4xl px-4 pt-4 text-xs text-muted-foreground">
            发布时的内容快照 ·{" "}
            {share.expiresAt
              ? `有效至 ${new Date(share.expiresAt).toLocaleString()}`
              : "长期有效"}
          </p>
          <ShareThread
            snapshot={share.snapshot}
            files={share.files}
            fileURL={(reference, download) =>
              shareFileURL(location, reference, download)
            }
          />
        </>
      ) : (
        <p
          role="status"
          className="py-12 text-center text-sm text-muted-foreground"
        >
          正在加载分享…
        </p>
      )}
    </main>
  )
}

function OpenAssistant({ location }: { location: ShareLocation }) {
  const [attempted, setAttempted] = useState(false)
  const open = () => {
    const link = new URL("iyw-assistant://share")
    link.hash = new URLSearchParams({
      token: location.token,
      api: location.apiBase,
    }).toString()
    window.location.assign(link.toString())
    setAttempted(true)
  }
  return (
    <div className="space-y-1 text-right">
      <Button onClick={open}>
        <ExternalLink className="mr-2 size-4" />
        一键打开原助理
      </Button>
      {attempted && (
        <p role="status" className="max-w-64 text-xs text-muted-foreground">
          如未打开，请确认已安装新版原助理并允许浏览器唤起；仍可在此网页阅读。
        </p>
      )}
    </div>
  )
}

function useShareValidity(
  share: ShareRecord | null,
  location: ShareLocation | null,
  invalidate: () => void
) {
  const check = useCallback(() => {
    if (!share || !location) return
    if (share.expiresAt && Date.parse(share.expiresAt) <= Date.now()) {
      invalidate()
      return
    }
    const controller = new AbortController()
    void readShare(location, controller.signal).catch((err) => {
      if (/失效|不存在|过期/.test(err.message)) invalidate()
    })
    return () => controller.abort()
  }, [share, location, invalidate])
  useEffect(() => {
    if (!share || !location) return
    const onVisible = () => {
      if (document.visibilityState === "visible") check()
    }
    const timer = window.setInterval(check, 30_000)
    document.addEventListener("visibilitychange", onVisible)
    return () => {
      clearInterval(timer)
      document.removeEventListener("visibilitychange", onVisible)
    }
  }, [share, location, check])
}
