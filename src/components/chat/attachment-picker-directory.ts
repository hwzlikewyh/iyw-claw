import { splitAbsPath } from "@/lib/file-open-target"
import { isAbsoluteFilePath } from "@/lib/file-path-display"

const STORAGE_KEY = "iyw-claw:attachment-picker-directory"

export function attachmentPickerDirectory(
  fallback?: string
): string | undefined {
  try {
    const saved = window.localStorage.getItem(STORAGE_KEY)
    if (saved && isAbsoluteFilePath(saved)) return saved
  } catch {
    // 存储不可用时仍允许选择附件。
  }
  return fallback
}

export function rememberAttachmentDirectory(path: string, directory = false) {
  if (!isAbsoluteFilePath(path)) return
  const value = directory ? path : splitAbsPath(path)?.rootPath
  if (!value) return
  try {
    window.localStorage.setItem(STORAGE_KEY, value)
  } catch {
    console.warn("[attachment-picker] could not remember directory")
  }
}
