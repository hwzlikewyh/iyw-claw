import type {
  BundledLanguage,
  BundledTheme,
  HighlighterGeneric,
  TokensResult,
} from "shiki"

export interface SharedHighlightRequest {
  code: string
  language: string
  themes: { light: string; dark: string }
}

type SharedHighlighter = HighlighterGeneric<BundledLanguage, BundledTheme>
type HighlightConsumer = () => boolean
interface PendingHighlight {
  consumers: Set<HighlightConsumer>
  result: Promise<TokensResult | null>
}

const resourceLoads = new Map<string, Promise<void>>()
const pendingHighlights = new Map<string, Map<string, PendingHighlight>>()
let highlighterPromise: Promise<SharedHighlighter> | null = null

export function getHighlightContextKey(
  request: SharedHighlightRequest
): string {
  return JSON.stringify([request.language, request.themes])
}

async function getHighlighter(): Promise<SharedHighlighter> {
  if (highlighterPromise) return highlighterPromise
  highlighterPromise = import("shiki")
    .then(({ createHighlighter }) =>
      createHighlighter({ langs: [], themes: ["github-light", "github-dark"] })
    )
    .catch((error) => {
      highlighterPromise = null
      throw error
    })
  return highlighterPromise
}

function loadResource(
  highlighter: SharedHighlighter,
  kind: "language" | "theme",
  name: string
): Promise<void> {
  const loaded =
    kind === "language"
      ? highlighter.getLoadedLanguages()
      : highlighter.getLoadedThemes()
  if (loaded.includes(name)) return Promise.resolve()
  const key = JSON.stringify([kind, name])
  const existing = resourceLoads.get(key)
  if (existing) return existing
  const loading = Promise.resolve()
    .then(() =>
      kind === "language"
        ? highlighter.loadLanguage(name as BundledLanguage)
        : highlighter.loadTheme(name as BundledTheme)
    )
    .finally(() => resourceLoads.delete(key))
  resourceLoads.set(key, loading)
  return loading
}

function hasConsumer(consumers: Set<HighlightConsumer>): boolean {
  for (const consumer of consumers) {
    if (consumer()) return true
  }
  return false
}

async function performHighlight(
  request: SharedHighlightRequest,
  consumers: Set<HighlightConsumer>
): Promise<TokensResult | null> {
  const highlighter = await getHighlighter()
  if (!hasConsumer(consumers)) return null
  await Promise.all([
    loadResource(highlighter, "language", request.language),
    ...Object.values(request.themes).map((theme) =>
      loadResource(highlighter, "theme", theme)
    ),
  ])
  if (!hasConsumer(consumers)) return null
  return highlighter.codeToTokens(request.code, {
    lang: request.language as BundledLanguage,
    themes: request.themes,
  })
}

function getPendingHighlight(
  request: SharedHighlightRequest,
  consumer: HighlightConsumer
): PendingHighlight {
  const contextKey = getHighlightContextKey(request)
  let byCode = pendingHighlights.get(contextKey)
  if (!byCode) {
    byCode = new Map()
    pendingHighlights.set(contextKey, byCode)
  }
  const existing = byCode.get(request.code)
  if (existing) {
    existing.consumers.add(consumer)
    return existing
  }
  const consumers = new Set([consumer])
  // 原文直接作为键，精确比较且不拼接、复制全文；任务结束后释放引用。
  const result = performHighlight(request, consumers).finally(() => {
    byCode.delete(request.code)
    if (byCode.size === 0) pendingHighlights.delete(contextKey)
    consumers.clear()
  })
  const pending = { consumers, result }
  byCode.set(request.code, pending)
  return pending
}

export function highlightCode(
  request: SharedHighlightRequest
): Promise<TokensResult>
export function highlightCode(
  request: SharedHighlightRequest,
  consumer: HighlightConsumer
): Promise<TokensResult | null>
export async function highlightCode(
  request: SharedHighlightRequest,
  consumer: HighlightConsumer = () => true
): Promise<TokensResult | null> {
  const snapshot = { ...request, themes: { ...request.themes } }
  const result = await getPendingHighlight(snapshot, consumer).result
  // 无人消费而跳过计算后，可能恰好有新订阅加入，交给下一次任务处理。
  if (result === null && consumer()) return highlightCode(snapshot, consumer)
  return result
}
