"use client"

import { useTranslations } from "next-intl"
import { list, record, text, numeric } from "./context"
const DAY_MS = 86400000
type Task = { name: string; start: number; end: number; progress: number }

export function Gantt({ props }: { props: Record<string, unknown> }) {
  const t = useTranslations("IntelligentUI")
  const tasks: Task[] = list(props.tasks)
    .slice(0, 100)
    .map((value) => {
      const item = record(value)
      return {
        name: text(item.name),
        start: Date.parse(text(item.start)),
        end: Date.parse(text(item.end)),
        progress: Math.max(0, Math.min(100, numeric(item.progress))),
      }
    })
    .filter(
      (task) =>
        Number.isFinite(task.start) &&
        Number.isFinite(task.end) &&
        task.end >= task.start
    )
  const milestones = list(props.milestones)
    .slice(0, 100)
    .map((value) => {
      const item = record(value)
      return { name: text(item.name), date: Date.parse(text(item.date)) }
    })
    .filter((item) => Number.isFinite(item.date))
  const dates = [
    ...tasks.flatMap((task) => [task.start, task.end]),
    ...milestones.map((item) => item.date),
  ]
  if (!dates.length)
    return <p className="text-xs text-muted-foreground">{t("empty")}</p>
  const min = Math.min(...dates),
    max = Math.max(...dates, min + DAY_MS)
  const position = (date: number) => ((date - min) / (max - min)) * 100
  return (
    <div className="space-y-2">
      <div className="flex justify-between text-xs text-muted-foreground">
        <span>{new Date(min).toISOString().slice(0, 10)}</span>
        <span>{new Date(max).toISOString().slice(0, 10)}</span>
      </div>
      {tasks.map((task, i) => (
        <TaskRow key={i} task={task} position={position} />
      ))}
      <Milestones values={milestones} position={position} />
    </div>
  )
}
function TaskRow({
  task,
  position,
}: {
  task: Task
  position: (date: number) => number
}) {
  return (
    <div className="grid grid-cols-[minmax(5rem,1fr)_3fr] gap-2 text-xs">
      <span className="truncate" title={task.name}>
        {task.name}
      </span>
      <div className="relative h-6 rounded bg-muted">
        <div
          className="absolute top-1 h-4 overflow-hidden rounded bg-primary/30"
          style={{
            left: `${position(task.start)}%`,
            width: `${Math.max(1, position(task.end) - position(task.start))}%`,
          }}
          title={`${new Date(task.start).toISOString().slice(0, 10)} → ${new Date(task.end).toISOString().slice(0, 10)} · ${task.progress}%`}
        >
          <div
            className="h-full bg-primary/60"
            style={{ width: `${task.progress}%` }}
          />
        </div>
      </div>
    </div>
  )
}

function Milestones({
  values: milestones,
  position,
}: {
  values: { name: string; date: number }[]
  position: (date: number) => number
}) {
  return (
    <div className="space-y-2">
      {milestones.map((item, i) => (
        <div
          key={i}
          className="grid grid-cols-[minmax(5rem,1fr)_3fr] gap-2 text-xs"
        >
          <span>{item.name}</span>
          <div className="relative h-5">
            <span
              style={{ left: `${position(item.date)}%` }}
              className="absolute -translate-x-1/2 text-primary"
              title={new Date(item.date).toISOString().slice(0, 10)}
            >
              ◆
            </span>
          </div>
        </div>
      ))}
    </div>
  )
}
