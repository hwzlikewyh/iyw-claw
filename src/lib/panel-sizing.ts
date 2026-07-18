export function scalePanelPixels(
  basePixels: number,
  zoomPercent: number
): number {
  return (basePixels * zoomPercent) / 100
}

export function unscalePanelPixels(
  scaledPixels: number,
  zoomPercent: number
): number {
  if (zoomPercent <= 0) return scaledPixels
  return (scaledPixels * 100) / zoomPercent
}
