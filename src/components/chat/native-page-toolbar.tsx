"use client"

import { useState, type ComponentType } from "react"
import { useTranslations } from "next-intl"
import {
  ChevronLeft,
  ChevronRight,
  RefreshCcw,
  OctagonX,
  ScanLine,
  Focus,
  ImagePlus,
  PencilRuler,
  MessageSquarePlus,
  TerminalSquare,
  SearchCheck,
  Smartphone,
  Monitor,
  Braces,
  Minus,
  Plus,
  Copy,
  ArrowUpRight,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import type { NativePageState } from "@/lib/native-page-api"

export type PageTool =
  | "back"
  | "forward"
  | "reload"
  | "stop"
  | "pick"
  | "region"
  | "capture"
  | "markup"
  | "share"
  | "console"
  | "find"
  | "device"
  | "devtools"
  | "zoom-in"
  | "zoom-out"
  | "copy"
  | "external"

type Props = {
  state: NativePageState | null
  url: string
  busy: boolean
  picking: boolean
  mobile: boolean
  zoom: number
  native: boolean
  onAction: (action: PageTool) => void
  onNavigate: (url: string) => void
}

export function NativePageToolbar(props: Props) {
  const t = useTranslations("Folder.chat.interactiveHtml.native")
  const icon = (
    action: PageTool,
    Icon: ComponentType<{ className?: string }>,
    disabled = false
  ) => (
    <Button
      key={action}
      size="icon-sm"
      variant="ghost"
      title={t(action)}
      aria-label={t(action)}
      disabled={props.busy || disabled}
      onClick={() => props.onAction(action)}
    >
      <Icon />
    </Button>
  )
  return (
    <div className="shrink-0 border-b">
      <NativeAddress props={props} icon={icon} />
      <div className="flex flex-wrap items-center gap-0.5 px-2 pb-1">
        {icon("pick", ScanLine, !props.native || !props.state)}
        {icon("region", Focus, !props.native || !props.state)}
        {icon("capture", ImagePlus, !props.native || !props.state)}
        {icon("markup", PencilRuler, !props.native || !props.state)}
        {icon("share", MessageSquarePlus)}
        {icon("console", TerminalSquare, !props.native || !props.state)}
        {icon("find", SearchCheck, !props.native || !props.state)}
        {icon("device", props.mobile ? Smartphone : Monitor)}
        {icon("devtools", Braces, !props.native || !props.state)}
        {icon("zoom-out", Minus, !props.native || !props.state)}
        <span className="px-1 text-xs text-muted-foreground">
          {Math.round(props.zoom * 100)}%
        </span>
        {icon("zoom-in", Plus, !props.native || !props.state)}
      </div>
      {props.picking && (
        <p className="px-3 pb-2 text-xs text-primary">{t("pickHint")}</p>
      )}
    </div>
  )
}

function NativeAddress({
  props,
  icon,
}: {
  props: Props
  icon: (
    action: PageTool,
    Icon: ComponentType<{ className?: string }>,
    disabled?: boolean
  ) => React.ReactNode
}) {
  const t = useTranslations("Folder.chat.interactiveHtml.native")
  const [address, setAddress] = useState(props.url)
  return (
    <div className="flex items-center gap-1 px-2 py-1">
      {icon("back", ChevronLeft, !props.state?.canBack)}
      {icon("forward", ChevronRight, !props.state?.canForward)}
      {icon(
        props.state?.loading ? "stop" : "reload",
        props.state?.loading ? OctagonX : RefreshCcw,
        !props.native || !props.state
      )}
      <Input
        className="h-8 min-w-0 flex-1 text-xs"
        value={address}
        aria-label={t("address")}
        onChange={(event) => setAddress(event.target.value)}
        onFocus={(event) => event.target.select()}
        onKeyDown={(event) => {
          if (event.key === "Enter") props.onNavigate(address)
          if (event.key === "Escape") setAddress(props.url)
        }}
        spellCheck={false}
        disabled={props.busy || (props.native && !props.state)}
      />
      {icon("copy", Copy)}
      {icon("external", ArrowUpRight)}
    </div>
  )
}
