"use client"

import { useState, type ReactNode } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { useConversationShare } from "./conversation-share-context"
import { FileSelection, ShareFooter } from "./share-composer-controls"
import { ShareHistory } from "./share-history"
import { ShareThread } from "./share-thread"
import { ShareSelectionToolbar } from "./share-selection-toolbar"

export function ConversationSharePanel() {
  const share = useConversationShare()
  if (!share?.state) return null
  return <SharePanelContent share={share} state={share.state} />
}

type Share = NonNullable<ReturnType<typeof useConversationShare>>
type Composer = NonNullable<Share["state"]>

function SharePanelContent({
  share,
  state,
}: {
  share: Share
  state: Composer
}) {
  const [history, setHistory] = useState(false)
  const content = (
    <>
      <ShareHeading
        state={state}
        onHistory={() => setHistory(!history)}
        history={history}
      />
      {history ? (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <ShareHistory
            key={state.result?.record.id}
            conversationId={share.conversationId}
          />
        </div>
      ) : (
        <ShareContent state={state} />
      )}
      {!history &&
        (state.preview ? (
          <ShareFooter state={state} close={share.close} />
        ) : (
          <ShareSelectionToolbar state={state} close={share.close} />
        ))}
    </>
  )
  if (!state.preview && !history)
    return (
      <section
        aria-label="选择分享内容"
        className="mx-auto flex max-h-[40dvh] w-full max-w-4xl shrink-0 flex-col gap-2 px-4 py-3"
      >
        {content}
      </section>
    )
  return (
    <SharePreviewDialog
      state={state}
      history={history}
      close={() => (history ? setHistory(false) : share.close())}
    >
      {content}
    </SharePreviewDialog>
  )
}

function SharePreviewDialog({
  state,
  history,
  close,
  children,
}: {
  state: Composer
  history: boolean
  close: () => void
  children: ReactNode
}) {
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
          <DialogTitle>{history ? "分享记录" : "分享预览"}</DialogTitle>
          <DialogDescription>
            仅分享选中的对话和成果。任何持有链接的人可查看，请先检查敏感信息。
          </DialogDescription>
        </DialogHeader>
        {children}
      </DialogContent>
    </Dialog>
  )
}

function ShareHeading({
  state,
  onHistory,
  history,
}: {
  state: Composer
  onHistory: () => void
  history: boolean
}) {
  return (
    <div className="flex shrink-0 items-center gap-2">
      <Input
        aria-label="分享标题"
        value={state.title}
        maxLength={255}
        onChange={(event) => state.setTitle(event.target.value)}
        disabled={!!state.status || !!state.result}
      />
      <Button
        variant="ghost"
        size="sm"
        disabled={!!state.status}
        onClick={onHistory}
      >
        {history ? "返回分享" : "分享记录"}
      </Button>
      {!state.preview && !history && (
        <select
          aria-label="分享有效期"
          title="分享有效期"
          value={state.days}
          disabled={!!state.status || !!state.result}
          onChange={(event) => state.setDays(Number(event.target.value))}
          className="h-8 shrink-0 rounded-md bg-muted/40 px-2 text-xs"
        >
          {[1, 7, 15, 30, 0].map((days) => (
            <option key={days} value={days}>
              {days ? `${days} 天有效` : "长期有效"}
            </option>
          ))}
        </select>
      )}
    </div>
  )
}

function ShareContent({ state }: { state: Composer }) {
  return (
    <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
      {!state.source && !state.error && (
        <p role="status" className="py-2 text-sm">
          正在加载完整会话和成果…
        </p>
      )}
      {!state.preview && state.source && (
        <>
          <p className="text-xs text-muted-foreground">
            在上方对话中勾选要分享的消息，成果文件单独确认。
          </p>
          <FileSelection state={state} />
        </>
      )}
      {state.preview && state.snapshot && (
        <ShareThread
          snapshot={state.snapshot}
          files={state.preview.specs}
          fileURL={(reference) => state.previewFiles.get(reference)}
        />
      )}
    </div>
  )
}
