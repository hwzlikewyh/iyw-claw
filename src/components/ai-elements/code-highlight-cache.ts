import type {
  BundledLanguage,
  BundledTheme,
  HighlighterGeneric,
  ThemedToken,
} from "shiki"

export const CODE_TOKEN_CACHE_MAX_ENTRIES = 128
export const CODE_TOKEN_CACHE_MAX_SOURCE_CHARS = 2_000_000
export const CODE_TOKEN_CACHE_TTL_MS = 3 * 60 * 60 * 1000
const PRIMARY_HASH_SEED = 5381
const SECONDARY_HASH_SEED = 52711
const PRIMARY_HASH_MULTIPLIER = 33
const SECONDARY_HASH_MULTIPLIER = 131
const PRIMARY_HASH_MODULUS = 2_147_483_647
const SECONDARY_HASH_MODULUS = 2_147_483_629

export interface TokenizedCode {
  tokens: ThemedToken[][]
  fg: string
  bg: string
}

interface CacheEntry {
  tokenized: TokenizedCode
  sourceChars: number
  lastAccessAt: number
}

interface HighlightRequest {
  cacheKey: string
  code: string
  language: BundledLanguage
}

type SharedHighlighter = HighlighterGeneric<BundledLanguage, BundledTheme>
type HighlightSubscriber = (result: TokenizedCode) => void

const tokenCache = new Map<string, CacheEntry>()
const languageLoads = new Map<BundledLanguage, Promise<void>>()
const highlightTasks = new Map<string, Promise<void>>()
const subscribers = new Map<string, Set<HighlightSubscriber>>()
let cachedSourceChars = 0
let highlighterPromise: Promise<SharedHighlighter> | null = null

function getCacheKey(code: string, language: BundledLanguage): string {
  let primary = PRIMARY_HASH_SEED
  let secondary = SECONDARY_HASH_SEED
  for (let index = 0; index < code.length; index += 1) {
    const codePoint = code.charCodeAt(index)
    primary =
      (primary * PRIMARY_HASH_MULTIPLIER + codePoint) % PRIMARY_HASH_MODULUS
    secondary =
      (secondary * SECONDARY_HASH_MULTIPLIER + codePoint) %
      SECONDARY_HASH_MODULUS
  }
  return `${language}:${code.length}:${primary}:${secondary}`
}

function purgeExpired(now: number): void {
  for (const [key, entry] of tokenCache) {
    if (now - entry.lastAccessAt < CODE_TOKEN_CACHE_TTL_MS) continue
    tokenCache.delete(key)
    cachedSourceChars -= entry.sourceChars
  }
}

function readCache(cacheKey: string, now: number): TokenizedCode | null {
  purgeExpired(now)
  const entry = tokenCache.get(cacheKey)
  if (!entry) return null
  tokenCache.delete(cacheKey)
  entry.lastAccessAt = now
  tokenCache.set(cacheKey, entry)
  return entry.tokenized
}

function enforceCacheLimits(): void {
  while (
    tokenCache.size > CODE_TOKEN_CACHE_MAX_ENTRIES ||
    cachedSourceChars > CODE_TOKEN_CACHE_MAX_SOURCE_CHARS
  ) {
    const oldest = tokenCache.entries().next().value
    if (!oldest) return
    const [key, entry] = oldest
    tokenCache.delete(key)
    cachedSourceChars -= entry.sourceChars
  }
}

function writeCache(request: HighlightRequest, tokenized: TokenizedCode): void {
  const now = Date.now()
  purgeExpired(now)
  if (request.code.length > CODE_TOKEN_CACHE_MAX_SOURCE_CHARS) return
  const previous = tokenCache.get(request.cacheKey)
  if (previous) cachedSourceChars -= previous.sourceChars
  tokenCache.delete(request.cacheKey)
  tokenCache.set(request.cacheKey, {
    tokenized,
    sourceChars: request.code.length,
    lastAccessAt: now,
  })
  cachedSourceChars += request.code.length
  enforceCacheLimits()
}

async function getHighlighter(): Promise<SharedHighlighter> {
  if (highlighterPromise) return highlighterPromise
  highlighterPromise = import("shiki")
    .then(({ createHighlighter }) =>
      createHighlighter({
        langs: [],
        themes: ["github-light", "github-dark"],
      })
    )
    .catch((error) => {
      highlighterPromise = null
      throw error
    })
  return highlighterPromise
}

async function ensureLanguage(
  highlighter: SharedHighlighter,
  language: BundledLanguage
): Promise<void> {
  if (highlighter.getLoadedLanguages().includes(language)) return
  const existing = languageLoads.get(language)
  if (existing) return existing
  const loading = highlighter.loadLanguage(language).finally(() => {
    languageLoads.delete(language)
  })
  languageLoads.set(language, loading)
  return loading
}

async function performHighlight(
  request: HighlightRequest
): Promise<TokenizedCode> {
  const highlighter = await getHighlighter()
  await ensureLanguage(highlighter, request.language)
  const result = highlighter.codeToTokens(request.code, {
    lang: request.language,
    themes: { dark: "github-dark", light: "github-light" },
  })
  return {
    bg: result.bg ?? "transparent",
    fg: result.fg ?? "inherit",
    tokens: result.tokens,
  }
}

function notifySubscribers(cacheKey: string, tokenized: TokenizedCode): void {
  const currentSubscribers = subscribers.get(cacheKey)
  subscribers.delete(cacheKey)
  if (!currentSubscribers) return
  for (const subscriber of currentSubscribers) subscriber(tokenized)
}

function startHighlight(request: HighlightRequest): void {
  if (highlightTasks.has(request.cacheKey)) return
  const task = (async () => {
    try {
      const tokenized = await performHighlight(request)
      writeCache(request, tokenized)
      notifySubscribers(request.cacheKey, tokenized)
    } catch (error) {
      subscribers.delete(request.cacheKey)
      console.error("Failed to highlight code:", error)
    } finally {
      highlightTasks.delete(request.cacheKey)
    }
  })()
  highlightTasks.set(request.cacheKey, task)
}

export function getCachedCodeHighlight(
  code: string,
  language: BundledLanguage
): TokenizedCode | null {
  return readCache(getCacheKey(code, language), Date.now())
}

export function subscribeCodeHighlight(
  code: string,
  language: BundledLanguage,
  subscriber: HighlightSubscriber
): () => void {
  const request = { cacheKey: getCacheKey(code, language), code, language }
  const cached = readCache(request.cacheKey, Date.now())
  if (cached) {
    subscriber(cached)
    return () => {}
  }
  let currentSubscribers = subscribers.get(request.cacheKey)
  if (!currentSubscribers) {
    currentSubscribers = new Set()
    subscribers.set(request.cacheKey, currentSubscribers)
  }
  currentSubscribers.add(subscriber)
  startHighlight(request)
  return () => {
    currentSubscribers.delete(subscriber)
    if (currentSubscribers.size === 0) subscribers.delete(request.cacheKey)
  }
}
