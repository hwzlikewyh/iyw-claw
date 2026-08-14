"use client"

import { memo, type CSSProperties, type ReactNode } from "react"

export interface AgentGlyphProps {
  size?: number | string
}

interface GlyphProps extends AgentGlyphProps {
  title: string
  children: ReactNode
}

const glyphStyle: CSSProperties = {
  display: "block",
  filter: "drop-shadow(0 0.7px 0.6px rgb(15 23 42 / 0.22))",
  overflow: "visible",
}

function Glyph({ title, size = "1em", children }: GlyphProps) {
  return (
    <svg
      aria-label={title}
      height={size}
      role="img"
      shapeRendering="geometricPrecision"
      style={glyphStyle}
      viewBox="0 0 24 24"
      width={size}
      xmlns="http://www.w3.org/2000/svg"
    >
      <title>{title}</title>
      {children}
    </svg>
  )
}

export const FarMountainIcon = memo(function FarMountainIcon({
  size,
}: AgentGlyphProps) {
  return (
    <Glyph title="远山" size={size}>
      <path
        d="M2.5 18.7 8.7 8.2c.7-1.2 2.4-1.2 3.1 0l2.2 3.7 1.1-1.7c.7-1.1 2.3-1.1 3 0l3.4 5.7c.8 1.4-.2 3.1-1.8 3.1H4.2c-1.5 0-2.4-1.3-1.7-2.3Z"
        fill="#69B8A8"
      />
      <path d="m5.1 19 5.7-8.3c.6-.9 2-.9 2.6 0l5.7 8.3H5.1Z" fill="#D96F62" />
      <path
        d="m9.7 12.3 1.1-1.6c.6-.9 2-.9 2.6 0l1.1 1.6-1.6-.5-.8 1.1-.9-1.1-1.5.5Z"
        fill="#F8DDAE"
      />
    </Glyph>
  )
})

export const StarRiverIcon = memo(function StarRiverIcon({
  size,
}: AgentGlyphProps) {
  return (
    <Glyph title="星河" size={size}>
      <ellipse
        cx="12"
        cy="12"
        rx="8.7"
        ry="5.2"
        fill="none"
        stroke="#4B8FD8"
        strokeWidth="2.7"
        transform="rotate(-18 12 12)"
      />
      <path
        d="M5.1 15.5c3.3 1.7 7.9 1.6 12.1-.3 1.2-.5 2.2-1.2 3.1-2"
        fill="none"
        stroke="#78D2C4"
        strokeLinecap="round"
        strokeWidth="2.7"
      />
      <path
        d="m15.9 3.2.7 2.1 2.1.7-2.1.7-.7 2.1-.7-2.1-2.1-.7 2.1-.7.7-2.1Z"
        fill="#F2B84B"
      />
      <circle cx="6.2" cy="8.6" r="1.35" fill="#F2B84B" />
    </Glyph>
  )
})

export const FlowingLightIcon = memo(function FlowingLightIcon({
  size,
}: AgentGlyphProps) {
  return (
    <Glyph title="流光" size={size}>
      <path
        d="M3.1 15.8c4.1-5.8 8.2-7.6 14.9-7.5-3.2 1.5-5.4 3.6-7 6.5-2.3 4.1-5.6 4.5-7.9 1Z"
        fill="#5E8FE6"
      />
      <path
        d="M5 18.2c3.7.2 6.3-1 8.2-3.8 1.8-2.7 4.1-4.2 7.7-4.8-2.1 2.2-3.2 4.2-3.7 6.6-.7 3-4.2 4.8-7.2 3.8L5 18.2Z"
        fill="#E875A5"
      />
      <path
        d="m17.7 3 .8 2.4 2.4.8-2.4.8-.8 2.4-.8-2.4-2.4-.8 2.4-.8.8-2.4Z"
        fill="#F4C45E"
      />
    </Glyph>
  )
})

export const OpenClawGlyph = memo(function OpenClawGlyph({
  size,
}: AgentGlyphProps) {
  return (
    <Glyph title="开放之爪" size={size}>
      <path
        d="M8.2 4H5.5A2.5 2.5 0 0 0 3 6.5v11A2.5 2.5 0 0 0 5.5 20h2.7M15.8 4h2.7A2.5 2.5 0 0 1 21 6.5v11a2.5 2.5 0 0 1-2.5 2.5h-2.7"
        fill="none"
        stroke="#E36B5D"
        strokeLinecap="round"
        strokeWidth="2.8"
      />
      <path
        d="m8.4 7.2 1.8 9.6M12 6.4v10.8M15.6 7.2l-1.8 9.6"
        fill="none"
        stroke="#F3B85B"
        strokeLinecap="round"
        strokeWidth="2.4"
      />
    </Glyph>
  )
})

export const CloudBoatIcon = memo(function CloudBoatIcon({
  size,
}: AgentGlyphProps) {
  return (
    <Glyph title="云舟" size={size}>
      <path
        d="M5.1 15.7a3.4 3.4 0 0 1 .6-6.7 5 5 0 0 1 9.4-1.2 3.8 3.8 0 1 1 1.4 7.9H5.1Z"
        fill="#70B9E8"
      />
      <path
        d="m6.2 15.2 12.2-.7-2.2 4.1c-.4.8-1.2 1.3-2.1 1.3H9.5c-.8 0-1.6-.5-1.9-1.2l-1.4-3.5Z"
        fill="#2E887E"
      />
      <path d="M11.4 6.6v8.2l5.1-.3-5.1-7.9Z" fill="#F2B84B" />
    </Glyph>
  )
})

export const WindChaserIcon = memo(function WindChaserIcon({
  size,
}: AgentGlyphProps) {
  return (
    <Glyph title="逐风" size={size}>
      <path
        d="M3.1 7.4h7.4M2.5 11.9h9.1M4.2 16.4h6.1"
        fill="none"
        stroke="#66C7B1"
        strokeLinecap="round"
        strokeWidth="2.5"
      />
      <path
        d="M9.5 5.6c-.8-.5-1.7.4-1.2 1.2l3.4 5.2-3.4 5.2c-.5.8.4 1.7 1.2 1.2l10-5.5c.7-.4.7-1.4 0-1.8l-10-5.5Z"
        fill="#E87365"
      />
      <path d="m13.7 9.6 3.8 2.4-3.8 2.4 1-2.4-1-2.4Z" fill="#F5C45B" />
    </Glyph>
  )
})

export const HermesGlyph = memo(function HermesGlyph({
  size,
}: AgentGlyphProps) {
  return (
    <Glyph title="赫尔墨斯" size={size}>
      <path
        d="M11.9 5.1v14.2"
        stroke="#D99A3D"
        strokeLinecap="round"
        strokeWidth="2.3"
      />
      <path
        d="M10.6 7.9C7.1 4.4 4.2 5 3.1 5.7c1.1 3.6 3.5 5.1 7.5 4.4V7.9ZM13.3 7.9c3.5-3.5 6.4-2.9 7.5-2.2-1.1 3.6-3.5 5.1-7.5 4.4V7.9Z"
        fill="#6AB7D9"
      />
      <path
        d="M8.4 13.2c2.2-1.7 4.9 1.3 7.1-.4M8.5 16.4c2.1 1.7 4.8-1.4 7-.1"
        fill="none"
        stroke="#8A6ED1"
        strokeLinecap="round"
        strokeWidth="1.8"
      />
      <circle cx="12" cy="4.2" r="1.7" fill="#F2C15A" />
    </Glyph>
  )
})

export const GreenMistBuddyIcon = memo(function GreenMistBuddyIcon({
  size,
}: AgentGlyphProps) {
  return (
    <Glyph title="青岚" size={size}>
      <path
        d="M3.2 16.7c2.2-1.5 4.1-1.5 6.1 0 1.9 1.4 3.7 1.4 5.5.1 2.1-1.5 3.8-1.5 6 0"
        fill="none"
        stroke="#7FC7B8"
        strokeLinecap="round"
        strokeWidth="2.5"
      />
      <path
        d="M8.1 6.1a4.2 4.2 0 1 0 0 8.4 4.2 4.2 0 0 0 0-8.4Z"
        fill="#42A99B"
      />
      <path
        d="M15.9 6.1a4.2 4.2 0 1 0 0 8.4 4.2 4.2 0 0 0 0-8.4Z"
        fill="#E47A6C"
      />
      <path
        d="M10.3 9.2h3.4a1.6 1.6 0 1 1 0 3.2h-3.4a1.6 1.6 0 1 1 0-3.2Z"
        fill="#F5C45B"
      />
    </Glyph>
  )
})

export const MoonWhiteIcon = memo(function MoonWhiteIcon({
  size,
}: AgentGlyphProps) {
  return (
    <Glyph title="月白" size={size}>
      <path
        d="M17.7 16.8A8 8 0 0 1 9.1 4.1a8.1 8.1 0 1 0 8.6 12.7Z"
        fill="#5D86D8"
      />
      <path
        d="m17.4 5.1.7 2 2 .7-2 .7-.7 2-.7-2-2-.7 2-.7.7-2Z"
        fill="#F2BE54"
      />
      <circle cx="14.2" cy="12.4" r="1.2" fill="#8CD0C2" />
    </Glyph>
  )
})

export const InkRiverPiIcon = memo(function InkRiverPiIcon({
  size,
}: AgentGlyphProps) {
  return (
    <Glyph title="墨川" size={size}>
      <path
        d="M5.2 7.2h13.6M8.2 7.4v8.9M15.8 7.4v8.9"
        fill="none"
        stroke="#304C5B"
        strokeLinecap="round"
        strokeWidth="3"
      />
      <path
        d="M3.3 17.1c2.8-1.9 5.3-1.8 7.7.1 2.3 1.8 5 1.8 9.7-.6"
        fill="none"
        stroke="#4E9FD1"
        strokeLinecap="round"
        strokeWidth="2.8"
      />
      <circle cx="18.2" cy="5.2" r="1.5" fill="#78C8B7" />
    </Glyph>
  )
})

export const TinyFocusIcon = memo(function TinyFocusIcon({
  size,
}: AgentGlyphProps) {
  return (
    <Glyph title="知微" size={size}>
      <circle
        cx="11"
        cy="12"
        r="7.1"
        fill="none"
        stroke="#55B2A6"
        strokeWidth="2.7"
      />
      <path
        d="M11 7.9a4.1 4.1 0 1 0 4.1 4.1"
        fill="none"
        stroke="#E17367"
        strokeLinecap="round"
        strokeWidth="2.7"
      />
      <path
        d="m16.2 16.9 3.9 3.1"
        stroke="#4B6F91"
        strokeLinecap="round"
        strokeWidth="2.5"
      />
      <circle cx="15.9" cy="8.1" r="2" fill="#F2BD4F" />
    </Glyph>
  )
})
