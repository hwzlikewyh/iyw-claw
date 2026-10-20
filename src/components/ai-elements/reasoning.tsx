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
} from "react"

import { Shimmer } from "./shimmer"
import { ReasoningBody } from "./reasoning-body"
export { ReasoningBody } from "./reasoning-body"
export type { ReasoningBodyProps } from "./reasoning-body"
import { useStickToBottom } from "use-stick-to-bottom"

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
    const hasAutoClosedRef = useRef(false)
    const startTimeRef = useRef<number | null>(null)

    // Track when streaming starts and compute duration
    useEffect(() => {
      if (isStreaming) {
        hasAutoClosedRef.current = false
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
        defaultOpen === false
      ) {
        // 原位结束后只收起一次，用户重新展开历史内容时不再启动关闭计时器。
        hasAutoClosedRef.current = true
        if (isOpen) setIsOpen(false)
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
        !hasAutoClosedRef.current
      ) {
        const timer = setTimeout(() => {
          setIsOpen(false)
          hasAutoClosedRef.current = true
        }, AUTO_CLOSE_DELAY)

        return () => clearTimeout(timer)
      }
    }, [isStreaming, isOpen, setIsOpen])

    const handleOpenChange = useCallback(
      (newOpen: boolean) => {
        if (isStreaming) {
          manuallyClosedRef.current = !newOpen
        } else hasAutoClosedRef.current = true
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

export const ReasoningContent = memo(
  ({ className, children, ...props }: ReasoningContentProps) => {
    const { isStreaming } = useReasoning()
    const { scrollRef, contentRef } = useStickToBottom({
      initial: isStreaming ? "instant" : false,
      resize: "instant",
    })
    return (
      <CollapsibleContent
        ref={scrollRef}
        className={cn(
          "mt-4 max-h-[min(15rem,35vh)] overflow-y-auto overscroll-contain pe-1",
          "iyw-claw-reasoning-content outline-none",
          className
        )}
        {...props}
      >
        <div ref={contentRef}>
          <ReasoningBody>{children}</ReasoningBody>
        </div>
      </CollapsibleContent>
    )
  }
)

Reasoning.displayName = "Reasoning"
ReasoningTrigger.displayName = "ReasoningTrigger"
ReasoningContent.displayName = "ReasoningContent"
