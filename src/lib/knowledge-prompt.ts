import type { KnowledgeSelection } from "./cloud-knowledge"
import type { PromptDraft, PromptInputBlock } from "./types"

const SOURCE_PREFIX = "iyw-knowledge://source/"
const SOURCE_MIME_TYPE = "application/vnd.iyw.knowledge-source"

export function isKnowledgeSourceBlock(block: PromptInputBlock) {
  return block.type === "resource_link" && block.uri.startsWith(SOURCE_PREFIX)
}

export function knowledgePrompt(
  draft: PromptDraft,
  selection: KnowledgeSelection | undefined
): PromptDraft {
  if (!selection?.selected.length) return draft
  return {
    ...draft,
    blocks: [
      ...draft.blocks.filter((block) => !isKnowledgeSourceBlock(block)),
      ...selection.selected.map(
        (choice): PromptInputBlock => ({
          type: "resource_link",
          uri: `${SOURCE_PREFIX}${selection.scope}/${choice.category}/${choice.kind}/${choice.id}/${choice.folderId}`,
          name: choice.name,
          mime_type: SOURCE_MIME_TYPE,
        })
      ),
    ],
  }
}

export function knowledgeSelectionFromBlocks(
  blocks: PromptInputBlock[]
): KnowledgeSelection | undefined {
  let selection: KnowledgeSelection | undefined
  for (const block of blocks) {
    if (block.type !== "resource_link" || !isKnowledgeSourceBlock(block))
      continue
    const [scope, category, kind, id, folderId, extra] = block.uri
      .slice(SOURCE_PREFIX.length)
      .split("/")
    if (
      extra !== undefined ||
      !/^[a-f0-9]{64}$/.test(scope) ||
      !/^[01]$/.test(category) ||
      (kind !== "folder" && kind !== "file") ||
      !/^\d{1,128}$/.test(id) ||
      !/^\d{1,128}$/.test(folderId) ||
      (selection && selection.scope !== scope)
    )
      return undefined
    selection ??= { scope, revision: 0, selected: [] }
    selection.selected.push({
      category: category === "0" ? 0 : 1,
      kind,
      id,
      folderId,
      name: block.name,
    })
  }
  return selection
}
