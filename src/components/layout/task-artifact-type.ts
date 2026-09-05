import type { TaskArtifactInfo } from "@/lib/api"
import { isImageFile, languageFromPath } from "@/lib/language-detect"

export type ArtifactVisualKind =
  | "archive"
  | "audio"
  | "code"
  | "data"
  | "database"
  | "document"
  | "file"
  | "folder"
  | "font"
  | "image"
  | "link"
  | "spreadsheet"
  | "video"

const VIDEO_EXTENSIONS = new Set([
  "3gp",
  "avi",
  "m4v",
  "mkv",
  "mov",
  "mp4",
  "ogv",
  "webm",
])
const AUDIO_EXTENSIONS = new Set([
  "aac",
  "flac",
  "m4a",
  "mp3",
  "oga",
  "ogg",
  "opus",
  "wav",
  "wma",
])
const DATA_EXTENSIONS = new Set([
  "cfg",
  "conf",
  "env",
  "gql",
  "graphql",
  "hcl",
  "ini",
  "json",
  "json5",
  "jsonc",
  "jsonl",
  "ndjson",
  "plist",
  "proto",
  "properties",
  "tf",
  "tfvars",
  "toml",
  "xml",
  "xsd",
  "xsl",
  "yaml",
  "yml",
])
const DOCUMENT_EXTENSIONS = new Set([
  "doc",
  "docx",
  "epub",
  "md",
  "markdown",
  "odt",
  "pdf",
  "ppt",
  "pptx",
  "rst",
  "rtf",
  "txt",
])
const SPREADSHEET_EXTENSIONS = new Set(["csv", "ods", "tsv", "xls", "xlsx"])
const ARCHIVE_EXTENSIONS = new Set([
  "7z",
  "bz2",
  "cab",
  "gz",
  "iso",
  "lz",
  "lzma",
  "rar",
  "tar",
  "tgz",
  "xz",
  "zip",
  "zst",
])
const FONT_EXTENSIONS = new Set(["eot", "otf", "ttf", "woff", "woff2"])
const DATABASE_EXTENSIONS = new Set([
  "db",
  "duckdb",
  "pgsql",
  "sqlite",
  "sqlite3",
  "sql",
])

export function artifactVisualKind(item: TaskArtifactInfo): ArtifactVisualKind {
  if (item.kind === "directory") return "folder"

  const visualPath = artifactVisualPath(item)
  const extension = artifactExtension(visualPath)
  if (isImageFile(visualPath)) return "image"
  if (item.kind === "url") return "link"
  if (VIDEO_EXTENSIONS.has(extension)) return "video"
  if (AUDIO_EXTENSIONS.has(extension)) return "audio"
  if (DATABASE_EXTENSIONS.has(extension)) return "database"
  if (SPREADSHEET_EXTENSIONS.has(extension)) return "spreadsheet"
  if (ARCHIVE_EXTENSIONS.has(extension)) return "archive"
  if (FONT_EXTENSIONS.has(extension)) return "font"
  if (DOCUMENT_EXTENSIONS.has(extension)) return "document"
  if (DATA_EXTENSIONS.has(extension)) return "data"
  if (languageFromPath(item.path) !== "plaintext") return "code"
  return "file"
}

function artifactVisualPath(item: TaskArtifactInfo): string {
  if (item.kind !== "url") return item.path
  try {
    const pathname = new URL(item.path).pathname
    return artifactExtension(pathname) ? pathname : item.displayName || pathname
  } catch {
    return item.displayName || item.path
  }
}

function artifactExtension(path: string): string {
  return (
    path
      .split(/[./\\]/)
      .pop()
      ?.toLowerCase() ?? ""
  )
}
