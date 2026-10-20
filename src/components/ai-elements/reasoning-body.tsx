"use client"

import type { ComponentProps } from "react"
import { memo, useMemo } from "react"
import {
  Streamdown,
  defaultRehypePlugins,
  defaultRemarkPlugins,
} from "streamdown"
import { cn } from "@/lib/utils"
import { markdownLinkComponents } from "./markdown-link"
import { localPathCodeBlockComponents } from "./local-path-code-block"
import { rehypePluginsAllowingIywClaw } from "./rehype-allow-iyw-claw"
import { normalizeMathDelimiters } from "./message"
import { remarkRewriteFileUriLinks } from "./remark-file-uri-links"
import { remarkLocalPathLinks } from "./remark-local-path-links"
import { useStreamdownPlugins } from "./streamdown-plugins"

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

ReasoningBody.displayName = "ReasoningBody"
