"use client"

import type { ComponentProps, ReactNode } from "react"

import { useControllableState } from "@radix-ui/react-use-controllable-state"
import { useTranslations } from "next-intl"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { cn } from "@/lib/utils"
import { BrainIcon, ChevronDownIcon } from "lucide-react"
import {
  createContext,
  memo,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react"
import {
  Streamdown,
  defaultRehypePlugins,
  defaultRemarkPlugins,
} from "streamdown"

import { Shimmer } from "./shimmer"
import { markdownLinkComponents } from "./markdown-link"
import { localPathCodeBlockComponents } from "./local-path-code-block"
import { rehypePluginsAllowingIywClaw } from "./rehype-allow-iyw-claw"
import { normalizeMathDelimiters } from "./message"
import { remarkRewriteFileUriLinks } from "./remark-file-uri-links"
import { remarkLocalPathLinks } from "./remark-local-path-links"
import { useStreamdownPlugins } from "./streamdown-plugins"

interface ReasoningContextValue {
  isStreaming: boolean
  isOpen: boolean
  setIsOpen: (open: boolean) => void
  duration: number | undefined
  expandable: boolean
}

const ReasoningContext = createContext<ReasoningContextValue | null>(null)

export const useReasoning = () => {
  const context = useContext(ReasoningContext)
  if (!context) {
    throw new Error("Reasoning components must be used within Reasoning")
  }
  return context
}

export type ReasoningProps = ComponentProps<typeof Collapsible> & {
  isStreaming?: boolean
  open?: boolean
  defaultOpen?: boolean
  onOpenChange?: (open: boolean) => void
  duration?: number
  expandable?: boolean
}

const AUTO_CLOSE_DELAY = 1000
const MS_IN_S = 1000

export const Reasoning = memo(
  ({
    className,
    isStreaming = false,
    open,
    defaultOpen,
    onOpenChange,
    duration: durationProp,
    expandable = true,
    children,
    ...props
  }: ReasoningProps) => {
    const resolvedDefaultOpen = expandable
      ? (defaultOpen ?? isStreaming)
      : false
    // Track if defaultOpen was explicitly set to false (to prevent auto-open)
    const isExplicitlyClosed = defaultOpen === false || !expandable

    const [isOpen, setIsOpen] = useControllableState<boolean>({
      defaultProp: resolvedDefaultOpen,
      onChange: onOpenChange,
      prop: expandable ? open : false,
    })
    const [duration, setDuration] = useControllableState<number | undefined>({
      defaultProp: undefined,
      prop: durationProp,
    })

    const hasEverStreamedRef = useRef(isStreaming)
    const manuallyClosedRef = useRef(false)
    const previousStreamingRef = useRef(isStreaming)
    const [hasAutoClosed, setHasAutoClosed] = useState(false)
    const startTimeRef = useRef<number | null>(null)

    // Track when streaming starts and compute duration
    useEffect(() => {
      if (isStreaming) {
        hasEverStreamedRef.current = true
        if (startTimeRef.current === null) {
          startTimeRef.current = Date.now()
        }
      } else if (startTimeRef.current !== null) {
        setDuration(Math.ceil((Date.now() - startTimeRef.current) / MS_IN_S))
        startTimeRef.current = null
      }
    }, [isStreaming, setDuration])

    // Auto-open when streaming starts (unless explicitly closed)
    useEffect(() => {
      if (!isStreaming) {
        manuallyClosedRef.current = false
      }
    }, [isStreaming])

    // A reasoning part can stay mounted while its streaming flag changes. If
    // it was opened during a live turn, an explicit `defaultOpen={false}` must
    // still win when the turn settles; otherwise the old open state leaks into
    // the completed transcript. This only runs on the streaming → settled
    // transition, so a user can still open the completed reasoning manually.
    useEffect(() => {
      if (
        previousStreamingRef.current &&
        !isStreaming &&
        defaultOpen === false &&
        isOpen
      ) {
        setIsOpen(false)
      }
      previousStreamingRef.current = isStreaming
    }, [defaultOpen, isOpen, isStreaming, setIsOpen])

    useEffect(() => {
      if (isStreaming && !isOpen && !isExplicitlyClosed) {
        if (!manuallyClosedRef.current) {
          setIsOpen(true)
        }
      }
    }, [isStreaming, isOpen, setIsOpen, isExplicitlyClosed])

    // Auto-close when streaming ends (once only, and only if it ever streamed)
    useEffect(() => {
      if (
        hasEverStreamedRef.current &&
        !isStreaming &&
        isOpen &&
        !hasAutoClosed
      ) {
        const timer = setTimeout(() => {
          setIsOpen(false)
          setHasAutoClosed(true)
        }, AUTO_CLOSE_DELAY)

        return () => clearTimeout(timer)
      }
    }, [isStreaming, isOpen, setIsOpen, hasAutoClosed])

    const handleOpenChange = useCallback(
      (newOpen: boolean) => {
        if (isStreaming) {
          manuallyClosedRef.current = !newOpen
        }
        setIsOpen(newOpen)
      },
      [isStreaming, setIsOpen]
    )

    const contextValue = useMemo(
      () => ({ duration, isOpen, isStreaming, setIsOpen, expandable }),
      [duration, isOpen, isStreaming, setIsOpen, expandable]
    )

    return (
      <ReasoningContext.Provider value={contextValue}>
        <Collapsible
          className={cn("not-prose", className)}
          onOpenChange={handleOpenChange}
          open={isOpen}
          {...props}
        >
          {children}
        </Collapsible>
      </ReasoningContext.Provider>
    )
  }
)

export type ReasoningTriggerProps = ComponentProps<
  typeof CollapsibleTrigger
> & {
  getThinkingMessage?: (isStreaming: boolean, duration?: number) => ReactNode
}

export const ReasoningTrigger = memo(
  ({
    className,
    children,
    getThinkingMessage,
    ...props
  }: ReasoningTriggerProps) => {
    const t = useTranslations("Folder.chat.reasoning")
    const { isStreaming, isOpen, duration, expandable } = useReasoning()
    const defaultGetThinkingMessage = useCallback(
      (nextIsStreaming: boolean, nextDuration?: number) => {
        if (nextIsStreaming || nextDuration === 0) {
          return (
            <Shimmer duration={1} shineColor="var(--primary)">
              {t("thinking")}
            </Shimmer>
          )
        }
        if (nextDuration === undefined) {
          return <p>{t("thoughtForFewSeconds")}</p>
        }
        return <p>{t("thoughtForSeconds", { duration: nextDuration })}</p>
      },
      [t]
    )
    const thinkingMessageBuilder =
      getThinkingMessage ?? defaultGetThinkingMessage

    return (
      <CollapsibleTrigger
        className={cn(
          "flex w-full items-center gap-2 text-muted-foreground text-sm transition-colors",
          expandable
            ? "hover:text-foreground"
            : "cursor-default hover:text-muted-foreground",
          className
        )}
        disabled={!expandable}
        {...props}
      >
        {children ?? (
          <>
            <BrainIcon className="size-4" />
            {thinkingMessageBuilder(isStreaming, duration)}
            {expandable && (
              <ChevronDownIcon
                className={cn(
                  "size-4 transition-transform",
                  isOpen ? "rotate-180" : "rotate-0"
                )}
              />
            )}
          </>
        )}
      </CollapsibleTrigger>
    )
  }
)

export type ReasoningContentProps = ComponentProps<
  typeof CollapsibleContent
> & {
  children: string
}

export type ReasoningBodyProps = ComponentProps<"div"> & {
  children: string
}

const remarkPlugins = [
  ...Object.values(defaultRemarkPlugins),
  remarkRewriteFileUriLinks,
  remarkLocalPathLinks,
]
const rehypePlugins = rehypePluginsAllowingIywClaw(defaultRehypePlugins)

export const ReasoningBody = memo(
  ({ className, children, ...props }: ReasoningBodyProps) => {
    const normalized = useMemo(
      () => normalizeMathDelimiters(children),
      [children]
    )
    const plugins = useStreamdownPlugins(normalized)

    return (
      <div
        className={cn("text-sm text-muted-foreground outline-none", className)}
        {...props}
      >
        <Streamdown
          plugins={plugins}
          remarkPlugins={remarkPlugins}
          rehypePlugins={rehypePlugins}
          components={{
            ...localPathCodeBlockComponents,
            ...markdownLinkComponents,
          }}
        >
          {normalized}
        </Streamdown>
      </div>
    )
  }
)

export const ReasoningContent = memo(
  ({ className, children, ...props }: ReasoningContentProps) => (
    <CollapsibleContent
      className={cn(
        "mt-4 max-h-[min(15rem,35vh)] overflow-y-auto overscroll-contain pe-1",
        "iyw-claw-reasoning-content outline-none",
        className
      )}
      {...props}
    >
      <ReasoningBody>{children}</ReasoningBody>
    </CollapsibleContent>
  )
)

Reasoning.displayName = "Reasoning"
ReasoningTrigger.displayName = "ReasoningTrigger"
ReasoningBody.displayName = "ReasoningBody"
ReasoningContent.displayName = "ReasoningContent"
