"use client"

import { useCallback, useState } from "react"
import {
  cloneSnapshot,
  createEmptySnapshot,
  type EditorSnapshot,
  type EditorTool,
} from "./image-editor-model"

interface HistoryState {
  current: EditorSnapshot
  past: EditorSnapshot[]
  future: EditorSnapshot[]
}

const HISTORY_LIMIT = 50
const DEFAULT_COLOR = "#ef4444"
const DEFAULT_STROKE_WIDTH = 4
const DEFAULT_ZOOM = 1

function initialHistory(): HistoryState {
  return { current: createEmptySnapshot(), past: [], future: [] }
}

function undoHistory(state: HistoryState): HistoryState {
  const previous = state.past[state.past.length - 1]
  if (!previous) return state
  return {
    current: cloneSnapshot(previous),
    past: state.past.slice(0, -1),
    future: [cloneSnapshot(state.current), ...state.future],
  }
}

function redoHistory(state: HistoryState): HistoryState {
  const next = state.future[0]
  if (!next) return state
  return {
    current: cloneSnapshot(next),
    past: [...state.past, cloneSnapshot(state.current)],
    future: state.future.slice(1),
  }
}

function useEditorHistory() {
  const [history, setHistory] = useState<HistoryState>(initialHistory)
  const commit = useCallback((next: EditorSnapshot) => {
    setHistory((state) => ({
      current: cloneSnapshot(next),
      past: [...state.past, cloneSnapshot(state.current)].slice(-HISTORY_LIMIT),
      future: [],
    }))
  }, [])
  const undo = useCallback(() => setHistory(undoHistory), [])
  const redo = useCallback(() => setHistory(redoHistory), [])
  const resetHistory = useCallback(() => setHistory(initialHistory()), [])
  return {
    snapshot: history.current,
    canUndo: history.past.length > 0,
    canRedo: history.future.length > 0,
    commit,
    undo,
    redo,
    resetHistory,
  }
}

function useEditorControls() {
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [toolState, setToolState] = useState({
    tool: "select" as EditorTool,
    revision: 0,
  })
  const [color, setColor] = useState(DEFAULT_COLOR)
  const [strokeWidth, setStrokeWidth] = useState(DEFAULT_STROKE_WIDTH)
  const [zoom, setZoom] = useState(DEFAULT_ZOOM)
  const setTool = useCallback((tool: EditorTool) => {
    setToolState((state) => ({ tool, revision: state.revision + 1 }))
  }, [])
  const resetControls = useCallback(() => {
    setSelectedId(null)
    setToolState((state) => ({
      tool: "select",
      revision: state.revision + 1,
    }))
    setColor(DEFAULT_COLOR)
    setStrokeWidth(DEFAULT_STROKE_WIDTH)
    setZoom(DEFAULT_ZOOM)
  }, [])
  return {
    selectedId,
    setSelectedId,
    tool: toolState.tool,
    toolRevision: toolState.revision,
    setTool,
    color,
    setColor,
    strokeWidth,
    setStrokeWidth,
    zoom,
    setZoom,
    resetControls,
  }
}

interface EditorActionOptions {
  snapshot: EditorSnapshot
  selectedId: string | null
  setSelectedId: (id: string | null) => void
  commit: (snapshot: EditorSnapshot) => void
  historyUndo: () => void
  historyRedo: () => void
  resetHistory: () => void
  resetControls: () => void
}

function useEditorActions(options: EditorActionOptions) {
  const {
    snapshot,
    selectedId,
    setSelectedId,
    commit,
    historyUndo,
    historyRedo,
    resetHistory,
    resetControls,
  } = options
  const removeSelected = useCallback(() => {
    if (!selectedId) return
    commit({
      ...snapshot,
      annotations: snapshot.annotations.filter(
        (annotation) => annotation.id !== selectedId
      ),
    })
    setSelectedId(null)
  }, [commit, selectedId, setSelectedId, snapshot])
  const clearEdits = useCallback(() => {
    if (snapshot.annotations.length === 0 && !snapshot.crop) return
    commit(createEmptySnapshot())
    setSelectedId(null)
  }, [commit, setSelectedId, snapshot])
  const undo = useCallback(() => {
    historyUndo()
    setSelectedId(null)
  }, [historyUndo, setSelectedId])
  const redo = useCallback(() => {
    historyRedo()
    setSelectedId(null)
  }, [historyRedo, setSelectedId])
  const reset = useCallback(() => {
    resetHistory()
    resetControls()
  }, [resetControls, resetHistory])
  return { undo, redo, removeSelected, clearEdits, reset }
}

export function useImageEditor() {
  const history = useEditorHistory()
  const controls = useEditorControls()
  const actions = useEditorActions({
    snapshot: history.snapshot,
    selectedId: controls.selectedId,
    setSelectedId: controls.setSelectedId,
    commit: history.commit,
    historyUndo: history.undo,
    historyRedo: history.redo,
    resetHistory: history.resetHistory,
    resetControls: controls.resetControls,
  })
  return { ...history, ...controls, ...actions }
}
