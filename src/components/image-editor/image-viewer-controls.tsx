"use client"

import type { ReactNode } from "react"
import { Dialog as DialogPrimitive } from "radix-ui"
import {
  ChevronLeft,
  ChevronRight,
  Download,
  Eye,
  LoaderCircle,
  PencilLine,
  RotateCcw,
  RotateCw,
  Scan,
  X,
  ZoomIn,
  ZoomOut,
} from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { cn } from "@/lib/utils"
import {
  clampImageZoom,
  IMAGE_ZOOM_MAX,
  IMAGE_ZOOM_MIN,
  IMAGE_ZOOM_STEP,
  type ImagePreviewNavigation,
  type ImageViewerMode,
} from "./image-editor-model"

interface IconButtonProps {
  label: string
  disabled?: boolean
  onClick: () => void
  children: ReactNode
  className?: string
}

function IconButton(props: IconButtonProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          disabled={props.disabled}
          aria-label={props.label}
          onClick={props.onClick}
          className={cn(
            "rounded-md text-white/75 hover:bg-white/10 hover:text-white",
            props.className
          )}
        >
          {props.children}
        </Button>
      </TooltipTrigger>
      <TooltipContent side="bottom" sideOffset={6}>
        {props.label}
      </TooltipContent>
    </Tooltip>
  )
}

interface ModeSwitchProps {
  mode: ImageViewerMode
  disabled: boolean
  onChange: (mode: ImageViewerMode) => void
}

function ModeSwitch({ mode, disabled, onChange }: ModeSwitchProps) {
  const t = useTranslations("Folder.chat.messageList")
  const options = [
    { value: "view" as const, label: t("imageViewerView"), icon: Eye },
    {
      value: "annotate" as const,
      label: t("imageViewerAnnotate"),
      icon: PencilLine,
    },
  ]
  return (
    <div
      role="group"
      aria-label={t("imageViewerMode")}
      className="flex h-9 shrink-0 items-center rounded-md bg-white/8 p-0.5"
    >
      {options.map((option) => {
        const Icon = option.icon
        return (
          <button
            key={option.value}
            type="button"
            disabled={disabled}
            aria-label={option.label}
            aria-pressed={mode === option.value}
            title={option.label}
            onClick={() => onChange(option.value)}
            className={cn(
              "flex h-8 min-w-8 items-center justify-center gap-1.5 rounded-[5px] px-2 text-xs text-white/60 outline-none transition-colors hover:text-white focus-visible:ring-2 focus-visible:ring-white/50 disabled:pointer-events-none disabled:opacity-40 sm:min-w-[4.75rem]",
              mode === option.value && "bg-white/14 text-white shadow-sm"
            )}
          >
            <Icon className="size-4" />
            <span className="hidden sm:inline">{option.label}</span>
          </button>
        )
      })}
    </div>
  )
}

interface ImageViewerHeaderProps {
  title: string
  mode: ImageViewerMode
  ready: boolean
  busy: boolean
  navigation?: ImagePreviewNavigation
  onModeChange: (mode: ImageViewerMode) => void
  onRotateLeft: () => void
  onRotateRight: () => void
  onDownload: () => void
  onClose: () => void
}

function HeaderActions(props: ImageViewerHeaderProps) {
  const t = useTranslations("Folder.chat.messageList")
  const compactClassName = "max-[420px]:size-8"
  return (
    <div className="flex shrink-0 items-center gap-0.5">
      {props.mode === "view" ? (
        <>
          <IconButton
            label={t("imageViewerRotateLeft")}
            disabled={!props.ready || props.busy}
            onClick={props.onRotateLeft}
            className={compactClassName}
          >
            <RotateCcw />
          </IconButton>
          <IconButton
            label={t("imageViewerRotateRight")}
            disabled={!props.ready || props.busy}
            onClick={props.onRotateRight}
            className={compactClassName}
          >
            <RotateCw />
          </IconButton>
        </>
      ) : null}
      <IconButton
        label={t("downloadImage")}
        disabled={!props.ready || props.busy}
        onClick={props.onDownload}
        className={compactClassName}
      >
        {props.busy ? <LoaderCircle className="animate-spin" /> : <Download />}
      </IconButton>
      <IconButton
        label={t("imageEditorClose")}
        disabled={props.busy}
        onClick={props.onClose}
        className={compactClassName}
      >
        <X />
      </IconButton>
    </div>
  )
}

export function ImageViewerHeader(props: ImageViewerHeaderProps) {
  const t = useTranslations("Folder.chat.messageList")
  const counter = props.navigation
    ? `${props.navigation.index + 1} / ${props.navigation.total}`
    : null
  return (
    <TooltipProvider>
      <header className="flex h-14 min-w-0 items-center gap-1 border-b border-white/8 bg-zinc-950/90 px-2 sm:gap-2 sm:px-3">
        <div className="flex min-w-0 flex-1 items-baseline gap-2">
          <DialogPrimitive.Title className="min-w-0 flex-1 truncate text-sm font-medium text-white">
            {props.title || t("imageViewerTitle")}
          </DialogPrimitive.Title>
          {counter ? (
            <span className="shrink-0 text-xs tabular-nums text-white/45 max-[479px]:hidden">
              {counter}
            </span>
          ) : null}
        </div>
        <ModeSwitch
          mode={props.mode}
          disabled={!props.ready || props.busy}
          onChange={props.onModeChange}
        />
        <HeaderActions {...props} />
      </header>
    </TooltipProvider>
  )
}

interface ImageViewerNavigationProps {
  navigation?: ImagePreviewNavigation
  disabled: boolean
}

export function ImageViewerNavigation({
  navigation,
  disabled,
}: ImageViewerNavigationProps) {
  const t = useTranslations("Folder.chat.messageList")
  if (!navigation || navigation.total <= 1) return null
  const previousDisabled = disabled || navigation.index <= 0
  const nextDisabled = disabled || navigation.index >= navigation.total - 1
  return (
    <TooltipProvider>
      <div className="pointer-events-none absolute inset-x-2 top-1/2 z-20 flex -translate-y-1/2 justify-between sm:inset-x-4">
        <IconButton
          label={t("imageViewerPrevious")}
          disabled={previousDisabled}
          onClick={() => navigation.onIndexChange(navigation.index - 1)}
          className="pointer-events-auto size-10 border border-white/10 bg-black/55 shadow-lg backdrop-blur hover:bg-black/75 sm:size-11"
        >
          <ChevronLeft className="size-5" />
        </IconButton>
        <IconButton
          label={t("imageViewerNext")}
          disabled={nextDisabled}
          onClick={() => navigation.onIndexChange(navigation.index + 1)}
          className="pointer-events-auto size-10 border border-white/10 bg-black/55 shadow-lg backdrop-blur hover:bg-black/75 sm:size-11"
        >
          <ChevronRight className="size-5" />
        </IconButton>
      </div>
    </TooltipProvider>
  )
}

interface ImageViewerZoomControlsProps {
  zoom: number
  ready: boolean
  onZoomChange: (zoom: number) => void
}

export function ImageViewerZoomControls(props: ImageViewerZoomControlsProps) {
  const t = useTranslations("Folder.chat.messageList")
  const changeZoom = (delta: number) =>
    props.onZoomChange(clampImageZoom(props.zoom + delta))
  return (
    <TooltipProvider>
      <div className="flex h-10 w-[11.5rem] items-center justify-center gap-0.5 rounded-md border border-white/10 bg-zinc-900/90 p-1 text-white shadow-xl backdrop-blur">
        <IconButton
          label={t("imageEditorZoomOut")}
          disabled={!props.ready || props.zoom <= IMAGE_ZOOM_MIN}
          onClick={() => changeZoom(-IMAGE_ZOOM_STEP)}
        >
          <ZoomOut />
        </IconButton>
        <button
          type="button"
          disabled={!props.ready || props.zoom === 1}
          onClick={() => props.onZoomChange(1)}
          className="h-8 w-14 shrink-0 rounded-[5px] text-xs tabular-nums text-white/70 outline-none hover:bg-white/10 hover:text-white focus-visible:ring-2 focus-visible:ring-white/50 disabled:opacity-50"
          aria-label={t("imageViewerFit")}
          title={t("imageViewerFit")}
        >
          {Math.round(props.zoom * 100)}%
        </button>
        <IconButton
          label={t("imageViewerFit")}
          disabled={!props.ready || props.zoom === 1}
          onClick={() => props.onZoomChange(1)}
        >
          <Scan />
        </IconButton>
        <IconButton
          label={t("imageEditorZoomIn")}
          disabled={!props.ready || props.zoom >= IMAGE_ZOOM_MAX}
          onClick={() => changeZoom(IMAGE_ZOOM_STEP)}
        >
          <ZoomIn />
        </IconButton>
      </div>
    </TooltipProvider>
  )
}
