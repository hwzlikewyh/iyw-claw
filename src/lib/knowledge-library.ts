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
  fileName: string
  folderId: string
  fileSize: number
  fileExt: string
  uploadEmployName: string
  updatedAt: string
  docProcessStatus: number | null
  docProcessMessage: string
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
  input: Pick<LibraryTarget, "scope" | "category"> & { fileIds: string[] }
) {
  return transport.call<{ list: RefreshedFile[] }>(
    "knowledge_library_refresh",
    { input }
  )
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
