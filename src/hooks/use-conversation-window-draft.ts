import { useEffect, type RefObject } from "react"
import { useTranslations } from "next-intl"
import type { RichComposerHandle } from "@/components/chat/composer/rich-composer"
import type { InputAttachment } from "@/components/chat/message-input-attachments"
import type { PromptInputBlock } from "@/lib/types"
import { registerConversationWindowDraft } from "@/lib/conversation-window-draft"
import { useKnowledgeComposerContext } from "@/lib/knowledge-composer-context"
import { isKnowledgeSourceBlock, knowledgePrompt } from "@/lib/knowledge-prompt"

interface CaptureOptions {
  tabId?: string | null
  ready: boolean
  editing: boolean
  editor: RefObject<RichComposerHandle | null>
  attachments: RefObject<InputAttachment[]>
  embedded: RefObject<Map<string, PromptInputBlock>>
}

export function useConversationWindowDraft(options: CaptureOptions) {
  const t = useTranslations("ConversationTabs")
  useEffect(() => {
    if (!options.tabId || !options.ready) return
    return registerConversationWindowDraft(options.tabId, () => {
      if (
        options.editing ||
        options.attachments.current.some(
          (item) => item.type === "image" && item.staging
        )
      ) {
        throw new Error(t("busyDraft"))
      }
      const knowledge = knowledgePrompt(
        { blocks: [], displayText: "" },
        useKnowledgeComposerContext.getState().entries[options.tabId!]
          ?.selection
      ).blocks
      return {
        doc: options.editor.current?.getJSON() ?? null,
        attachments: options.attachments.current.map((item) =>
          item.type === "image" ? { ...item, previewUrl: undefined } : item
        ),
        embeddedPayloads: [
          ...Array.from(options.embedded.current.entries()).filter(
            ([, block]) => !isKnowledgeSourceBlock(block)
          ),
          ...knowledge.map((block): [string, PromptInputBlock] => [
            block.type === "resource_link" ? block.uri : "",
            block,
          ]),
        ],
      }
    })
  }, [options, t])
}
