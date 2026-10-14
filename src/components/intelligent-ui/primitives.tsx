import { text, numeric, toneClass } from "./context"
type Props = { props: Record<string, unknown> }
export const PRIMITIVE_COMPONENTS = {
  text: UiText,
  badge: Badge,
  stat: Stat,
  progress: Progress,
  callout: Callout,
  divider: Divider,
}
function UiText({ props: p }: Props) {
  return (
    <p
      className={`${toneClass(p.tone)} ${p.size === "lg" ? "text-lg" : p.size === "sm" ? "text-xs" : "text-sm"} whitespace-pre-wrap break-words`}
    >
      {text(p.text)}
    </p>
  )
}
function Badge({ props: p }: Props) {
  return (
    <span
      className={`inline-flex w-fit rounded border px-2 py-0.5 text-xs ${toneClass(p.tone)}`}
    >
      {text(p.text)}
    </span>
  )
}
function Stat({ props: p }: Props) {
  const value =
    typeof p.value === "number" && typeof p.precision === "number"
      ? p.value.toFixed(Math.max(0, Math.min(12, Math.trunc(p.precision))))
      : text(p.value)
  return (
    <div>
      <p className="text-xs text-muted-foreground">{text(p.label)}</p>
      <p className={`text-xl font-semibold tabular-nums ${toneClass(p.tone)}`}>
        {value}{" "}
        <span className="text-sm font-normal">{text(p.unit ?? "")}</span>
      </p>
      {Boolean(p.note) && (
        <p className="text-xs text-muted-foreground">{text(p.note)}</p>
      )}
    </div>
  )
}
function Progress({ props: p }: Props) {
  return (
    <div className="space-y-1">
      <span className="text-xs">{text(p.label ?? "")}</span>
      <progress
        aria-label={text(p.label ?? "Progress")}
        value={numeric(p.value)}
        max={Math.max(1, numeric(p.max, 100))}
        className="w-full accent-primary"
      />
    </div>
  )
}
function Callout({ props: p }: Props) {
  return (
    <aside className={`rounded border bg-muted/30 p-3 ${toneClass(p.tone)}`}>
      {Boolean(p.title) && <strong>{text(p.title)}</strong>}
      <p>{text(p.text)}</p>
    </aside>
  )
}
function Divider() {
  return <hr className="border-border" />
}
