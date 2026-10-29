"use client"

import {
  cloneElement,
  isValidElement,
  useMemo,
  type ComponentProps,
  type ReactElement,
} from "react"
import { MessageResponse } from "@/components/ai-elements/message"

function ReadOnlyLink({ href, children }: ComponentProps<"a">) {
  if (!href || !/^(https?:|mailto:|tel:)/i.test(href))
    return <span>{children}</span>
  return (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      className="text-primary underline"
    >
      {children}
    </a>
  )
}

function ReadOnlyImage({ alt }: ComponentProps<"img">) {
  return (
    <span className="text-muted-foreground">
      {alt || "图片未包含在本次分享中"}
    </span>
  )
}

function ReadOnlyPre({ children, ...props }: ComponentProps<"pre">) {
  // 保留现有 Streamdown 高亮和代码操作，不启用本地路径导航。
  if (isValidElement(children))
    return cloneElement(children as ReactElement<Record<string, unknown>>, {
      "data-block": "true",
    })
  return <pre {...props}>{children}</pre>
}

export function ShareMessageText({
  text,
  user,
}: {
  text: string
  user: boolean
}) {
  const components = useMemo(
    () => ({ a: ReadOnlyLink, img: ReadOnlyImage, pre: ReadOnlyPre }),
    []
  )
  return (
    <MessageResponse readOnly components={components} softBreaks={user}>
      {text}
    </MessageResponse>
  )
}
