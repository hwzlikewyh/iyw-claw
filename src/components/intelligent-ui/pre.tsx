"use client"

import type { Components } from "streamdown"
import {
  Component,
  isValidElement,
  lazy,
  Suspense,
  type ReactNode,
  type ComponentProps,
} from "react"
import { localPathCodeBlockComponents } from "@/components/ai-elements/local-path-code-block"
import { useUiScope } from "./scope"

const UiBlock = lazy(() => import("./block"))
class Boundary extends Component<
  { children: ReactNode; fallback: ReactNode; source: string },
  { failed: boolean; source: string }
> {
  state = { failed: false, source: this.props.source }
  static getDerivedStateFromError() {
    return { failed: true }
  }
  static getDerivedStateFromProps(
    props: { source: string },
    state: { source: string }
  ) {
    return props.source !== state.source
      ? { failed: false, source: props.source }
      : null
  }
  render() {
    return this.state.failed ? this.props.fallback : this.props.children
  }
}

function UiPre({ children, ...props }: ComponentProps<"pre">) {
  const scope = useUiScope()
  const fallback = (
    <localPathCodeBlockComponents.pre {...props}>
      {children}
    </localPathCodeBlockComponents.pre>
  )
  if (
    !scope?.enabled ||
    !isValidElement<{ className?: string; children?: unknown }>(children)
  )
    return fallback
  const language = children.props.className?.match(
    /language-(iyw-ui|iyw-widget)(?:\s|$)/
  )?.[1]
  const source = children.props.children
  if (!language || typeof source !== "string") return fallback
  const closed = scope.closedSources.has(source.trim())
  return (
    <Boundary source={source} fallback={fallback}>
      <Suspense fallback={fallback}>
        <UiBlock
          source={source}
          kind={language === "iyw-widget" ? "widget" : "block"}
          closed={closed}
          fallback={fallback}
        />
      </Suspense>
    </Boundary>
  )
}
export const intelligentUiComponents: Components = {
  pre: UiPre as Components["pre"],
}
