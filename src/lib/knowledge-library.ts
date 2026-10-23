import type { Transport } from "@/lib/transport"
import type { KnowledgeCategory, KnowledgeFolder } from "./cloud-knowledge"

export const LIBRARY_PAGE_SIZE = 50
export const LIBRARY_BATCH_LIMIT = 10

export interface LibraryFolders {
  scope: string
  folders: KnowledgeFolder[]
}

export interface LibraryTarget {
  scope: string
  category: KnowledgeCategory
  folderId: string
  folderName: string
}

export interface LibraryFile {
  fileId: string
  docId: string
  fileName: string
  folderId: string
  fileSize: number
  fileExt: string
  uploadEmployName: string
  updatedAt: string
  docProcessStatus: number | null
  docProcessMessage: string
  fileUrl: string
  tosUrl: string
  syncStatus: number | null
  syncMessage: string
  pointNum: number | null
}

export interface LibraryFilePage {
  list: LibraryFile[]
  total: number
}

export function getLibraryFolders(
  transport: Transport,
  category: KnowledgeCategory
) {
  return transport.call<LibraryFolders>("knowledge_library_folders", {
    category,
  })
}

export function getLibraryFiles(
  transport: Transport,
  input: LibraryTarget & { searchText: string; page: number }
) {
  const { scope, category, folderId, searchText, page } = input
  const query = { scope, category, folderId, searchText, page }
  return transport.call<LibraryFilePage>("knowledge_library_files", {
    input: query,
  })
}

export interface RefreshedFile {
  fileId: string
  docProcessStatus: number | null
  docProcessMessage: string
}

export function refreshLibraryFiles(
  transport: Transport,
  input: Pick<LibraryTarget, "scope" | "category"> & {
    fileIds: string[]
    forceRefresh?: boolean
  }
) {
  return transport.call<{ list: RefreshedFile[] }>(
    "knowledge_library_refresh",
    { input }
  )
}

export interface LibraryFileMutation extends LibraryTarget {
  fileId: string
  fileName?: string
  destinationFolderId?: string
}

export function updateLibraryFile(
  transport: Transport,
  input: LibraryFileMutation
) {
  return transport.call<void>("knowledge_library_file_update", {
    input: {
      scope: input.scope,
      category: input.category,
      fileId: input.fileId,
      fileName: input.fileName,
      folderId: input.destinationFolderId,
    },
  })
}

export function deleteLibraryFile(
  transport: Transport,
  input: Pick<LibraryFileMutation, "scope" | "category" | "fileId">
) {
  return transport.call<void>("knowledge_library_file_delete", { input })
}

export interface LibraryPoint {
  pointId: string
  chunkTitle: string
  content: string
  question: string
  chunkType: string
  attachmentLink?: string
  raw: Record<string, unknown>
}

export function getLibraryPoints(
  transport: Transport,
  input: Pick<LibraryTarget, "scope" | "category"> & { fileId: string }
) {
  return transport.call<{ list?: Record<string, unknown>[] }>(
    "knowledge_library_points",
    { input: { ...input, offset: 0, limit: 100 } }
  )
}

export function createLibraryPoint(
  transport: Transport,
  input: Pick<LibraryTarget, "scope" | "category"> & {
    fileId: string
    chunkTitle: string
    content: string
    question: string
  }
) {
  return transport.call<void>("knowledge_library_point_create", {
    input: { ...input, chunkType: "text" },
  })
}

export function updateLibraryPoint(
  transport: Transport,
  input: Pick<LibraryTarget, "scope" | "category"> & {
    fileId: string
    pointId: string
    chunkTitle: string
    content: string
    question: string
  }
) {
  return transport.call<void>("knowledge_library_point_update", { input })
}

export function deleteLibraryPoint(
  transport: Transport,
  input: Pick<LibraryTarget, "scope" | "category"> & {
    fileId: string
    pointId: string
  }
) {
  return transport.call<void>("knowledge_library_point_delete", { input })
}

export function normalizeLibraryPoint(
  raw: Record<string, unknown>
): LibraryPoint {
  const value = (key: string) => String(raw[key] ?? "")
  return {
    pointId: value("pointId") || value("point_id"),
    chunkTitle: value("chunkTitle") || value("chunk_title"),
    content: value("content") || value("origin_text") || value("md_content"),
    question:
      value("question") ||
      value("originalQuestion") ||
      value("original_question"),
    chunkType: value("chunkType") || value("chunk_type"),
    attachmentLink:
      value("attachmentLink") || value("attachment_link") || undefined,
    raw,
  }
}

export function folderPath(
  folders: KnowledgeFolder[],
  id: string
): KnowledgeFolder[] {
  for (const folder of folders) {
    if (folder.folderId === id) return [folder]
    const children = folderPath(folder.children, id)
    if (children.length) return [folder, ...children]
  }
  return []
}

export function isFileProcessing(status: number | null) {
  return status === 1 || status === 3 || status === 6
}

export function libraryFileSize(bytes: number, locale?: string) {
  if (!Number.isFinite(bytes) || bytes < 0) return "—"
  if (bytes === 0) return "0 B"
  const units = ["B", "KB", "MB", "GB"]
  const power = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1
  )
  const value = new Intl.NumberFormat(locale, {
    maximumFractionDigits: power ? 1 : 0,
  }).format(bytes / 1024 ** power)
  return `${value} ${units[power]}`
}
