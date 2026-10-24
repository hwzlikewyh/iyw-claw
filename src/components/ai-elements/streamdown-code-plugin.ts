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
import {
  CODE_TOKEN_CACHE_MAX_ENTRIES,
  CODE_TOKEN_CACHE_MAX_SOURCE_CHARS,
} from "./code-highlight-cache"

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
const highlights = new Map<string, Map<string, HighlightEntry>>()
const completed = new Map<
  HighlightResult,
  { contextKey: string; cache: Map<string, HighlightEntry>; code: string }
>()
let cachedSourceChars = 0

function retainHighlight(
  request: SharedHighlightRequest,
  cache: Map<string, HighlightEntry>,
  result: HighlightResult
) {
  if (request.code.length > CODE_TOKEN_CACHE_MAX_SOURCE_CHARS) {
    cache.delete(request.code)
    if (!cache.size) highlights.delete(getHighlightContextKey(request))
    return
  }
  cache.set(request.code, result)
  completed.set(result, {
    contextKey: getHighlightContextKey(request),
    cache,
    code: request.code,
  })
  cachedSourceChars += request.code.length
  // 流式代码的旧版本不再无限保留，沿用现有代码高亮缓存预算。
  while (
    completed.size > CODE_TOKEN_CACHE_MAX_ENTRIES ||
    cachedSourceChars > CODE_TOKEN_CACHE_MAX_SOURCE_CHARS
  ) {
    const oldest = completed.entries().next().value
    if (!oldest) break
    const [value, entry] = oldest
    completed.delete(value)
    cachedSourceChars -= entry.code.length
    if (entry.cache.get(entry.code) === value) entry.cache.delete(entry.code)
    if (!entry.cache.size) highlights.delete(entry.contextKey)
  }
}

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
      retainHighlight(request, cache, result)
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
  if (entry && !(entry instanceof Set)) {
    const retained = completed.get(entry)
    if (retained) {
      completed.delete(entry)
      completed.set(entry, retained)
    }
    return entry
  }
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
