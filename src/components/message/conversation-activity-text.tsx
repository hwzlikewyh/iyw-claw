"use client"

export function ConversationActivityText({
  phase,
  detail,
  elapsed,
  waiting,
  visible,
}: {
  phase: string
  detail: string
  elapsed?: string | null
  waiting: boolean
  visible: boolean
}) {
  return (
    <span>
      <strong
        key={phase}
        role="status"
        aria-live={visible ? "polite" : "off"}
        aria-atomic="true"
        className={
          waiting
            ? "conversation-activity-phase conversation-waiting-label"
            : "conversation-activity-phase"
        }
      >
        {phase}
      </strong>
      {elapsed && (
        <span className="conversation-activity-elapsed">{elapsed}</span>
      )}
      {detail && <small>{detail}</small>}
    </span>
  )
}
