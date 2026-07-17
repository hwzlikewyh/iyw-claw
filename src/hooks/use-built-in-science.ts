"use client"

import { useEffect, useState } from "react"

import { scienceList } from "@/lib/api"
import type { ScienceListItem } from "@/lib/types"

let cachedScience: ScienceListItem[] | null = null
let inflight: Promise<ScienceListItem[]> | null = null
const subscribers = new Set<(skills: ScienceListItem[]) => void>()

async function loadScience(): Promise<ScienceListItem[]> {
  if (cachedScience) return cachedScience
  if (inflight) return inflight
  inflight = scienceList()
    .then((skills) => {
      cachedScience = skills
      inflight = null
      subscribers.forEach((subscriber) => subscriber(skills))
      return skills
    })
    .catch((error) => {
      inflight = null
      throw error
    })
  return inflight
}

export function useBuiltInScience(): ScienceListItem[] {
  const [skills, setSkills] = useState<ScienceListItem[]>(
    () => cachedScience ?? []
  )

  useEffect(() => {
    let cancelled = false
    if (!cachedScience) {
      loadScience()
        .then((next) => {
          if (!cancelled) setSkills(next)
        })
        .catch((error) => {
          console.warn("[useBuiltInScience] failed to load science:", error)
        })
    }
    const onUpdate = (next: ScienceListItem[]) => {
      if (!cancelled) setSkills(next)
    }
    subscribers.add(onUpdate)
    return () => {
      cancelled = true
      subscribers.delete(onUpdate)
    }
  }, [])

  return skills
}
