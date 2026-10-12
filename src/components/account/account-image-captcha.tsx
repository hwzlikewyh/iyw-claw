"use client"

import { useEffect, useRef, useState } from "react"
import { RefreshCw, ShieldCheck, X } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import type { IywImageChallenge } from "@/lib/iyw-account-flow"
import {
  drawAccountPuzzle,
  PUZZLE_HEIGHT,
  PUZZLE_PIECE_WIDTH,
  PUZZLE_WIDTH,
} from "./account-puzzle-canvas"

import { AccountImageSliderTrack } from "./account-image-slider-track"

interface Props {
  challenge: IywImageChallenge
  loading: boolean
  onVerify: (position: number) => Promise<void>
  onRefresh: () => Promise<void>
  onCancel: () => void
}

export function AccountImageCaptcha(props: Props) {
  return <ImageCaptcha key={props.challenge.id} {...props} />
}

function ImageCaptcha(props: Props) {
  const t = useTranslations("AccountFlow")
  const {
    background,
    piece,
    position,
    setPosition,
    failed,
    imageError,
    imageReady,
    verify,
  } = useImageCaptcha(props)
  return (
    <section
      className="rounded-xl border border-primary/15 bg-primary/[0.025] p-3"
      aria-label={t("sliderTitle")}
    >
      <CaptchaHeader {...props} />
      <div className="mx-auto w-full max-w-[320px]">
        <PuzzlePicture
          background={background}
          piece={piece}
          position={position}
          imageError={imageError}
        />
        <AccountImageSliderTrack
          position={position}
          onChange={setPosition}
          loading={props.loading}
          disabled={props.loading || imageError || !imageReady}
          failed={failed}
          onVerify={verify}
        />
      </div>
    </section>
  )
}

function useImageCaptcha(props: Props) {
  const background = useRef<HTMLCanvasElement>(null)
  const piece = useRef<HTMLCanvasElement>(null)
  const [position, setPosition] = useState(0)
  const [failed, setFailed] = useState(false)
  const [imageError, setImageError] = useState(false)
  const [imageReady, setImageReady] = useState(false)
  const submitting = useRef(false)
  useEffect(() => {
    if (!background.current || !piece.current) return
    return drawAccountPuzzle(
      { background: background.current, piece: piece.current },
      props.challenge,
      { onError: () => setImageError(true), onReady: () => setImageReady(true) }
    )
  }, [props.challenge])
  const verify = async (value: number) => {
    if (
      submitting.current ||
      props.loading ||
      imageError ||
      !imageReady ||
      value === 0
    )
      return
    submitting.current = true
    setFailed(false)
    try {
      await props.onVerify(value)
    } catch {
      setFailed(true)
      setPosition(0)
    } finally {
      submitting.current = false
    }
  }
  return {
    background,
    piece,
    position,
    setPosition,
    failed,
    imageError,
    imageReady,
    verify,
  }
}

function CaptchaHeader(props: Props) {
  const t = useTranslations("AccountFlow")
  return (
    <div className="mb-3 flex items-center justify-between gap-2">
      <div className="flex items-center gap-2 text-xs font-medium">
        <ShieldCheck className="size-3.5 text-primary" />
        {t("sliderTitle")}
      </div>
      <div className="flex gap-1">
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="size-6"
          disabled={props.loading}
          aria-label={t("sliderRefresh")}
          onClick={() => void props.onRefresh()}
        >
          <RefreshCw className="size-3" />
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="size-6"
          disabled={props.loading}
          aria-label={t("cancel")}
          onClick={props.onCancel}
        >
          <X className="size-3" />
        </Button>
      </div>
    </div>
  )
}

function PuzzlePicture({
  background,
  piece,
  position,
  imageError,
}: {
  background: React.RefObject<HTMLCanvasElement | null>
  piece: React.RefObject<HTMLCanvasElement | null>
  position: number
  imageError: boolean
}) {
  const t = useTranslations("AccountFlow")
  return (
    <div
      className="relative overflow-hidden rounded-lg bg-muted"
      style={{ aspectRatio: `${PUZZLE_WIDTH}/${PUZZLE_HEIGHT}` }}
    >
      <canvas
        ref={background}
        width={PUZZLE_WIDTH}
        height={PUZZLE_HEIGHT}
        className="block w-full"
        aria-hidden
      />
      <canvas
        ref={piece}
        width={PUZZLE_WIDTH}
        height={PUZZLE_HEIGHT}
        className="absolute top-0 h-full drop-shadow-md"
        style={{
          left: `${(position / PUZZLE_WIDTH) * 100}%`,
          width: `${(PUZZLE_PIECE_WIDTH / PUZZLE_WIDTH) * 100}%`,
        }}
        aria-hidden
      />
      {imageError && (
        <p
          role="alert"
          className="absolute inset-0 flex items-center justify-center bg-muted px-4 text-center text-xs text-destructive"
        >
          {t("sliderImageFailed")}
        </p>
      )}
    </div>
  )
}
