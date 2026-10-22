import { getTransport } from "@/lib/transport"
import { extractAppCommandError } from "@/lib/app-error"

export const KNOWLEDGE_SELECTION_LIMIT = 32
export const KNOWLEDGE_PAGE_SIZE = 100
export type KnowledgeCategory = 0 | 1

export interface KnowledgeChoice {
  category: KnowledgeCategory
  kind: "folder" | "file"
  id: string
  name: string
  folderId: string
}

export interface KnowledgeSelection {
  scope: string
  revision: number
  selected: KnowledgeChoice[]
}

export interface KnowledgeFolder {
  folderId: string
  folderName: string
  fileCount: number
  children: KnowledgeFolder[]
}

export interface KnowledgeFile {
  fileId: string
  fileName: string
  folderId: string
  docId: string | null
  docProcessStatus: number | null
}

export interface KnowledgeFilePage {
  list: KnowledgeFile[]
  total: number
}

export function getKnowledgeSelection(conversationId: number | null) {
  return getTransport().call<KnowledgeSelection>("knowledge_selection_get", {
    conversationId,
  })
}

export function saveKnowledgeSelection(
  conversationId: number,
  selection: KnowledgeSelection
) {
  return getTransport().call<KnowledgeSelection>("knowledge_selection_set", {
    input: { conversationId, ...selection },
  })
}

export function getKnowledgeFolders(category: KnowledgeCategory) {
  return getTransport().call<KnowledgeFolder[]>("knowledge_folders", {
    category,
  })
}

export function getKnowledgeFiles(
  category: KnowledgeCategory,
  folderId: string,
  page: number
) {
  return getTransport().call<KnowledgeFilePage>("knowledge_files", {
    input: { category, folderId, page },
  })
}

export function knowledgeChoiceKey(choice: KnowledgeChoice) {
  return `${choice.category}:${choice.kind}:${choice.id}`
}

export function knowledgeError(cause: unknown) {
  return (
    extractAppCommandError(cause)?.message ??
    (cause instanceof Error ? cause.message : String(cause))
  )
}
