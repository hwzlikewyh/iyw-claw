"use client"

import { useState } from "react"
import { Share2 } from "lucide-react"
import type { AgentType } from "@/lib/types"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Checkbox } from "@/components/ui/checkbox"
import { copyTextToClipboard } from "@/lib/utils"
import { useShareComposer } from "./use-share-composer"
import { ShareThread } from "./share-thread"
import { ShareHistory } from "./share-history"

export function ConversationShareButton({
  conversationId,
  agentType,
}: {
  conversationId: number | null
  agentType: AgentType
}) {
  const [open, setOpen] = useState(false)
  if (!conversationId || conversationId <= 0) return null
  return (
    <div className="flex justify-end px-4 py-1">
      <Button size="sm" variant="ghost" onClick={() => setOpen(true)}>
        <Share2 className="mr-1.5 size-4" />
        分享会话
      </Button>
      {open && (
        <ShareDialog
          conversationId={conversationId}
          agentType={agentType}
          close={() => setOpen(false)}
        />
      )}
    </div>
  )
}

function ShareDialog({
  conversationId,
  agentType,
  close,
}: {
  conversationId: number
  agentType: AgentType
  close: () => void
}) {
  const state = useShareComposer(conversationId, agentType)
  const [history, setHistory] = useState(false)
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !state.status) close()
      }}
    >
      <DialogContent
        className="flex h-[94dvh] max-w-[96vw] flex-col overflow-hidden sm:max-w-6xl"
        onEscapeKeyDown={(event) => {
          if (state.status) event.preventDefault()
        }}
      >
        <DialogHeader className="shrink-0 pr-8">
          <DialogTitle>
            {history ? "分享记录" : state.preview ? "分享预览" : "选择分享内容"}
          </DialogTitle>
          <DialogDescription>
            仅分享选中的对话和成果。任何持有链接的人可查看，请先检查敏感信息。
          </DialogDescription>
        </DialogHeader>
        <div className="flex items-center justify-between gap-3">
          <Input
            aria-label="分享标题"
            value={state.title}
            maxLength={255}
            onChange={(event) => state.setTitle(event.target.value)}
            disabled={!!state.status || !!state.result}
          />
          <Button
            variant="ghost"
            disabled={!!state.status}
            onClick={() => setHistory(!history)}
          >
            {history ? "返回分享" : "分享记录"}
          </Button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
          {history ? (
            <ShareHistory
              key={state.result?.record.id}
              conversationId={conversationId}
            />
          ) : (
            <>
              {!state.source && !state.error && (
                <p role="status" className="py-8 text-center">
                  正在加载完整会话和成果…
                </p>
              )}
              {!state.preview && state.source && (
                <FileSelection state={state} />
              )}
              {state.snapshot && (
                <ShareThread
                  snapshot={
                    state.preview ? state.snapshot : state.source!.snapshot
                  }
                  files={state.preview?.specs ?? []}
                  fileURL={(reference) => state.previewFiles.get(reference)}
                  selection={
                    state.preview || state.status
                      ? undefined
                      : { ids: state.ids, toggle: state.toggle }
                  }
                />
              )}
            </>
          )}
        </div>
        {!history && <ShareFooter state={state} close={close} />}
      </DialogContent>
    </Dialog>
  )
}

type Composer = ReturnType<typeof useShareComposer>

function FileSelection({ state }: { state: Composer }) {
  return (
    <details className="mx-auto w-full max-w-4xl px-4 py-2" open>
      <summary className="cursor-pointer text-sm font-medium">
        成果文件（已选{" "}
        {
          state.candidates.filter((file) => state.fileIDs.has(file.reference))
            .length
        }
        /{state.candidates.length}）
      </summary>
      <p className="my-2 text-xs text-muted-foreground">
        全选对话不会自动上传文件。图片和成果需单独勾选；补充文件归到最后一条所选消息。
      </p>
      <div className="space-y-2">
        {state.candidates.map((file) => (
          <label
            key={file.reference}
            className="flex items-center gap-2 text-sm"
          >
            <Checkbox
              checked={state.fileIDs.has(file.reference)}
              disabled={!!state.status}
              onCheckedChange={() => state.toggleFile(file.reference)}
            />
            <span className="truncate">{file.name}</span>
          </label>
        ))}
      </div>
      <label className="my-3 inline-block cursor-pointer text-sm underline">
        上传补充成果
        <input
          type="file"
          multiple
          className="sr-only"
          disabled={!state.ids.size || !!state.status}
          onChange={(event) => {
            if (event.target.files) state.addFiles(event.target.files)
            event.target.value = ""
          }}
        />
      </label>
    </details>
  )
}

function ShareFooter({ state, close }: { state: Composer; close: () => void }) {
  return (
    <div className="shrink-0 space-y-2 border-t pt-3">
      {state.error && (
        <p role="alert" className="text-sm text-destructive">
          {state.error}
          {!state.source && (
            <Button variant="link" onClick={state.retry}>
              重试
            </Button>
          )}
        </p>
      )}
      {state.status && (
        <p role="status" className="text-sm">
          {state.status}
        </p>
      )}
      {state.result ? (
        <ShareSuccess
          url={state.result.url}
          expiresAt={state.result.record.expiresAt}
        />
      ) : (
        <div className="flex flex-wrap items-center justify-between gap-3">
          <label className="flex items-center gap-2 text-sm">
            <Checkbox
              checked={
                state.all ? true : state.ids.size ? "indeterminate" : false
              }
              onCheckedChange={state.toggleAll}
              disabled={!state.source || !!state.preview || !!state.status}
            />
            全选对话
            <span className="text-muted-foreground">
              已选 {state.ids.size} 条
            </span>
          </label>
          <div className="flex flex-wrap items-center gap-2">
            <label className="flex items-center gap-2 text-sm">
              有效期
              <select
                className="h-9 rounded-md border bg-background px-2"
                value={state.days}
                disabled={!!state.status}
                onChange={(event) => state.setDays(Number(event.target.value))}
              >
                {[1, 7, 15, 30, 0].map((days) => (
                  <option key={days} value={days}>
                    {days ? `${days} 天` : "长期"}
                  </option>
                ))}
              </select>
            </label>
            <Button
              variant="ghost"
              disabled={!!state.status}
              onClick={state.preview ? state.back : close}
            >
              {state.preview ? "返回选择" : "取消"}
            </Button>
            <Button
              disabled={
                !!state.status ||
                !state.snapshot?.messages.length ||
                !state.title.trim()
              }
              onClick={() =>
                void (state.preview ? state.publish() : state.prepare())
              }
            >
              {state.preview ? "生成分享链接" : "预览分享"}
            </Button>
          </div>
        </div>
      )}
    </div>
  )
}

function ShareSuccess({
  url,
  expiresAt,
}: {
  url: string
  expiresAt: string | null
}) {
  const [copied, setCopied] = useState(false)
  return (
    <div className="space-y-2">
      <p className="text-sm">
        分享已生成 ·{" "}
        {expiresAt
          ? `到期时间：${new Date(expiresAt).toLocaleString()}`
          : "长期有效，可随时撤销"}
      </p>
      <div className="flex gap-2">
        <Input value={url} readOnly aria-label="分享链接" />
        <Button
          onClick={() =>
            void copyTextToClipboard(url).then((ok) => setCopied(ok))
          }
        >
          {copied ? "已复制" : "复制链接"}
        </Button>
        <Button variant="outline" asChild>
          <a href={url} target="_blank" rel="noreferrer">
            打开分享
          </a>
        </Button>
      </div>
    </div>
  )
}
