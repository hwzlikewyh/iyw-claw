export function dropIndexFromMidpoints(
  clientX: number,
  midpoints: number[]
): number {
  return midpoints.reduce(
    (index, midpoint) => index + Number(midpoint < clientX),
    0
  )
}

export function clientPointFromDrag(
  event: unknown,
  pagePoint: { x: number; y: number }
): { x: number; y: number } {
  const pointer = event as { clientX?: unknown; clientY?: unknown } | null
  if (
    pointer &&
    typeof pointer.clientX === "number" &&
    typeof pointer.clientY === "number"
  ) {
    return { x: pointer.clientX, y: pointer.clientY }
  }
  return {
    x: pagePoint.x - (typeof window === "undefined" ? 0 : window.scrollX),
    y: pagePoint.y - (typeof window === "undefined" ? 0 : window.scrollY),
  }
}
