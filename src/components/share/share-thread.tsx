"use client"

import { AgentIcon } from "@/components/agent-icon"
import { Message, MessageContent } from "@/components/ai-elements/message"
import { Checkbox } from "@/components/ui/checkbox"
import type {
  ShareFileSpec,
  ShareSnapshot,
} from "@/lib/conversation-share/types"
import { ShareFileCard } from "./share-file-card"
import { ShareMessageText } from "./share-message-text"
import "@/components/message/conversation-presentation.css"

interface ThreadProps {
  snapshot: ShareSnapshot
  files: ShareFileSpec[]
  fileURL?: (reference: string, download?: boolean) => string | undefined
  selection?: { ids: Set<string>; toggle: (id: string) => void }
}

export function ShareThread({
  snapshot,
  files,
  fileURL,
  selection,
}: ThreadProps) {
  return (
    <div className="mx-auto w-full max-w-4xl space-y-5 px-4 py-5">
      {snapshot.messages.map((message) => (
        <div key={message.id} className="relative flex min-w-0 gap-3">
          {selection && (
            <Checkbox
              className="mt-3"
              checked={selection.ids.has(message.id)}
              onCheckedChange={() => selection.toggle(message.id)}
              aria-label={`选择${message.role === "user" ? "提问" : "回答"}`}
            />
          )}
          <Message from={message.role}>
            <MessageContent>
              {message.role === "assistant" && (
                <div className="conversation-assistant-header flex items-center gap-2 text-sm font-semibold">
                  <AgentIcon
                    agentType={snapshot.agentType}
                    className="size-6 rounded-full bg-muted p-0.5"
                  />
                  <span>原助理</span>
                </div>
              )}
              {message.blocks.map((block, index) =>
                block.type === "text" ? (
                  <ShareMessageText
                    key={index}
                    text={block.text || ""}
                    user={message.role === "user"}
                  />
                ) : (
                  (() => {
                    const file = files.find(
                      (candidate) => candidate.reference === block.reference
                    )
                    return file ? (
                      <ShareFileCard
                        key={block.reference}
                        file={file}
                        url={fileURL?.(file.reference)}
                        downloadURL={fileURL?.(file.reference, true)}
                      />
                    ) : null
                  })()
                )
              )}
              {files
                .filter(
                  (file) =>
                    file.messageId === message.id &&
                    !message.blocks.some(
                      (block) => block.reference === file.reference
                    )
                )
                .map((file) => (
                  <ShareFileCard
                    key={file.reference}
                    file={file}
                    url={fileURL?.(file.reference)}
                    downloadURL={fileURL?.(file.reference, true)}
                  />
                ))}
            </MessageContent>
          </Message>
        </div>
      ))}
    </div>
  )
}
