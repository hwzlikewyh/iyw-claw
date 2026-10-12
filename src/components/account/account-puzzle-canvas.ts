import type { IywImageChallenge } from "@/lib/iyw-account-flow"

export const PUZZLE_WIDTH = 320
export const PUZZLE_HEIGHT = 194
export const PUZZLE_MAX_POSITION = 260
const PIECE_WIDTH = 42
const PIECE_RADIUS = 9
const PIECE_CROP_WIDTH = PIECE_WIDTH + 2 * PIECE_RADIUS + 3
export const PUZZLE_PIECE_WIDTH = PIECE_CROP_WIDTH
const PI = Math.PI

function puzzlePath(x: number, y: number) {
  const path = new Path2D()
  path.moveTo(x, y)
  path.arc(
    x + PIECE_WIDTH / 2,
    y - PIECE_RADIUS + 2,
    PIECE_RADIUS,
    0.72 * PI,
    2.26 * PI
  )
  path.lineTo(x + PIECE_WIDTH, y)
  path.arc(
    x + PIECE_WIDTH + PIECE_RADIUS - 2,
    y + PIECE_WIDTH / 2,
    PIECE_RADIUS,
    1.21 * PI,
    2.78 * PI
  )
  path.lineTo(x + PIECE_WIDTH, y + PIECE_WIDTH)
  path.lineTo(x, y + PIECE_WIDTH)
  path.arc(
    x + PIECE_RADIUS - 2,
    y + PIECE_WIDTH / 2,
    PIECE_RADIUS + 0.4,
    2.76 * PI,
    1.24 * PI,
    true
  )
  path.lineTo(x, y)
  return path
}

export function drawAccountPuzzle(
  canvases: { background: HTMLCanvasElement; piece: HTMLCanvasElement },
  challenge: IywImageChallenge,
  events: { onError: () => void; onReady: () => void }
) {
  const image = new Image()
  image.onload = () => {
    try {
      drawImage(canvases, challenge, image)
      events.onReady()
    } catch {
      events.onError()
    }
  }
  image.onerror = events.onError
  image.src = challenge.image_data
  return () => {
    image.onload = null
    image.onerror = null
  }
}

function drawImage(
  canvases: { background: HTMLCanvasElement; piece: HTMLCanvasElement },
  challenge: IywImageChallenge,
  image: HTMLImageElement
) {
  const background = canvases.background.getContext("2d")
  const piece = canvases.piece.getContext("2d")
  if (!background || !piece) throw new Error("Canvas unavailable")
  for (const canvas of Object.values(canvases)) {
    canvas.width = PUZZLE_WIDTH
    canvas.height = PUZZLE_HEIGHT
  }
  const path = puzzlePath(challenge.coords_x, challenge.coords_y)
  background.drawImage(image, 0, 0, PUZZLE_WIDTH, PUZZLE_HEIGHT)
  background.fillStyle = "rgba(0,0,0,0.55)"
  background.fill(path)
  background.strokeStyle = "rgba(255,255,255,0.9)"
  background.lineWidth = 2
  background.stroke(path)
  piece.save()
  piece.clip(path)
  piece.drawImage(image, 0, 0, PUZZLE_WIDTH, PUZZLE_HEIGHT)
  piece.restore()
  piece.strokeStyle = "white"
  piece.lineWidth = 2
  piece.stroke(path)
  const cropY = challenge.coords_y - 2 * PIECE_RADIUS - 1
  const crop = piece.getImageData(
    challenge.coords_x - 3,
    cropY,
    PIECE_CROP_WIDTH,
    PIECE_CROP_WIDTH
  )
  canvases.piece.width = PIECE_CROP_WIDTH
  piece.putImageData(crop, 0, cropY)
}
