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
import { useShareComposer } from "./use-share-composer"
import { ShareThread } from "./share-thread"
import { ShareHistory } from "./share-history"
import { FileSelection, ShareFooter } from "./share-composer-controls"

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
