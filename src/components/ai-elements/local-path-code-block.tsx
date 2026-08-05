"use client"

import {
  cloneElement,
  isValidElement,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ComponentProps,
  type ReactElement,
  type ReactNode,
} from "react"
import { Check, Copy } from "lucide-react"

import { copyTextToClipboard } from "@/lib/utils"
import {
  findLocalPathMatches,
  type LocalPathMatch,
} from "@/lib/local-path-links"
import { LocalPathActions } from "./local-path-actions"

type CodeChildProps = {
  children?: ReactNode
  className?: string
  [key: string]: unknown
}

function pathSegments(line: string, matches: LocalPathMatch[]) {
  const segments: ReactNode[] = []
  let cursor = 0
  for (const match of matches) {
    if (match.start > cursor) segments.push(line.slice(cursor, match.start))
    segments.push(
      <LocalPathActions
        key={`${match.start}-${match.path}`}
        path={match.path}
        className="max-w-none"
      >
        {match.path}
      </LocalPathActions>
    )
    cursor = match.end
  }
  if (cursor < line.length) segments.push(line.slice(cursor))
  return segments
}

function CodeBlockHeader({
  language,
  copied,
  onCopy,
}: {
  language: string
  copied: boolean
  onCopy: () => void
}) {
  const CopyIcon = copied ? Check : Copy
  return (
    <div className="flex items-center justify-between bg-muted/80 p-3 text-xs text-muted-foreground">
      <span className="ml-1 font-mono lowercase">{language}</span>
      <button
        type="button"
        title="Copy Code"
        onClick={onCopy}
        className="cursor-pointer p-1 transition-colors hover:text-foreground"
      >
        <CopyIcon aria-hidden="true" size={14} />
      </button>
    </div>
  )
}

function LocalPathCodeBlock({
  code,
  language,
}: {
  code: string
  language: string
}) {
  const [copied, setCopied] = useState(false)
  const copyTimerRef = useRef<number>(0)
  const lines = useMemo(
    () =>
      code
        .replace(/\n+$/, "")
        .split("\n")
        .map((line) => ({
          line,
          matches: findLocalPathMatches(line, { wholeLine: true }),
        })),
    [code]
  )

  const copy = async () => {
    if (!(await copyTextToClipboard(code))) return
    setCopied(true)
    window.clearTimeout(copyTimerRef.current)
    copyTimerRef.current = window.setTimeout(() => setCopied(false), 2000)
  }

  useEffect(() => () => window.clearTimeout(copyTimerRef.current), [])

  return (
    <div
      className="my-4 w-full overflow-hidden rounded-xl border border-border"
      data-language={language}
      data-streamdown="code-block"
    >
      <CodeBlockHeader
        language={language}
        copied={copied}
        onCopy={() => void copy()}
      />
      <pre className="overflow-x-auto border-t border-border p-4 text-sm">
        <code className="font-mono">
          {lines.map(({ line, matches }, index) => (
            <span key={`${index}-${line}`} className="block min-h-[1lh]">
              {pathSegments(line, matches)}
            </span>
          ))}
        </code>
      </pre>
    </div>
  )
}

function localPathCodeData(children: ReactNode) {
  if (!isValidElement<CodeChildProps>(children)) return null
  const code = children.props.children
  if (typeof code !== "string") return null
  const hasPath = code
    .split("\n")
    .some((line) => findLocalPathMatches(line, { wholeLine: true }).length > 0)
  if (!hasPath) return null
  const language =
    children.props.className?.match(/language-([^\s]+)/)?.[1] ?? "text"
  return { code, language }
}

function LocalPathPre({
  children,
}: ComponentProps<"pre"> & { node?: unknown }) {
  const localPathCode = localPathCodeData(children)
  if (localPathCode) return <LocalPathCodeBlock {...localPathCode} />
  if (!isValidElement<CodeChildProps>(children)) return children
  return cloneElement(children as ReactElement<CodeChildProps>, {
    "data-block": "true",
  })
}

export const localPathCodeBlockComponents = {
  pre: LocalPathPre,
}
