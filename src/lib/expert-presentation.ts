"use client"

import {
  Bot,
  Bug,
  CheckCheck,
  Download,
  FileCode2,
  FlaskConical,
  GitBranch,
  GitFork,
  GitMerge,
  Hammer,
  Image,
  Lightbulb,
  ListTodo,
  MessageSquareQuote,
  MessageSquareReply,
  PlayCircle,
  Puzzle,
  Sparkles,
  type LucideIcon,
} from "lucide-react"

/**
 * Bundled experts whose `experts.toml` category is `codex_native` are the
 * replacements for Codex CLI's own `.system` skills. They belong to the
 * codex-native managed family and must not surface in Experts UIs.
 */
export const CODEX_NATIVE_CATEGORY = "codex_native"

/**
 * Lucide icons referenced by built-in expert metadata (`ExpertMetadata.icon`
 * is a bare icon name string). Shared by the experts settings page and the
 * welcome-page quick actions so both resolve the same glyphs.
 */
export const EXPERT_ICON_MAP: Record<string, LucideIcon> = {
  Lightbulb,
  ListTodo,
  PlayCircle,
  Bot,
  GitFork,
  GitBranch,
  FlaskConical,
  CheckCheck,
  Bug,
  MessageSquareQuote,
  MessageSquareReply,
  GitMerge,
  Sparkles,
  FileCode2,
  Image,
  Puzzle,
  Hammer,
  Download,
}

/** Resolve an expert's icon name to a Lucide component (Sparkles fallback). */
export function getExpertIcon(name: string | null | undefined): LucideIcon {
  if (name && EXPERT_ICON_MAP[name]) return EXPERT_ICON_MAP[name]
  return Sparkles
}

/**
 * Pick a localized string from an expert metadata dictionary.
 *
 * next-intl locales are lower-case underscored like `zh_cn`; expert metadata
 * dictionaries use BCP47-ish keys like `zh-CN`. Normalize both sides, fall
 * back to the language prefix, then to `en`, then to any value.
 */
export function pickLocalized(
  dict: Record<string, string> | undefined,
  locale: string
): string {
  if (!dict) return ""
  if (dict[locale]) return dict[locale]
  const normalized = locale.replace("_", "-")
  if (dict[normalized]) return dict[normalized]
  const [lang] = normalized.split("-")
  const match = Object.keys(dict).find(
    (key) => key.toLowerCase().split("-")[0] === lang.toLowerCase()
  )
  if (match) return dict[match]
  return dict.en ?? Object.values(dict)[0] ?? ""
}
