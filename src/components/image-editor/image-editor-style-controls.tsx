"use client"

import { Palette, SlidersHorizontal } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { cn } from "@/lib/utils"
import type { ImageEditorToolbarProps } from "./image-editor-model"

const COLORS = ["#ef4444", "#f59e0b", "#22c55e", "#3b82f6", "#f8fafc"]
const STROKE_WIDTHS = [2, 4, 8]

function StyleTrigger({
  label,
  disabled,
  children,
}: {
  label: string
  disabled: boolean
  children: React.ReactNode
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <PopoverTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            disabled={disabled}
            aria-label={label}
            className="rounded-md text-white/75 hover:bg-white/10 hover:text-white data-[state=open]:bg-white/12 data-[state=open]:text-white"
          >
            {children}
          </Button>
        </PopoverTrigger>
      </TooltipTrigger>
      <TooltipContent side="top" sideOffset={6}>
        {label}
      </TooltipContent>
    </Tooltip>
  )
}

function ColorControl({ editor }: { editor: ImageEditorToolbarProps }) {
  const t = useTranslations("Folder.chat.messageList")
  return (
    <Popover>
      <StyleTrigger
        label={t("imageEditorColor")}
        disabled={!editor.ready || editor.busy}
      >
        <Palette />
        <span
          className="absolute bottom-1 right-1 size-2.5 rounded-full border border-white/50"
          style={{ backgroundColor: editor.color }}
        />
      </StyleTrigger>
      <PopoverContent
        side="top"
        align="center"
        sideOffset={10}
        className="w-auto gap-2 rounded-md border border-white/10 bg-zinc-900 p-2 text-white"
      >
        <span className="px-1 text-xs text-white/55">
          {t("imageEditorColor")}
        </span>
        <div className="flex items-center gap-1.5">
          {COLORS.map((color) => (
            <button
              key={color}
              type="button"
              onClick={() => editor.onColorChange(color)}
              aria-label={t("imageEditorChooseColor", { color })}
              aria-pressed={editor.color === color}
              className={cn(
                "size-7 rounded-full border border-white/30 outline-none transition-transform hover:scale-110 focus-visible:ring-2 focus-visible:ring-white/60",
                editor.color === color &&
                  "ring-2 ring-white ring-offset-2 ring-offset-zinc-900"
              )}
              style={{ backgroundColor: color }}
            />
          ))}
        </div>
      </PopoverContent>
    </Popover>
  )
}

function StrokeControl({ editor }: { editor: ImageEditorToolbarProps }) {
  const t = useTranslations("Folder.chat.messageList")
  return (
    <Popover>
      <StyleTrigger
        label={t("imageEditorStrokeWidth")}
        disabled={!editor.ready || editor.busy}
      >
        <SlidersHorizontal />
      </StyleTrigger>
      <PopoverContent
        side="top"
        align="center"
        sideOffset={10}
        className="w-auto gap-2 rounded-md border border-white/10 bg-zinc-900 p-2 text-white"
      >
        <span className="px-1 text-xs text-white/55">
          {t("imageEditorStrokeWidth")}
        </span>
        <div className="flex items-center gap-1">
          {STROKE_WIDTHS.map((width) => (
            <button
              key={width}
              type="button"
              onClick={() => editor.onStrokeWidthChange(width)}
              aria-label={t("imageEditorStrokeWidthValue", { width })}
              aria-pressed={editor.strokeWidth === width}
              className={cn(
                "flex size-9 items-center justify-center rounded-[5px] text-white/70 outline-none hover:bg-white/10 focus-visible:ring-2 focus-visible:ring-white/60",
                editor.strokeWidth === width && "bg-white/14 text-white"
              )}
            >
              <span
                className="w-5 rounded-full bg-current"
                style={{ height: width }}
              />
            </button>
          ))}
        </div>
      </PopoverContent>
    </Popover>
  )
}

export function ImageEditorStyleControls({
  editor,
}: {
  editor: ImageEditorToolbarProps
}) {
  if (editor.tool === "select" || editor.tool === "crop") return null
  return (
    <>
      <ColorControl editor={editor} />
      <StrokeControl editor={editor} />
    </>
  )
}
