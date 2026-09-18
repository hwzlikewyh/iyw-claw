import { bundledLanguages, bundledLanguagesInfo } from "shiki"
import type { BundledTheme } from "shiki"
import type {
  CodeHighlighterPlugin,
  HighlightOptions,
  HighlightResult,
} from "@streamdown/code"
import {
  getHighlightContextKey,
  highlightCode,
  type SharedHighlightRequest,
} from "./shared-code-highlighter"

type HighlightSubscriber = (result: HighlightResult) => void
type HighlightEntry = HighlightResult | Set<HighlightSubscriber>

const aliases = Object.fromEntries(
  bundledLanguagesInfo.flatMap((language) =>
    (language.aliases ?? []).map((alias) => [alias, language.id])
  )
)
const languages = new Set(Object.keys(bundledLanguages))
const defaultThemes: [BundledTheme, BundledTheme] = [
  "github-light",
  "github-dark",
]
// 首批沿用 Streamdown 的结果保留周期，避免回看时新增缓存淘汰和重算。
const highlights = new Map<string, Map<string, HighlightEntry>>()

function normalizeLanguage(language: string): string {
  const normalized = language.trim().toLowerCase()
  return aliases[normalized] ?? normalized
}

function publishHighlight(
  subscribers: Set<HighlightSubscriber>,
  result: HighlightResult
): void {
  for (const subscriber of subscribers) {
    try {
      subscriber(result)
    } catch (error) {
      console.error("[Streamdown Code] Highlight subscriber failed:", error)
    }
  }
  subscribers.clear()
}

function startHighlight(
  request: SharedHighlightRequest,
  cache: Map<string, HighlightEntry>,
  subscribers: Set<HighlightSubscriber>
): void {
  void highlightCode(request)
    .then((result) => {
      cache.set(request.code, result)
      publishHighlight(subscribers, result)
    })
    .catch((error) => {
      cache.delete(request.code)
      if (cache.size === 0) highlights.delete(getHighlightContextKey(request))
      subscribers.clear()
      console.error("[Streamdown Code] Failed to highlight code:", error)
    })
}

function highlight(
  options: HighlightOptions,
  callback?: HighlightSubscriber
): HighlightResult | null {
  const request: SharedHighlightRequest = {
    code: options.code,
    language: normalizeLanguage(options.language),
    themes: { light: options.themes[0], dark: options.themes[1] },
  }
  const contextKey = getHighlightContextKey(request)
  let cache = highlights.get(contextKey)
  if (!cache) {
    cache = new Map()
    highlights.set(contextKey, cache)
  }
  const entry = cache.get(request.code)
  if (entry && !(entry instanceof Set)) return entry
  const subscribers = entry ?? new Set<HighlightSubscriber>()
  if (callback) subscribers.add(callback)
  if (!entry) {
    cache.set(request.code, subscribers)
    startHighlight(request, cache, subscribers)
  }
  return null
}

export const code: CodeHighlighterPlugin = {
  name: "shiki",
  type: "code-highlighter",
  supportsLanguage: (language) => languages.has(normalizeLanguage(language)),
  getSupportedLanguages: () =>
    [...languages] as ReturnType<
      CodeHighlighterPlugin["getSupportedLanguages"]
    >,
  getThemes: () => defaultThemes,
  highlight,
}
