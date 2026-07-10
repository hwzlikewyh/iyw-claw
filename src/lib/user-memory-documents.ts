export type UserMemoryDocumentId = "memory" | "profile" | "soul"

export interface UserMemoryDocument {
  id: UserMemoryDocumentId
  fileName: string
  labelKey:
    | "documents.memory.label"
    | "documents.profile.label"
    | "documents.soul.label"
  descriptionKey:
    | "documents.memory.description"
    | "documents.profile.description"
    | "documents.soul.description"
  placeholderKey:
    | "documents.memory.placeholder"
    | "documents.profile.placeholder"
    | "documents.soul.placeholder"
}

export const USER_MEMORY_DIR = ".iyw-claw"

export const USER_MEMORY_DOCUMENTS: UserMemoryDocument[] = [
  {
    id: "memory",
    fileName: "user-memory.md",
    labelKey: "documents.memory.label",
    descriptionKey: "documents.memory.description",
    placeholderKey: "documents.memory.placeholder",
  },
  {
    id: "profile",
    fileName: "user-profile.md",
    labelKey: "documents.profile.label",
    descriptionKey: "documents.profile.description",
    placeholderKey: "documents.profile.placeholder",
  },
  {
    id: "soul",
    fileName: "user-soul.md",
    labelKey: "documents.soul.label",
    descriptionKey: "documents.soul.description",
    placeholderKey: "documents.soul.placeholder",
  },
]

export function getUserMemoryDocument(
  id: UserMemoryDocumentId
): UserMemoryDocument {
  return (
    USER_MEMORY_DOCUMENTS.find((document) => document.id === id) ??
    USER_MEMORY_DOCUMENTS[0]
  )
}

export function userMemoryRelativePath(document: UserMemoryDocument): string {
  return `${USER_MEMORY_DIR}/${document.fileName}`
}

export function displayUserMemoryPath(
  homePath: string | null,
  relativePath: string
): string {
  if (!homePath) return relativePath
  const separator = homePath.includes("\\") ? "\\" : "/"
  const base = homePath.replace(/[\\/]+$/, "")
  return `${base}${separator}${relativePath.replace("/", separator)}`
}

export function userMemoryLineCount(content: string): number {
  return content.length === 0 ? 0 : content.split(/\r\n|\r|\n/).length
}
