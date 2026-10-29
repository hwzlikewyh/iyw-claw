"use client"

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Checkbox } from "@/components/ui/checkbox"
import { copyTextToClipboard } from "@/lib/utils"
import { useShareComposer } from "./use-share-composer"

type Composer = ReturnType<typeof useShareComposer>

export function FileSelection({ state }: { state: Composer }) {
  const count = state.candidates.filter((file) =>
    state.fileIDs.has(file.reference)
  ).length
  return (
    <details className="mx-auto w-full max-w-4xl px-4 py-2" open>
      <summary className="cursor-pointer text-sm font-medium">
        成果文件（已选 {count}/{state.candidates.length}）
      </summary>
      <p className="my-2 text-xs text-muted-foreground">
        全选对话不会自动上传文件。补充成果归到最后一条所选消息。
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

export function ShareFooter({
  state,
  close,
}: {
  state: Composer
  close: () => void
}) {
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
        <SelectionActions state={state} close={close} />
      )}
    </div>
  )
}

function SelectionActions({
  state,
  close,
}: {
  state: Composer
  close: () => void
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-3">
      <label className="flex items-center gap-2 text-sm">
        <Checkbox
          checked={state.all ? true : state.ids.size ? "indeterminate" : false}
          onCheckedChange={state.toggleAll}
          disabled={!state.source || !!state.preview || !!state.status}
        />
        全选对话
        <span className="text-muted-foreground">已选 {state.ids.size} 条</span>
      </label>
      <PublishActions state={state} close={close} />
    </div>
  )
}

function PublishActions({
  state,
  close,
}: {
  state: Composer
  close: () => void
}) {
  return (
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
        onClick={() => void (state.preview ? state.publish() : state.prepare())}
      >
        {state.preview ? "生成分享链接" : "预览分享"}
      </Button>
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
      <div className="flex flex-wrap gap-2">
        <Input
          value={url}
          readOnly
          aria-label="分享链接"
          className="min-w-0 flex-1"
        />
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
