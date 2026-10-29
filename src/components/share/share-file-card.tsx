"use client"

import { useEffect, useState } from "react"
import { Download, ExternalLink, FileText } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { ShareMessageText } from "./share-message-text"
import type { ShareFileSpec } from "@/lib/conversation-share/types"

interface FileProps {
  file: ShareFileSpec
  url?: string
  downloadURL?: string
}

export function ShareFileCard(props: FileProps) {
  const [open, setOpen] = useState(false)
  return (
    <>
      {props.url && /^image\/(?!svg)/.test(props.file.contentType) && (
        <button
          type="button"
          className="max-w-full cursor-zoom-in overflow-hidden rounded-md"
          onClick={() => setOpen(true)}
          aria-label={`预览 ${props.file.fileName}`}
        >
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src={props.url}
            alt={props.file.fileName}
            referrerPolicy="no-referrer"
            className="max-h-80 max-w-full object-contain"
          />
        </button>
      )}
      <div className="flex min-w-0 items-center gap-3 rounded-md border bg-card/40 px-3 py-2">
        <FileText className="size-5 shrink-0 text-muted-foreground" />
        <button
          type="button"
          className="min-w-0 flex-1 text-left"
          onClick={() => setOpen(true)}
          disabled={!props.url}
        >
          <span className="block truncate text-sm font-medium">
            {props.file.fileName}
          </span>
          <span className="text-xs text-muted-foreground">
            {(props.file.sizeBytes / 1024).toFixed(1)} KiB · 预览成果
          </span>
        </button>
        {props.downloadURL && (
          <Button variant="ghost" size="icon-sm" asChild>
            <a
              href={props.downloadURL}
              download={props.file.fileName}
              rel="noreferrer"
              aria-label={`下载 ${props.file.fileName}`}
            >
              <Download className="size-4" />
            </a>
          </Button>
        )}
      </div>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-h-[90dvh] overflow-y-auto sm:max-w-5xl">
          <DialogHeader>
            <DialogTitle>{props.file.fileName}</DialogTitle>
          </DialogHeader>
          {open && props.url && (
            <FilePreview file={props.file} url={props.url} />
          )}
        </DialogContent>
      </Dialog>
    </>
  )
}

function FilePreview({ file, url }: { file: ShareFileSpec; url: string }) {
  if (/^image\/(?!svg)/.test(file.contentType)) {
    return (
      // eslint-disable-next-line @next/next/no-img-element
      <img
        src={url}
        alt={file.fileName}
        referrerPolicy="no-referrer"
        className="max-h-[75dvh] w-full object-contain"
      />
    )
  }
  if (file.contentType.startsWith("video/"))
    return <video src={url} controls className="max-h-[75dvh] w-full" />
  if (file.contentType.startsWith("audio/"))
    return <audio src={url} controls className="w-full" />
  if (file.contentType === "application/pdf")
    return (
      <iframe src={url} title={file.fileName} className="h-[70dvh] w-full" />
    )
  if (/^(text\/(plain|markdown|csv)|application\/json)$/.test(file.contentType))
    return <TextPreview file={file} url={url} />
  return (
    <div className="space-y-3 py-6 text-sm">
      <p>此格式暂不支持在线预览，请下载原件查看。</p>
      <Button variant="outline" asChild>
        <a href={url} download={file.fileName} rel="noreferrer">
          <ExternalLink className="mr-2 size-4" />
          下载原件
        </a>
      </Button>
    </div>
  )
}

function TextPreview({ file, url }: { file: ShareFileSpec; url: string }) {
  const [text, setText] = useState<string | null>(null)
  const [error, setError] = useState("")
  useEffect(() => {
    const controller = new AbortController()
    if (file.sizeBytes > 2 * 1024 * 1024) return
    fetch(url, { signal: controller.signal, referrerPolicy: "no-referrer" })
      .then(async (response) => {
        if (!response.ok) throw new Error("文件预览不可用，分享可能已失效")
        setText(await response.text())
      })
      .catch((err) => {
        if (!controller.signal.aborted) setError(err.message)
      })
    return () => controller.abort()
  }, [file.sizeBytes, url])
  if (file.sizeBytes > 2 * 1024 * 1024) return <p>文本较大，请下载原件查看</p>
  if (error) return <p role="alert">{error}</p>
  if (text === null) return <p role="status">正在加载预览…</p>
  return file.contentType === "text/markdown" ? (
    <ShareMessageText text={text} user={false} />
  ) : (
    <pre className="overflow-auto whitespace-pre-wrap text-sm">{text}</pre>
  )
}
