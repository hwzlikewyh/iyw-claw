import { foldReferenceLinks } from "@/lib/reference-link"
import {
  localFilePromptLabel,
  splitLocalFilePrompts,
} from "@/lib/local-file-prompt"

// 历史标题可能在路径中途被截断，无法还原文件名时移除残留的内部提示。
const INCOMPLETE_LOCAL_FILE_PROMPT =
  /Local (?:file|directory) path \(use filesystem tools, not MCP resources\):[^\r\n]*/g

/** 优先显示正文，仅有文件时显示文件名；引用链接沿用统一的折叠规则。 */
export function formatConversationTitle(
  title: string | null | undefined
): string {
  if (!title) return ""
  const segments = splitLocalFilePrompts(title)
  const prose = segments
    .filter((segment) => segment.type === "text")
    .map((segment) => segment.value)
    .join("")
    .replace(INCOMPLETE_LOCAL_FILE_PROMPT, "")
    .trim()
  const files = segments
    .filter((segment) => segment.type === "file")
    .map((segment) => localFilePromptLabel(segment.path))
    .join(" ")
  return foldReferenceLinks(prose || files)
}
