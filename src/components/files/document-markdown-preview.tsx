"use client"

import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
  type ComponentProps,
  type ComponentType,
} from "react"
import { useTranslations } from "next-intl"
import { Streamdown, type Components } from "streamdown"
import { ListTree } from "lucide-react"
import { useStreamdownPlugins } from "@/components/ai-elements/streamdown-plugins"
import { useWorkspaceMarkdownOptions } from "@/components/message/markdown-workspace-links"
import { normalizeMathDelimiters } from "@/components/ai-elements/message"
import { EditableImagePreview } from "@/components/ui/editable-image-preview"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import { Button } from "@/components/ui/button"
import { PreviewToolbar } from "./preview-toolbar"
import {
  usePreviewResource,
  usePreviewVisibility,
} from "./use-preview-resource"
import {
  markdownPreviewPaths,
  PREVIEW_IMAGE_ORIGIN,
} from "./markdown-preview-paths"

interface MarkdownProps {
  content: string
  path: string
  rootPath: string
  truncated?: boolean
  onOpenWorkspace?: () => void
}

export function DocumentMarkdownPreview(props: MarkdownProps) {
  const ref = useRef<HTMLDivElement>(null)
  const [zoom, setZoom] = useState(1)
  const [outline, setOutline] = useState(false)
  const labels = useTranslations("Folder.previewControls")
  const headings = useDocumentHeadings(ref, outline, props.content)
  const navigate = (id: string) =>
    ref.current
      ?.querySelector(
        `#${CSS.escape(id)}, #${CSS.escape(`user-content-${id}`)}`
      )
      ?.scrollIntoView({ block: "start" })
  return (
    <div ref={ref} className="flex h-full min-h-0 flex-col bg-background">
      <PreviewToolbar
        zoom={zoom}
        onZoom={setZoom}
        onFullscreen={() =>
          void ref.current?.requestFullscreen().catch(() => {})
        }
      >
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={labels("outline")}
          title={labels("outline")}
          aria-pressed={outline}
          onClick={() => setOutline((value) => !value)}
        >
          <ListTree className="size-4" />
        </Button>
      </PreviewToolbar>
      <div className="flex min-h-0 flex-1">
        {outline && (
          <DocumentOutline headings={headings} onNavigate={navigate} />
        )}
        <MarkdownBody {...props} zoom={zoom} onNavigate={navigate} />
      </div>
    </div>
  )
}

function useDocumentOptions(props: MarkdownProps) {
  const { content, path, rootPath, onOpenWorkspace } = props
  const normalized = useMemo(() => normalizeMathDelimiters(content), [content])
  const plugins = useStreamdownPlugins(normalized)
  const folders = useAppWorkspaceStore((store) => store.folders)
  const roots = useMemo(
    () =>
      folders.some((folder) => folder.path === rootPath) || !rootPath
        ? folders
        : [...folders, { id: -1, path: rootPath }],
    [folders, rootPath]
  )
  const options = useWorkspaceMarkdownOptions({
    content: normalized,
    documentPath: path,
    roots,
    onOpenWorkspace,
  })
  const remarkPlugins = useMemo(
    () => [
      ...options.remarkPlugins,
      [markdownPreviewPaths, { rootPath, path }] as [
        typeof markdownPreviewPaths,
        { rootPath: string; path: string },
      ],
    ],
    [options.remarkPlugins, rootPath, path]
  )
  const components = useMemo<Components>(
    () => ({
      ...options.components,
      img: (image) => <MarkdownImage {...image} rootPath={rootPath} />,
      a: (anchor) => {
        const Link = options.components.a as ComponentType<ComponentProps<"a">>
        if (anchor.href?.startsWith("#"))
          return <a href={anchor.href}>{anchor.children}</a>
        return <Link {...anchor} />
      },
    }),
    [options.components, rootPath]
  )
  return { normalized, plugins, ...options, remarkPlugins, components }
}

function MarkdownBody(
  props: MarkdownProps & { zoom: number; onNavigate: (id: string) => void }
) {
  const { normalized, ...options } = useDocumentOptions(props)
  const t = useTranslations("Folder.chat.workspaceFiles")
  return (
    <div
      className="min-w-0 flex-1 overflow-auto p-4 sm:p-6"
      onClick={(event) => {
        const link = (event.target as HTMLElement).closest("a[href^='#']")
        const target = link?.getAttribute("href")?.slice(1)
        if (!target) return
        event.preventDefault()
        try {
          props.onNavigate(decodeURIComponent(target))
        } catch {
          props.onNavigate(target)
        }
      }}
    >
      <article
        data-artifact-preview-text
        className="mx-auto max-w-[80ch] break-words [&_pre]:max-w-full [&_pre]:overflow-x-auto [&_table]:block [&_table]:max-w-full [&_table]:overflow-x-auto [&_ol]:list-decimal [&_ol]:pl-6 [&_ul]:list-disc [&_ul]:pl-6 [&_p]:[content-visibility:auto]"
        style={{ zoom: props.zoom, fontSize: "1rem", lineHeight: 1.7 }}
      >
        <Streamdown mode="static" parseIncompleteMarkdown={false} {...options}>
          {normalized}
        </Streamdown>
      </article>
      {props.truncated && (
        <p className="mt-4 border-t pt-3 text-xs text-muted-foreground">
          {t("previewTruncated")}
        </p>
      )}
    </div>
  )
}

function DocumentOutline({
  headings,
  onNavigate,
}: {
  headings: DocumentHeading[]
  onNavigate: (id: string) => void
}) {
  const labels = useTranslations("Folder.previewControls")
  return (
    <nav
      aria-label={labels("outline")}
      className="w-44 shrink-0 overflow-auto border-r p-2 max-sm:w-28"
    >
      {headings.map((heading) => (
        <button
          key={heading.id}
          type="button"
          className="block w-full break-words px-2 py-1 text-left text-xs hover:bg-muted"
          style={{ paddingLeft: `${heading.depth * 6}px` }}
          onClick={() => onNavigate(heading.id)}
        >
          {heading.text}
        </button>
      ))}
    </nav>
  )
}

interface DocumentHeading {
  id: string
  text: string
  depth: number
}

function useDocumentHeadings(
  ref: RefObject<HTMLDivElement | null>,
  enabled: boolean,
  content: string
) {
  const [headings, setHeadings] = useState<DocumentHeading[]>([])
  useEffect(() => {
    const article = ref.current?.querySelector("article")
    if (!enabled || !article) return
    // 复用已渲染的标题和 ID，避免为了目录再次解析整篇 Markdown。
    const update = () => {
      const nodes = article.querySelectorAll(
        ":scope > div > :is(h1,h2,h3,h4,h5,h6)"
      )
      const next = Array.from(nodes, (node) => ({
        id: node.id,
        text: node.textContent ?? "",
        depth: Number(node.tagName.slice(1)),
      }))
      setHeadings((previous) =>
        previous.length === next.length &&
        previous.every(
          (heading, index) =>
            heading.id === next[index].id &&
            heading.text === next[index].text &&
            heading.depth === next[index].depth
        )
          ? previous
          : next
      )
    }
    update()
    const observer = new MutationObserver(update)
    observer.observe(article, { childList: true, subtree: true })
    return () => observer.disconnect()
  }, [ref, enabled, content])
  return headings
}

function MarkdownImage({
  src,
  alt,
  rootPath,
  ...props
}: ComponentProps<"img"> & { rootPath: string }) {
  const value = typeof src === "string" ? src : ""
  if (!value.startsWith(PREVIEW_IMAGE_ORIGIN)) {
    return (
      // eslint-disable-next-line @next/next/no-img-element
      <img
        {...props}
        alt={alt ?? ""}
        src={value}
        loading="lazy"
        className="max-w-full"
      />
    )
  }
  return (
    <LocalMarkdownImage
      path={decodeURIComponent(value.slice(PREVIEW_IMAGE_ORIGIN.length))}
      rootPath={rootPath}
      alt={alt ?? ""}
    />
  )
}

function LocalMarkdownImage({
  path,
  rootPath,
  alt,
}: {
  path: string
  rootPath: string
  alt: string
}) {
  const { ref, visible } = usePreviewVisibility<HTMLSpanElement>()
  const state = usePreviewResource(rootPath, path, visible)
  return (
    <span ref={ref} className="my-3 inline-block min-h-16 max-w-full">
      {state?.resource && visible ? (
        <EditableImagePreview
          src={state.resource.url}
          alt={alt}
          trigger="image"
          imageProps={{ className: "max-w-full" }}
        />
      ) : (
        <span className="text-xs text-muted-foreground">{alt}</span>
      )}
    </span>
  )
}
