'use client'

// Board - the memory map as a Miro-style canvas. Takes the whole window
// (the app layout drops its sidebar, topbar and banners on this route) and
// brings its own chrome:
//
//   top-left     brand block, back to the app, which board (org or Personal)
//   left rail    Select (V), Add memory (N), Connect (C), Blast (B), Undo,
//                Redo, Tidy up
//   top-right    search, counts, Rewind
//   bottom       type legend (click to hide a type), hint strip, zoom + map
//   right        memory drawer - opens on a card click, closes on any
//                press outside it
//
// Data:
//   GET    /api/enterprise/graph          records + links (scoped: org or Personal, never both)
//   POST   /api/records                   a memory typed on the board (same path as Capture)
//   DELETE /api/records                   undoing a memory made on the board
//   POST   /api/enterprise/graph/links    a link, created only once a relation is picked
//   PATCH  /api/enterprise/graph/links    change a link's relation
//   DELETE /api/enterprise/graph/links    remove a link
//
// Memories are Obsidian-style dots settled into a brain-shaped cloud by a
// force layout (board-model.ts). Where a person drags a dot is their own
// layout, saved per browser and scope. Everything here can be undone.

import './board.css'
import galaxy from '@/assets/galaxy.jpg'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  ReactFlow, ReactFlowProvider, Background, BackgroundVariant, MiniMap, useReactFlow, useNodesState,
  ConnectionMode, MarkerType,
  type Node, type Edge, type NodeMouseHandler, type OnConnectEnd, type OnNodeDrag, type Viewport,
} from '@xyflow/react'
import '@xyflow/react/dist/style.css'
import { toast } from 'sonner'
import {
  MousePointer2, StickyNote, Spline, Undo2, Redo2, Search, X, Minus, Plus, Maximize, Bomb, Orbit, Brain,
  Map as MapIcon, RotateCcw, Loader2, AlertCircle, ChevronUp, ChevronDown, ChevronLeft, Play, Pause, Video,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import { api, type BoardData, type MemoryCard } from '@/lib/api'
import {
  TYPE_META, TYPE_ORDER, brainLayout, arrivalPoint, dotSize, seededRandom, useIsDark,
  loadPositions, savePositions, clearPositions, relationMeta,
  type GraphNode, type GraphEdge, type RecordType, type XY, type BrainLayout,
} from './board-model'
import {
  BoardCtx, MemoryDot, Connector, RelationPicker, MemoryDrawer, Composer,
  type BoardUi, type MemoryData, type LinkData,
} from './board-parts'

type Tool = 'select' | 'memory' | 'connect'
type Picker =
  | { mode: 'new'; from: string; to: string; x: number; y: number }
  | { mode: 'edit'; edgeId: string; x: number; y: number }
interface HistoryEntry { label: string; undo: () => Promise<void> | void; redo: () => Promise<void> | void }

const nodeTypes = { memory: MemoryDot }
const edgeTypes = { rel: Connector }
const DRAWER_W = 420
const COMPOSER_W = 280

// Centre of a dot from its React Flow node (position is the top-left).
function centerOf(n: Node): XY {
  const d = (n.data as MemoryData | undefined)?.size ?? n.width ?? 14
  return { x: n.position.x + d / 2, y: n.position.y + d / 2 }
}

const easeInOutCubic = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2)
const easeOutExpo = (t: number) => (t >= 1 ? 1 : 1 - Math.pow(2, -10 * t))
const easeInOutBack = (t: number) => {
  const c = 1.2 * 1.525
  return t < 0.5
    ? (Math.pow(2 * t, 2) * ((c + 1) * 2 * t - c)) / 2
    : (Math.pow(2 * t - 2, 2) * ((c + 1) * (t * 2 - 2) + c) + 2) / 2
}

function boundsOf(points: XY[], pad = 60) {
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity
  for (const p of points) {
    minX = Math.min(minX, p.x); minY = Math.min(minY, p.y)
    maxX = Math.max(maxX, p.x); maxY = Math.max(maxY, p.y)
  }
  if (!Number.isFinite(minX)) return { x: -200, y: -200, width: 400, height: 400 }
  return { x: minX - pad, y: minY - pad, width: maxX - minX + pad * 2, height: maxY - minY + pad * 2 }
}
const TIP_KEY = 'lane:board:tip:v1'
// How many memories the board draws. The layout settles 1500 dots in about
// 1.5s; past that the wait is worse than the missing tail.
const BOARD_LIMIT = 400

function isTyping(t: EventTarget | null) {
  const el = t as HTMLElement | null
  return !!el && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable)
}

function toGraphNode(r: { id: string; title?: string | null; type?: string; workspaceId: string; createdAt: string; updatedAt: string; tags?: string | string[] | null }): GraphNode {
  let tags: string[] = []
  if (Array.isArray(r.tags)) tags = r.tags
  else if (typeof r.tags === 'string') { try { tags = JSON.parse(r.tags) } catch { tags = [] } }
  return {
    id: r.id,
    title: r.title || '',
    type: (r.type as RecordType) || 'note',
    workspaceId: r.workspaceId,
    createdAt: r.createdAt,
    updatedAt: r.updatedAt,
    tags,
  }
}

export function BoardView({ onAsk, onOpenMemory, onExit }: { onAsk: (q: string) => void; onOpenMemory: (memoryId: number) => void; onExit?: () => void }) {
  return (
    <ReactFlowProvider>
      <Board onAsk={onAsk} onOpenMemory={onOpenMemory} onExit={onExit} />
    </ReactFlowProvider>
  )
}

/** Lane memories in the shape the board was written for. */
function laneType(c: MemoryCard): RecordType {
  if (c.appName === 'Meeting') return 'meeting'
  if (c.appName === 'Note' || c.appName === 'Voice note' || c.appName === 'Clipboard') return 'note'
  if (c.decisions.length > 0) return 'decision'
  if (c.kind === 'article' || c.kind === 'document') return 'insight'
  if (c.kind === 'chat' || c.kind === 'email' || c.kind === 'social') return 'context'
  if (c.kind === 'code' || c.kind === 'table' || c.kind === 'form' || c.kind === 'search') return 'context'
  return 'idea'
}

function toRecord(c: MemoryCard): GraphNode {
  return { id: `m:${c.id}`, title: c.title, type: laneType(c), workspaceId: 'personal', createdAt: new Date(c.startedAt).toISOString(), updatedAt: new Date(c.createdAt).toISOString(), tags: c.projects }
}

/** User links plus links Lane infers from shared people and topics. */
function laneLinks(data: BoardData, ids: Set<string>): GraphEdge[] {
  const out: GraphEdge[] = []
  for (const l of data.links) {
    if (ids.has(l.fromKey) && ids.has(l.toKey)) out.push({ id: String(l.id), from: l.fromKey, to: l.toKey, kind: l.label || 'related_to', weight: 0.7, manual: true })
  }
  const kindOf = new Map(data.nodes.map((e) => [e.id, e.kind]))
  const byEntity = new Map<number, string[]>()
  const all = [...data.memories.map((m) => ({ card: m.card, entityIds: m.entityIds })), ...data.placed.map((c) => ({ card: c, entityIds: data.memories.find((m) => m.card.id === c.id)?.entityIds ?? [] }))]
  for (const m of all) {
    const key = `m:${m.card.id}`
    if (!ids.has(key)) continue
    for (const e of m.entityIds) byEntity.set(e, [...(byEntity.get(e) ?? []), key])
  }
  const seen = new Set(out.map((l) => `${l.from}|${l.to}`))
  let n = 0
  for (const [entity, keys] of byEntity) {
    if (keys.length < 2 || keys.length > 12) continue
    const kind = kindOf.get(entity) === 'person' ? 'same_people' : 'same_topic'
    for (let i = 1; i < keys.length && n < 800; i++) {
      const a = keys[i - 1], b = keys[i]
      if (seen.has(`${a}|${b}`) || seen.has(`${b}|${a}`)) continue
      seen.add(`${a}|${b}`)
      out.push({ id: `auto-${entity}-${i}`, from: a, to: b, kind, weight: 0.4, manual: false })
      n++
    }
  }
  return out
}

function Board({ onAsk, onOpenMemory, onExit }: { onAsk: (q: string) => void; onOpenMemory: (memoryId: number) => void; onExit?: () => void }) {
  const activeOrgId: string | null = null
  const hydrated = true
  const scope = 'personal'
  const dark = useIsDark()
  const rf = useReactFlow()
  const wrapRef = useRef<HTMLDivElement>(null)
  const searchRef = useRef<HTMLInputElement>(null)

  // ── Data ──────────────────────────────────────────────────────────
  const [records, setRecords] = useState<GraphNode[] | null>(null)
  const [links, setLinks] = useState<GraphEdge[]>([])
  const [err, setErr] = useState<string | null>(null)
  // True when the board is showing the newest BOARD_LIMIT of a larger memory.
  const [truncated, setTruncated] = useState(false)
  const pendingRef = useRef<Map<string, string>>(new Map()) // placeholder id -> title, for notes not yet filed
  const linksRef = useRef(links)
  linksRef.current = links
  const recordsRef = useRef(records)
  recordsRef.current = records
  const scopeRef = useRef(scope)

  // ── Layout ────────────────────────────────────────────────────────
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([])
  const baseRef = useRef<BrainLayout | null>(null)
  const savedRef = useRef<Map<string, XY>>(new Map()) // dot centres the person placed
  const didFitRef = useRef(false)
  const animRef = useRef<number | null>(null)
  const [animating, setAnimating] = useState(false)
  const [blasted, setBlasted] = useState(false)
  const blastedRef = useRef(false)
  blastedRef.current = blasted
  const homeRef = useRef<Map<string, XY> | null>(null)
  const [boom, setBoom] = useState<{ x: number; y: number; key: number } | null>(null)
  const [shaking, setShaking] = useState(false)
  const [labelMode, setLabelMode] = useState<'all' | 'hubs' | 'none'>('all')

  // ── Interaction ───────────────────────────────────────────────────
  const [tool, setToolState] = useState<Tool>('select')
  const toolRef = useRef<Tool>('select')
  const setTool = useCallback((t: Tool) => { toolRef.current = t; setToolState(t) }, [])
  const [connectSource, setConnectSourceState] = useState<string | null>(null)
  const connectSourceRef = useRef<string | null>(null)
  const setConnectSource = useCallback((id: string | null) => { connectSourceRef.current = id; setConnectSourceState(id) }, [])
  const [connecting, setConnecting] = useState(false)
  const [picker, setPicker] = useState<Picker | null>(null)
  const [drawerId, setDrawerId] = useState<string | null>(null)
  const drawerIdRef = useRef<string | null>(null)
  drawerIdRef.current = drawerId
  const [hoverId, setHoverId] = useState<string | null>(null)
  /* A walk down memory lane: the camera visits memories in the order they
     happened, one at a time, with everything else dimmed. */
  const [tour, setTour] = useState<{ i: number; playing: boolean } | null>(null)
  const tourRef = useRef<{ i: number; playing: boolean } | null>(null)
  tourRef.current = tour
  const endTourRef = useRef<(() => void) | null>(null)
  const [composer, setComposer] = useState<XY | null>(null)
  const composerRef = useRef<XY | null>(null)
  composerRef.current = composer
  const [composerBusy, setComposerBusy] = useState(false)
  const [fresh, setFresh] = useState<Map<string, string>>(new Map()) // id -> title it was saved with
  const [query, setQuery] = useState('')
  const [matchIdx, setMatchIdx] = useState(0)
  const [hiddenTypes, setHiddenTypes] = useState<Set<RecordType>>(new Set())
  const [zoomPct, setZoomPct] = useState(100)
  const [showLabels, setShowLabels] = useState(true)
  const [showMap, setShowMap] = useState(true)
  const [tipOpen, setTipOpen] = useState(false)
  const refreshTimers = useRef<number[]>([])

  useEffect(() => {
    try { setTipOpen(window.localStorage.getItem(TIP_KEY) !== '1') } catch { setTipOpen(true) }
    if (window.innerWidth < 720) setShowMap(false)
  }, [])
  const dismissTip = () => { setTipOpen(false); try { window.localStorage.setItem(TIP_KEY, '1') } catch { /* ignore */ } }

  // ── History (undo / redo) ─────────────────────────────────────────
  const undoRef = useRef<HistoryEntry[]>([])
  const redoRef = useRef<HistoryEntry[]>([])
  const historyBusy = useRef(false)
  const [, setHistTick] = useState(0)
  const bumpHistory = () => setHistTick((t) => t + 1)
  const push = useCallback((e: HistoryEntry) => {
    undoRef.current.push(e)
    if (undoRef.current.length > 100) undoRef.current.shift()
    redoRef.current = []
    bumpHistory()
  }, [])
  const step = useCallback(async (dir: 'undo' | 'redo') => {
    if (historyBusy.current) return
    const from = dir === 'undo' ? undoRef.current : redoRef.current
    const to = dir === 'undo' ? redoRef.current : undoRef.current
    const e = from.pop()
    if (!e) return
    historyBusy.current = true
    bumpHistory()
    try {
      await (dir === 'undo' ? e.undo() : e.redo())
      to.push(e)
      toast(`${dir === 'undo' ? 'Undid' : 'Redid'}: ${e.label}`, { duration: 1400 })
    } catch {
      toast.error(`Could not ${dir} that.`)
    } finally {
      historyBusy.current = false
      bumpHistory()
    }
  }, [])

  // ── Load ──────────────────────────────────────────────────────────
  const load = useCallback(async (opts?: { quiet?: boolean }) => {
    const forScope = scopeRef.current
    try {
      const data = await api.board(undefined, 300, 1, BOARD_LIMIT)
      if (scopeRef.current !== forScope) return
      setErr(null)
      const cards = new Map<number, MemoryCard>()
      for (const m of data.memories) cards.set(m.card.id, m.card)
      for (const c of data.placed) cards.set(c.id, c)
      const nodes = Array.from(cards.values()).map(toRecord)
      // A note typed on the board becomes a memory a little later; when it
      // has, the placeholder gives way and keeps its place.
      const byActivity = new Map(Array.from(cards.values()).map((c) => [c.activityId, c.id]))
      for (const [pid] of pendingRef.current) {
        const activityId = Number(pid.replace(/^p:/, ''))
        const memId = byActivity.get(activityId)
        if (memId != null) {
          const pos = savedRef.current.get(pid)
          if (pos) { savedRef.current.set(`m:${memId}`, pos); savedRef.current.delete(pid); savePositions(scopeRef.current, savedRef.current) }
          pendingRef.current.delete(pid)
        }
      }
      for (const [pid, title] of pendingRef.current) {
        nodes.unshift({ id: pid, title, type: 'note', workspaceId: 'personal', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), tags: [] })
      }
      setTruncated(data.memories.length >= BOARD_LIMIT)
      setRecords(nodes)
      const ids = new Set(nodes.map((n) => n.id))
      setLinks((prev) => [...laneLinks(data, ids), ...prev.filter((l) => l.id.startsWith('tmp-'))])
      setFresh((prev) => {
        if (prev.size === 0) return prev
        const next = new Map(prev)
        for (const n of nodes) {
          const t0 = next.get(n.id)
          if (t0 !== undefined && n.title !== t0) next.delete(n.id)
        }
        for (const k of Array.from(next.keys())) if (!ids.has(k)) next.delete(k)
        return next.size === prev.size ? prev : next
      })
    } catch (e) {
      if (!opts?.quiet) setErr((e as Error).message || 'failed')
    }
  }, [])

  useEffect(() => {
    const un = api.onMemoriesChanged(() => load({ quiet: true }))
    return () => { un.then((f) => f()) }
  }, [load])

  useEffect(() => {
    if (!hydrated) return
    scopeRef.current = scope
    baseRef.current = null
    didFitRef.current = false
    homeRef.current = null
    setBlasted(false)
    savedRef.current = loadPositions(scope)
    undoRef.current = []
    redoRef.current = []
    setRecords(null)
    setLinks([])
    setNodes([])
    setDrawerId(null)
    setPicker(null)
    setComposer(null)
    setFresh(new Map())
    load()
  }, [hydrated, scope]) // eslint-disable-line react-hooks/exhaustive-deps

  // Pick up memories that arrive from elsewhere (extension, integrations)
  // when the person comes back to the tab.
  useEffect(() => {
    const onFocus = () => { if (document.visibilityState === 'visible') load({ quiet: true }) }
    window.addEventListener('focus', onFocus)
    return () => window.removeEventListener('focus', onFocus)
  }, [load])

  useEffect(() => () => { refreshTimers.current.forEach((t) => window.clearTimeout(t)) }, [])
  const scheduleRefresh = useCallback(() => {
    for (const ms of [4000, 10000, 20000, 40000]) {
      refreshTimers.current.push(window.setTimeout(() => load({ quiet: true }), ms))
    }
  }, [load])

  // ── Build the React Flow nodes ────────────────────────────────────
  const degree = useMemo(() => {
    const m = new Map<string, number>()
    for (const l of links) {
      m.set(l.from, (m.get(l.from) ?? 0) + 1)
      m.set(l.to, (m.get(l.to) ?? 0) + 1)
    }
    return m
  }, [links])

  // Hubs are the best-connected ~5% (at least 4 links); their titles stay
  // up when the rest thin out, the way a star chart names only the bright ones.
  const hubCut = useMemo(() => {
    const ds = Array.from(degree.values()).sort((a, b) => b - a)
    return Math.max(4, ds[Math.floor(ds.length * 0.05)] ?? 4)
  }, [degree])

  const typeCounts = useMemo(() => {
    const m = new Map<RecordType, number>()
    for (const r of records ?? []) m.set(r.type, (m.get(r.type) ?? 0) + 1)
    return m
  }, [records])

  useEffect(() => {
    if (!records) return
    if (!baseRef.current) baseRef.current = brainLayout(records, linksRef.current)
    const base = baseRef.current
    setNodes((prev) => {
      const prevById = new Map(prev.map((n) => [n.id, n]))
      return records.map((r): Node<MemoryData> => {
        const deg = degree.get(r.id) ?? 0
        const size = dotSize(deg)
        const was = prevById.get(r.id)
        // Keep the dot's centre where it is; only its size may change.
        const c = (was ? centerOf(was) : null)
          ?? savedRef.current.get(r.id)
          ?? base.centers.get(r.id)
          ?? arrivalPoint(base, r.type, r.id)
        return {
          id: r.id,
          type: 'memory',
          position: { x: c.x - size / 2, y: c.y - size / 2 },
          width: size,
          height: size,
          zIndex: deg >= 4 ? 2 : 1,
          selected: was?.selected ?? false,
          hidden: hiddenTypes.has(r.type),
          data: { title: r.title, type: r.type, updatedAt: r.updatedAt, degree: deg, size, hub: deg >= hubCut, fresh: fresh.has(r.id) },
        }
      })
    })
  }, [records, degree, hubCut, hiddenTypes, fresh, setNodes])

  // ── Motion ────────────────────────────────────────────────────────
  // Every move of many dots (tidy, undo, blast, rearrange) is a JS tween.
  // Small moves go through React state so connectors stretch along with the
  // dots. Big ones (over LIVE_LIMIT dots) move the dot elements directly and
  // commit positions once at the end - React re-rendering hundreds of dots
  // and links per frame cannot hold 60fps - with connectors faded out in
  // flight and back in on landing.
  const LIVE_LIMIT = 60
  const flightRef = useRef<Map<string, XY> | null>(null) // live top-left positions mid-flight
  const [directFlight, setDirectFlight] = useState(false)
  const animateTo = useCallback((targets: Map<string, XY>, opts: {
    duration?: number
    ease?: (t: number) => number
    delay?: (id: string) => number
    swirl?: number
    onDone?: () => void
  } = {}) => {
    const memNodes = rf.getNodes().filter((n) => n.type === 'memory')
    const size = new Map(memNodes.map((n) => [n.id, (n.data as MemoryData).size]))
    // Start from where dots are right now, even mid-flight.
    const start = new Map<string, XY>()
    for (const n of memNodes) {
      const live = flightRef.current?.get(n.id)
      const d = size.get(n.id) ?? 14
      start.set(n.id, live ? { x: live.x + d / 2, y: live.y + d / 2 } : centerOf(n))
    }
    if (animRef.current) {
      cancelAnimationFrame(animRef.current)
      animRef.current = null
    }
    const moving = Array.from(targets.keys()).filter((id) => {
      const f = start.get(id), t = targets.get(id)
      return f && t && (Math.abs(f.x - t.x) > 0.5 || Math.abs(f.y - t.y) > 0.5)
    })
    if (moving.length === 0) { opts.onDone?.(); return }
    const direct = moving.length > LIVE_LIMIT
    const els = new Map<string, HTMLElement>()
    if (direct) {
      wrapRef.current?.querySelectorAll<HTMLElement>('.react-flow__node-memory').forEach((el) => {
        const id = el.getAttribute('data-id')
        if (id) els.set(id, el)
      })
    }
    const dur = opts.duration ?? 600
    const ease = opts.ease ?? easeInOutCubic
    const delays = new Map<string, number>()
    let maxDelay = 0
    for (const id of moving) { const d = opts.delay?.(id) ?? 0; delays.set(id, d); maxDelay = Math.max(maxDelay, d) }
    const live = new Map<string, XY>()
    flightRef.current = live
    const at = (id: string, el: number): XY => {
      const from = start.get(id)!, to = targets.get(id)!
      const k = ease(Math.min(1, Math.max(0, (el - (delays.get(id) ?? 0)) / dur)))
      let x = from.x + (to.x - from.x) * k
      let y = from.y + (to.y - from.y) * k
      if (opts.swirl) {
        // Bow the path sideways so dots travel on arcs, not straight lines.
        const bow = Math.sin(Math.PI * Math.min(1, Math.max(0, k))) * opts.swirl
        x += -(to.y - from.y) * bow
        y += (to.x - from.x) * bow
      }
      const d = size.get(id) ?? 14
      return { x: x - d / 2, y: y - d / 2 }
    }
    const commit = (pos: Map<string, XY>) =>
      setNodes((ns) => ns.map((n) => { const p = pos.get(n.id); return p ? { ...n, position: p } : n }))

    const t0 = performance.now()
    setAnimating(true)
    setDirectFlight(direct)
    const tick = (now: number) => {
      const el = now - t0
      for (const id of moving) live.set(id, at(id, el))
      if (direct) {
        live.forEach((p, id) => { const e = els.get(id); if (e) e.style.transform = `translate(${p.x}px, ${p.y}px)` })
      } else {
        commit(live)
      }
      if (el < dur + maxDelay) {
        animRef.current = requestAnimationFrame(tick)
      } else {
        animRef.current = null
        const final = new Map<string, XY>()
        for (const id of moving) final.set(id, at(id, dur + maxDelay))
        commit(final)
        flightRef.current = null
        setAnimating(false)
        setDirectFlight(false)
        opts.onDone?.()
      }
    }
    animRef.current = requestAnimationFrame(tick)
  }, [rf, setNodes])
  useEffect(() => () => { if (animRef.current) cancelAnimationFrame(animRef.current) }, [])

  // Fit the whole board once, on first paint of each scope. The timer is
  // not cancelled by later node updates (React Flow measures cards right
  // after mount), only by leaving the page.
  const fitTimer = useRef<number | null>(null)
  useEffect(() => () => { if (fitTimer.current) window.clearTimeout(fitTimer.current) }, [])
  useEffect(() => {
    if (didFitRef.current || !records || nodes.length === 0) return
    didFitRef.current = true
    fitTimer.current = window.setTimeout(() => {
      rf.fitView({ padding: window.innerWidth < 720 ? 0.06 : 0.14, maxZoom: 1.1 })
    }, 60)
  }, [records, nodes.length, rf])

  // ── Edges ─────────────────────────────────────────────────────────
  const hiddenIds = useMemo(() => {
    const s = new Set<string>()
    if (hiddenTypes.size === 0) return s
    for (const r of records ?? []) if (hiddenTypes.has(r.type)) s.add(r.id)
    return s
  }, [records, hiddenTypes])

  const edges = useMemo<Edge<LinkData>[]>(() => {
    const out: Edge<LinkData>[] = links.map((l) => {
      const color = relationMeta(l.kind).color
      return {
        id: l.id,
        source: l.from,
        target: l.to,
        type: 'rel',
        hidden: hiddenIds.has(l.from) || hiddenIds.has(l.to),
        data: { kind: l.kind, manual: l.manual },
        markerEnd: { type: MarkerType.ArrowClosed, color, width: 13, height: 13 },
      }
    })
    if (picker?.mode === 'new') {
      out.push({
        id: '__pending', source: picker.from, target: picker.to, type: 'rel',
        data: { kind: 'related_to', pending: true },
        markerEnd: { type: MarkerType.ArrowClosed, color: '#4262ff', width: 16, height: 16 },
      })
    }
    return out
  }, [links, picker, hiddenIds])

  // ── Link operations ───────────────────────────────────────────────
  const findLinkId = (from: string, to: string) =>
    linksRef.current.find((l) => l.from === from && l.to === to && !l.id.startsWith('tmp-'))?.id

  const apiCreateLink = useCallback(async (from: string, to: string, kind: string): Promise<string | null> => {
    const temp = `tmp-${Math.random().toString(36).slice(2)}`
    setLinks((prev) => [...prev, { id: temp, from, to, kind, weight: 0.7, manual: true }])
    try {
      const id = String(await api.addBoardLink(from, to, kind))
      setLinks((prev) => prev.some((l) => l.id === temp)
        ? prev.map((l) => (l.id === temp ? { ...l, id } : l))
        : prev.some((l) => l.id === id) ? prev : [...prev, { id, from, to, kind, weight: 0.7, manual: true }])
      return id
    } catch (e) {
      setLinks((prev) => prev.filter((l) => l.id !== temp))
      toast.error(`Could not save that link: ${String(e)}`)
      return null
    }
  }, [])

  const apiDeleteLink = useCallback(async (id: string): Promise<boolean> => {
    const was = linksRef.current.find((l) => l.id === id)
    setLinks((prev) => prev.filter((l) => l.id !== id))
    if (id.startsWith('auto-')) return true
    try {
      await api.removeBoardLink(Number(id))
      return true
    } catch (e) {
      if (was) setLinks((prev) => [...prev, was])
      toast.error(`Could not remove that link: ${String(e)}`)
      return false
    }
  }, [])

  const apiSetKind = useCallback(async (id: string, kind: string): Promise<boolean> => {
    const was = linksRef.current.find((l) => l.id === id)
    if (!was) return false
    if (id.startsWith('auto-')) {
      // Naming an inferred link makes it yours.
      setLinks((prev) => prev.filter((l) => l.id !== id))
      return (await apiCreateLink(was.from, was.to, kind)) != null
    }
    setLinks((prev) => prev.map((l) => (l.id === id ? { ...l, kind, manual: true } : l)))
    try {
      await api.setBoardLinkLabel(Number(id), kind)
      return true
    } catch (e) {
      setLinks((prev) => prev.map((l) => (l.id === id ? was : l)))
      toast.error(`Could not change that link: ${String(e)}`)
      return false
    }
  }, [apiCreateLink])

  const openNewLink = useCallback((from: string, to: string, x: number, y: number) => {
    if (from === to) return
    const existing = linksRef.current.find((l) => (l.from === from && l.to === to) || (l.from === to && l.to === from))
    setDrawerId(null)
    if (existing && !existing.id.startsWith('tmp-')) {
      setPicker({ mode: 'edit', edgeId: existing.id, x, y })
      return
    }
    setPicker({ mode: 'new', from, to, x, y })
  }, [])

  const titleOf = useCallback((id: string) => recordsRef.current?.find((r) => r.id === id)?.title ?? null, [])

  const pickRelation = useCallback(async (kind: string) => {
    const p = picker
    if (!p) return
    setPicker(null)
    if (p.mode === 'new') {
      const { from, to } = p
      const id = await apiCreateLink(from, to, kind)
      if (!id) return
      push({
        label: `link (${relationMeta(kind).label})`,
        undo: async () => { const cur = findLinkId(from, to); if (cur) await apiDeleteLink(cur) },
        redo: async () => { await apiCreateLink(from, to, kind) },
      })
      return
    }
    const link = linksRef.current.find((l) => l.id === p.edgeId)
    if (!link || link.kind === kind) return
    const { from, to } = link
    const prevKind = link.kind
    if (!(await apiSetKind(link.id, kind))) return
    push({
      label: `relation (${relationMeta(kind).label})`,
      undo: async () => { const cur = findLinkId(from, to); if (cur) await apiSetKind(cur, prevKind) },
      redo: async () => { const cur = findLinkId(from, to); if (cur) await apiSetKind(cur, kind) },
    })
  }, [picker, apiCreateLink, apiDeleteLink, apiSetKind, push])

  const removePickedLink = useCallback(async () => {
    if (picker?.mode !== 'edit') return
    const link = linksRef.current.find((l) => l.id === picker.edgeId)
    setPicker(null)
    if (!link) return
    const { from, to, kind } = link
    if (!(await apiDeleteLink(link.id))) return
    push({
      label: 'remove link',
      undo: async () => { await apiCreateLink(from, to, kind) },
      redo: async () => { const cur = findLinkId(from, to); if (cur) await apiDeleteLink(cur) },
    })
  }, [picker, apiCreateLink, apiDeleteLink, push])

  const reversePickedLink = useCallback(async () => {
    if (picker?.mode !== 'edit') return
    const link = linksRef.current.find((l) => l.id === picker.edgeId)
    setPicker(null)
    if (!link) return
    const { from, to, kind } = link
    if (!(await apiDeleteLink(link.id))) return
    if (!(await apiCreateLink(to, from, kind))) { await apiCreateLink(from, to, kind); return }
    const flip = async (a: string, b: string) => {
      const cur = findLinkId(a, b)
      if (cur && (await apiDeleteLink(cur))) await apiCreateLink(b, a, kind)
    }
    push({ label: 'reverse link', undo: () => flip(to, from), redo: () => flip(from, to) })
  }, [picker, apiCreateLink, apiDeleteLink, push])

  // ── Memories ──────────────────────────────────────────────────────
  const placeComposer = useCallback((clientX: number, clientY: number) => {
    const p = rf.screenToFlowPosition({ x: clientX, y: clientY })
    setComposer({ x: p.x - COMPOSER_W / 2, y: p.y - 28 })
    setTool('select')
    setDrawerId(null)
    setPicker(null)
    if (rf.getZoom() < 0.8) rf.setCenter(p.x, p.y + 60, { zoom: 1, duration: 350 })
  }, [rf, setTool])

  const addLocalRecord = useCallback((rec: GraphNode, pos: XY) => {
    savedRef.current.set(rec.id, pos)
    savePositions(scopeRef.current, savedRef.current)
    setFresh((prev) => new Map(prev).set(rec.id, rec.title))
    setRecords((prev) => (prev && !prev.some((r) => r.id === rec.id) ? [rec, ...prev] : prev ?? [rec]))
  }, [])

  const removeLocalRecord = useCallback((id: string) => {
    setRecords((prev) => prev?.filter((r) => r.id !== id) ?? prev)
    setLinks((prev) => prev.filter((l) => l.from !== id && l.to !== id))
    setNodes((prev) => prev.filter((n) => n.id !== id))
    if (drawerIdRef.current === id) setDrawerId(null)
  }, [setNodes])

  const postMemory = useCallback(async (content: string): Promise<{ rec: GraphNode; dedup: boolean } | null> => {
    try {
      const activityId = await api.addNote(content)
      const id = `p:${activityId}`
      const title = content.split('\n').find((l) => l.trim())?.slice(0, 80) ?? 'Note'
      pendingRef.current.set(id, title)
      return { rec: { id, title, type: 'note', workspaceId: 'personal', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), tags: [] }, dedup: false }
    } catch (e) {
      toast.error(`Could not save the note: ${String(e)}`)
      return null
    }
  }, [])

  const goTo = useCallback((id: string, open = true) => {
    const n = rf.getNode(id)
    if (!n) { toast('That memory is not on this board.'); return }
    const zoom = Math.max(rf.getZoom(), 0.9)
    const shift = open && window.innerWidth > 720 ? DRAWER_W / 2 / zoom : 0
    const c = centerOf(n)
    rf.setCenter(c.x + shift, c.y, { zoom, duration: 450 })
    if (open) setDrawerId(id)
  }, [rf])

  const createMemory = useCallback(async (text: string) => {
    const at = composerRef.current
    if (!at) return
    setComposerBusy(true)
    try {
      const out = await postMemory(text)
      if (!out) return
      setComposer(null)
      if (out.dedup) {
        toast('That exact text is already saved.')
        if (rf.getNode(out.rec.id)) goTo(out.rec.id)
        return
      }
      const pos = { x: at.x + COMPOSER_W / 2, y: at.y + 20 } // where the new dot's centre lands
      let id = out.rec.id
      addLocalRecord(out.rec, pos)
      toast.success('Saved. Rabbit is filing it now.', { duration: 2200 })
      scheduleRefresh()
      push({
        label: 'add memory',
        undo: async () => {
          const activityId = Number(id.replace(/^[pm]:/, ''))
          const rec = recordsRef.current?.find((r) => r.id === id)
          // A filed note is a memory: its activity id is known only through the card.
          const target = id.startsWith('p:') ? activityId : (await api.memoryDetail(activityId)).card.activityId
          await api.deleteActivity(target)
          pendingRef.current.delete(id)
          void rec
          removeLocalRecord(id)
        },
        redo: async () => {
          const again = await postMemory(text)
          if (!again) throw new Error('create failed')
          id = again.rec.id
          addLocalRecord(again.rec, pos)
          scheduleRefresh()
        },
      })
    } finally {
      setComposerBusy(false)
    }
  }, [postMemory, addLocalRecord, removeLocalRecord, scheduleRefresh, push, goTo, rf])

  // ── Tidy up ───────────────────────────────────────────────────────
  // Positions here are dot centres.
  const arrange = useCallback((centers: Map<string, XY>, duration = 520) => {
    animateTo(centers, { duration, ease: easeInOutCubic })
  }, [animateTo])

  const tidy = useCallback(() => {
    if (!records?.length) return
    const before = {
      base: baseRef.current,
      saved: new Map(savedRef.current),
      pos: new Map(rf.getNodes().filter((n) => n.type === 'memory').map((n) => [n.id, centerOf(n)])),
    }
    const apply = () => {
      const base = brainLayout(recordsRef.current ?? [], linksRef.current)
      baseRef.current = base
      savedRef.current = new Map()
      clearPositions(scopeRef.current)
      homeRef.current = null
      setBlasted(false)
      animateTo(base.centers, { duration: 900, ease: easeInOutCubic, swirl: 0.08 })
      rf.fitBounds(boundsOf(Array.from(base.centers.values())), { padding: 0.14, duration: 900 })
    }
    apply()
    push({
      label: 'tidy up',
      undo: () => {
        baseRef.current = before.base
        savedRef.current = new Map(before.saved)
        savePositions(scopeRef.current, savedRef.current)
        arrange(before.pos, 700)
      },
      redo: apply,
    })
  }, [records, animateTo, arrange, push, rf])

  // ── Blast ─────────────────────────────────────────────────────────
  // One button, two moves. First press: a shockwave from the middle of the
  // cloud flings every dot outward on curved paths, inner ones first. Second
  // press: they fly home to exactly where they were. The scattered state is
  // never saved; drags made while scattered are dropped on the way back.
  const blast = useCallback(() => {
    if (animRef.current) return
    const mem = rf.getNodes().filter((n) => n.type === 'memory' && !n.hidden)
    if (!mem.length) return

    if (!blastedRef.current) {
      const home = new Map(mem.map((n) => [n.id, centerOf(n)]))
      homeRef.current = home
      let cx = 0, cy = 0
      home.forEach((p) => { cx += p.x; cy += p.y })
      cx /= home.size; cy /= home.size
      let R = 0
      home.forEach((p) => { R = Math.max(R, Math.hypot(p.x - cx, p.y - cy)) })
      R = Math.max(R, 220)
      const rand = seededRandom(Math.floor(performance.now()))
      const targets = new Map<string, XY>()
      const delays = new Map<string, number>()
      home.forEach((p, id) => {
        const dx = p.x - cx, dy = p.y - cy
        const d = Math.hypot(dx, dy)
        const ang = (d > 1 ? Math.atan2(dy, dx) : rand() * Math.PI * 2) + (rand() - 0.5) * 1.2
        const dist = d * 0.8 + R * (0.55 + 2.7 * Math.pow(rand(), 0.7))
        targets.set(id, { x: cx + Math.cos(ang) * dist, y: cy + Math.sin(ang) * dist })
        delays.set(id, (d / R) * 170 + rand() * 70)
      })
      const scr = rf.flowToScreenPosition({ x: cx, y: cy })
      const box = wrapRef.current?.getBoundingClientRect()
      setBoom({ x: scr.x - (box?.left ?? 0), y: scr.y - (box?.top ?? 0), key: Date.now() })
      setShaking(true)
      window.setTimeout(() => setShaking(false), 520)
      window.setTimeout(() => setBoom(null), 1500)
      setDrawerId(null)
      setPicker(null)
      setBlasted(true)
      animateTo(targets, { duration: 1500, ease: easeOutExpo, delay: (id) => delays.get(id) ?? 0, swirl: 0.14 })
      rf.fitBounds(boundsOf(Array.from(targets.values()), 80), { padding: 0.08, duration: 1500 })
      return
    }

    const home = homeRef.current ?? new Map<string, XY>()
    // Anything that arrived while scattered goes to its layout spot.
    for (const n of mem) {
      if (home.has(n.id)) continue
      const b = baseRef.current
      const r = recordsRef.current?.find((x) => x.id === n.id)
      if (b && r) home.set(n.id, b.centers.get(n.id) ?? arrivalPoint(b, r.type, r.id))
    }
    const rand = seededRandom(Math.floor(performance.now()))
    const delays = new Map<string, number>()
    home.forEach((_, id) => delays.set(id, rand() * 320))
    setBlasted(false)
    homeRef.current = null
    animateTo(home, { duration: 1250, ease: easeInOutBack, delay: (id) => delays.get(id) ?? 0, swirl: -0.2 })
    rf.fitBounds(boundsOf(Array.from(home.values())), { padding: 0.14, duration: 1400 })
  }, [rf, animateTo])

  // ── Canvas handlers ───────────────────────────────────────────────
  const dragStart = useRef<Map<string, XY>>(new Map())
  const onNodeDragStart: OnNodeDrag = useCallback((_e, _n, dragged) => {
    dragStart.current = new Map(dragged.filter((n) => n.type === 'memory').map((n) => [n.id, centerOf(n)]))
  }, [])
  const onNodeDragStop: OnNodeDrag = useCallback((_e, _n, dragged) => {
    if (blastedRef.current) return // scattered positions are never kept
    const moves = dragged
      .filter((n) => n.type === 'memory')
      .map((n) => ({ id: n.id, from: dragStart.current.get(n.id), to: centerOf(n) }))
      .filter((m): m is { id: string; from: XY; to: XY } => !!m.from && (m.from.x !== m.to.x || m.from.y !== m.to.y))
    if (!moves.length) return
    for (const m of moves) savedRef.current.set(m.id, m.to)
    savePositions(scopeRef.current, savedRef.current)
    const put = (which: 'from' | 'to') => {
      const map = new Map(moves.map((m) => [m.id, m[which]]))
      map.forEach((p, id) => savedRef.current.set(id, p))
      savePositions(scopeRef.current, savedRef.current)
      arrange(map)
    }
    push({ label: moves.length > 1 ? `move ${moves.length} memories` : 'move memory', undo: () => put('from'), redo: () => put('to') })
  }, [arrange, push])

  const onNodeClick: NodeMouseHandler = useCallback((e, node) => {
    if (node.type !== 'memory') return
    if (toolRef.current === 'connect') {
      const src = connectSourceRef.current
      if (!src) { setConnectSource(node.id); return }
      if (src === node.id) { setConnectSource(null); return }
      openNewLink(src, node.id, e.clientX, e.clientY)
      setConnectSource(null)
      setTool('select')
      return
    }
    setPicker(null)
    setDrawerId(node.id)
    // Slide the board left if the drawer would land on top of the card.
    const vp = rf.getViewport()
    const right = (node.position.x + ((node.data as MemoryData).size ?? 20) + 90) * vp.zoom + vp.x
    const limit = window.innerWidth - DRAWER_W - 32
    if (window.innerWidth > 720 && right > limit) {
      rf.setViewport({ x: vp.x - (right - limit), y: vp.y, zoom: vp.zoom }, { duration: 300 })
    }
  }, [openNewLink, setConnectSource, setTool, rf])

  const onPaneClick = useCallback((e: React.MouseEvent) => {
    if (toolRef.current === 'memory') { placeComposer(e.clientX, e.clientY); return }
    if (toolRef.current === 'connect') setConnectSource(null)
  }, [placeComposer, setConnectSource])

  const onConnectEnd: OnConnectEnd = useCallback((event, state) => {
    setConnecting(false)
    const from = state.fromNode?.id
    if (!from) return
    const pt = 'changedTouches' in event ? event.changedTouches[0] : event
    // Dropping anywhere on a dot or its label counts, not just the handle.
    let to: string | undefined = state.isValid ? state.toNode?.id : undefined
    if (!to) {
      const el = document.elementFromPoint(pt.clientX, pt.clientY) as HTMLElement | null
      to = el?.closest('.react-flow__node-memory')?.getAttribute('data-id') ?? undefined
    }
    if (!to || to === from) return
    openNewLink(from, to, pt.clientX, pt.clientY)
  }, [openNewLink])

  const onMove = useCallback((_: unknown, vp: Viewport) => {
    const el = wrapRef.current
    if (!el) return
    el.style.setProperty('--rb-label-scale', String(Math.min(6, Math.max(1, 1 / vp.zoom))))
    el.style.setProperty('--rb-dot-label-scale', String(Math.min(2.2, Math.max(1, 1 / vp.zoom))))
  }, [])
  // Relation labels keep a steady on-screen size (the CSS scale above).
  // Like Obsidian, a zoomed-out brain shows the shape, not the words: all
  // relation labels come in close up (or on a small board), and hovering or
  // opening a memory always shows its own. Dot titles thin out the same way:
  // everything close up, only well-linked memories mid-way, none far out.
  const linkCountRef = useRef(0)
  linkCountRef.current = links.length
  const zoomRef = useRef(1)
  const onMoveEnd = useCallback((_: unknown, vp: Viewport) => {
    zoomRef.current = vp.zoom
    setZoomPct(Math.round(vp.zoom * 100))
    // Titles need ~110px of screen room each; show them only when the dots
    // are that far apart on screen (hubs need less, they are few). Relation
    // labels need more still, unless the board has only a handful of links.
    const room = (baseRef.current?.spacing ?? 90) * vp.zoom
    setShowLabels(room >= 150 || linkCountRef.current <= 12)
    setLabelMode(room >= 140 ? 'all' : room >= 82 ? 'hubs' : 'none')
  }, [])
  useEffect(() => {
    const room = (baseRef.current?.spacing ?? 90) * zoomRef.current
    setShowLabels(room >= 150 || links.length <= 12)
  }, [links.length])

  const onEdgeLabel = useCallback((edgeId: string, x: number, y: number) => {
    setDrawerId(null)
    setPicker({ mode: 'edit', edgeId, x, y })
  }, [])

  // ── Search ────────────────────────────────────────────────────────
  const q = query.trim().toLowerCase()
  const matches = useMemo(
    () => (q ? (records ?? []).filter((r) => !hiddenTypes.has(r.type) && r.title.toLowerCase().includes(q)) : []),
    [q, records, hiddenTypes],
  )
  useEffect(() => { setMatchIdx(0) }, [q])
  const jump = (dir: 1 | -1) => {
    if (!matches.length) return
    const i = ((matchIdx % matches.length) + matches.length) % matches.length
    goTo(matches[i].id)
    setMatchIdx(i + dir)
  }

  // ── Keyboard ──────────────────────────────────────────────────────
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = isTyping(e.target)
      const meta = e.metaKey || e.ctrlKey
      if (meta && !typing && e.key.toLowerCase() === 'z') { e.preventDefault(); step(e.shiftKey ? 'redo' : 'undo'); return }
      if (meta && !typing && e.key.toLowerCase() === 'y') { e.preventDefault(); step('redo'); return }
      if (typing || meta || e.altKey) return
      switch (e.key) {
        case 'v': case 'V': setTool('select'); setConnectSource(null); break
        case 'n': case 'N': setTool('memory'); setConnectSource(null); break
        case 'c': case 'C': setTool('connect'); setConnectSource(drawerIdRef.current); setDrawerId(null); break
        case 'f': case 'F': rf.fitView({ padding: 0.14, maxZoom: 1.1, duration: 400 }); break
        case ' ': if (tourRef.current) { e.preventDefault(); setTour((t) => (t ? { ...t, playing: !t.playing } : t)) } break
        case 'b': case 'B': blast(); break
        case '=': case '+': rf.zoomIn({ duration: 180 }); break
        case '-': case '_': rf.zoomOut({ duration: 180 }); break
        case '/': e.preventDefault(); searchRef.current?.focus(); break
        case 'Escape':
          if (tourRef.current) { endTourRef.current?.(); break }
          if (toolRef.current !== 'select' || connectSourceRef.current) { setTool('select'); setConnectSource(null) }
          break
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [rf, step, setTool, setConnectSource, blast])

  // ── Shared UI state for cards and connectors ──────────────────────
  /* Up to 28 stops, spread evenly across the whole history so the walk
     covers the months rather than the busiest week, preferring the
     best-connected memory inside each slice. */
  const stops = useMemo(() => {
    const all = (records ?? []).filter((r) => !hiddenTypes.has(r.type))
    if (all.length === 0) return [] as GraphNode[]
    const sorted = [...all].sort((a, b) => new Date(a.updatedAt).getTime() - new Date(b.updatedAt).getTime())
    const want = Math.min(28, sorted.length)
    const per = sorted.length / want
    const picked: GraphNode[] = []
    for (let k = 0; k < want; k++) {
      const slice = sorted.slice(Math.floor(k * per), Math.max(Math.floor((k + 1) * per), Math.floor(k * per) + 1))
      const best = slice.reduce((a, b) => ((degree.get(b.id) ?? 0) > (degree.get(a.id) ?? 0) ? b : a), slice[0])
      if (best && !picked.some((p) => p.id === best.id)) picked.push(best)
    }
    return picked
  }, [records, hiddenTypes, degree])

  const startTour = useCallback((record = false) => {
    if (stops.length === 0) { toast.message('Nothing to walk through yet'); return }
    api.setFullscreen(true).catch(() => {})
    setDrawerId(null)
    setTour({ i: 0, playing: true })
    if (record) {
      // The walk is 5.2s a memory plus the glide in and the last beat.
      const seconds = Math.ceil(stops.length * 5.2 + 4)
      const name = `Memory lane ${new Date().toISOString().slice(0, 10)}`
      toast.message('Recording the walk', { description: 'macOS may ask for Screen Recording the first time.' })
      api.recordScreen(seconds, name)
        .then((path) => toast.success('Walk saved', { description: path, action: { label: 'Show', onClick: () => api.revealFile(path) } }))
        .catch((e) => toast.error(String(e)))
    }
  }, [stops.length])

  const endTour = useCallback(() => {
    setTour(null)
    api.setFullscreen(false).catch(() => {})
    rf.fitView({ padding: 0.14, maxZoom: 1.1, duration: 700 })
  }, [rf])
  endTourRef.current = endTour

  /* Each stop: glide, hold, move on. Six seconds a memory. */
  useEffect(() => {
    if (!tour?.playing) return
    const stop = stops[tour.i]
    if (!stop) { endTour(); return }
    const node = rf.getNode(stop.id)
    if (node) rf.setCenter(node.position.x, node.position.y, { zoom: 1.45, duration: 1600 })
    const t = setTimeout(() => {
      setTour((cur) => (cur && cur.playing ? (cur.i + 1 < stops.length ? { ...cur, i: cur.i + 1 } : null) : cur))
    }, 5200)
    return () => clearTimeout(t)
  }, [tour?.i, tour?.playing, stops, rf, endTour])

  /* Leaving the walk also leaves full screen, however it ends. */
  useEffect(() => {
    if (tour !== null) return
    api.setFullscreen(false).catch(() => {})
  }, [tour])

  // Hovering a memory (or opening it) lights it and its neighbours and
  // dims the rest, the Obsidian way.
  const focusId = (tour ? stops[tour.i]?.id ?? null : null) ?? hoverId ?? drawerId
  const focus = useMemo(() => {
    if (!focusId) return null
    const s = new Set([focusId])
    for (const l of links) {
      if (l.from === focusId) s.add(l.to)
      if (l.to === focusId) s.add(l.from)
    }
    return s
  }, [focusId, links])

  const ui = useMemo<BoardUi>(() => ({
    focus,
    focusId,
    hoverId,
    query: q,
    showLabels,
    connectSource,
    drawerId,
    selectedEdgeId: picker?.mode === 'edit' ? picker.edgeId : null,
    onEdgeLabel,
  }), [focus, focusId, drawerId, hoverId, q, showLabels, connectSource, picker, onEdgeLabel])

  // ── Derived chrome ────────────────────────────────────────────────
  const drawerNode = drawerId ? records?.find((r) => r.id === drawerId) ?? null : null
  const pickerLink = picker?.mode === 'edit' ? links.find((l) => l.id === picker.edgeId) : null
  const pickerFrom = picker?.mode === 'new' ? picker.from : pickerLink?.from
  const pickerTo = picker?.mode === 'new' ? picker.to : pickerLink?.to
  const presentTypes = TYPE_ORDER.filter((t) => (typeCounts.get(t) ?? 0) > 0)
  const totalCards = records?.length ?? 0

  const hint = composer
    ? 'Type the memory, then ⌘↵ or click away to save. Esc cancels.'
    : tool === 'memory'
      ? 'Click anywhere on the board to place a memory. Esc to cancel.'
      : tool === 'connect'
        ? (connectSource ? 'Now click the memory to link it to.' : 'Click the first memory, then the second.')
        : connecting
          ? 'Drop on another memory to link them.'
          : null

  return (
    <BoardCtx.Provider value={ui}>
      <div
        ref={wrapRef}
        className={cn(
          'rb-root', `tool-${tool}`, `labels-${labelMode}`, tour && 'is-tour',
          connecting && 'is-connecting', animating && 'is-flying', directFlight && 'is-direct',
          shaking && 'is-shaking', blasted && 'is-blasted',
        )}
        onDoubleClick={(e) => {
          if ((e.target as HTMLElement).closest('.react-flow__pane')) placeComposer(e.clientX, e.clientY)
        }}
      >
        {/* Deep space, behind everything. A picture, because no amount of CSS
          draws a convincing galaxy; it ships inside the app, so the board
          still works with the network off. */}
      <div className="rb-sky" aria-hidden="true" style={{ backgroundImage: `url(${galaxy})` }} />

      <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          onNodesChange={onNodesChange}
          onNodeClick={onNodeClick}
          onNodeMouseEnter={(_, n) => { if (n.type === 'memory' && !animRef.current) setHoverId(n.id) }}
          onNodeMouseLeave={() => setHoverId(null)}
          onNodeDragStart={onNodeDragStart}
          onNodeDragStop={onNodeDragStop}
          onPaneClick={onPaneClick}
          onConnectStart={() => { setConnecting(true); setDrawerId(null) }}
          onConnectEnd={onConnectEnd}
          isValidConnection={(c) => c.source !== c.target}
          onMove={onMove}
          onMoveEnd={onMoveEnd}
          connectionMode={ConnectionMode.Loose}
          connectionLineStyle={{ stroke: '#4262ff', strokeWidth: 2, strokeDasharray: '6 5' }}
          colorMode={dark ? 'dark' : 'light'}
          panOnScroll
          zoomOnPinch
          zoomOnDoubleClick={false}
          selectionKeyCode="Shift"
          multiSelectionKeyCode={['Meta', 'Control']}
          deleteKeyCode={null}
          nodeClickDistance={4}
          minZoom={0.05}
          maxZoom={2.5}
          onlyRenderVisibleElements={!animating}
          nodesDraggable={!animating}
          proOptions={{ hideAttribution: true }}
        >
          {/* The sky is drawn by the stylesheet; this stays for React Flow's own sizing. */}
          <Background variant={BackgroundVariant.Dots} gap={24} size={0} className="rb-bg" />
          {showMap && !animating && (
            <MiniMap
              className="rb-minimap"
              pannable
              zoomable
              nodeColor={(n) => TYPE_META[(n.data as MemoryData).type]?.ink ?? '#888'}
              nodeStrokeWidth={0}
              maskColor={dark ? 'rgba(21, 23, 28, 0.72)' : 'rgba(246, 246, 243, 0.72)'}
              bgColor={dark ? '#1f2229' : '#ffffff'}
              nodeBorderRadius={999}
            />
          )}
          {composer && (
            <Composer
              at={composer}
              busy={composerBusy}
              onSave={createMemory}
              onCancel={() => setComposer(null)}
            />
          )}
        </ReactFlow>

        {boom && <Shockwave key={boom.key} x={boom.x} y={boom.y} />}

        {/* Brand block */}
        <div className="rb-brand rb-panel">
          {onExit && (
            <button className="rb-back" onClick={onExit} title="Back to Lane" aria-label="Back to Lane">
              <ChevronLeft size={16} />
            </button>
          )}
          <img src="/white.png" alt="" className="logo" />
          <div className="names">
            <span className="app">Lane</span>
            <span className="board">Constellation</span>
          </div>
        </div>

        {/* Tool rail */}
        <div className="rb-rail rb-panel" role="toolbar" aria-label="Board tools">
          <RailButton tip="Select" keys="V" active={tool === 'select'} onClick={() => { setTool('select'); setConnectSource(null) }}>
            <MousePointer2 size={18} />
          </RailButton>
          <RailButton tip="Add memory" keys="N or double-click" active={tool === 'memory'} onClick={() => { setTool(tool === 'memory' ? 'select' : 'memory'); setConnectSource(null) }}>
            <StickyNote size={18} />
          </RailButton>
          <RailButton tip="Connect two memories" keys="C" active={tool === 'connect'} onClick={() => { setTool(tool === 'connect' ? 'select' : 'connect'); setConnectSource(null) }}>
            <Spline size={18} />
          </RailButton>
          <RailButton
            tip={blasted ? 'Bring them back' : 'Blast'}
            keys="B"
            active={blasted}
            disabled={!totalCards || (animating && !blasted)}
            onClick={blast}
            className="rb-blast-btn"
          >
            {blasted ? <Orbit size={18} /> : <Bomb size={18} />}
          </RailButton>
          <span className="sep" />
          <RailButton tip="Undo" keys="⌘Z" disabled={undoRef.current.length === 0} onClick={() => step('undo')}>
            <Undo2 size={18} />
          </RailButton>
          <RailButton tip="Redo" keys="⌘⇧Z" disabled={redoRef.current.length === 0} onClick={() => step('redo')}>
            <Redo2 size={18} />
          </RailButton>
          <span className="sep" />
          <RailButton tip="Tidy up" keys="Re-form the brain" disabled={!totalCards || animating} onClick={tidy}>
            <Brain size={18} />
          </RailButton>
        </div>

        {tour && (() => {
          const stop = stops[tour.i]
          const meta = stop ? TYPE_META[stop.type] : null
          return (
            <div className="rb-tour">
              <div className="rb-tour-top">
                <span className="rb-tour-count">{tour.i + 1} / {stops.length}</span>
                <div className="rb-tour-bar"><span style={{ width: `${((tour.i + 1) / stops.length) * 100}%` }} /></div>
                <button type="button" onClick={() => setTour((t) => (t ? { ...t, playing: !t.playing } : t))} title={tour.playing ? 'Pause' : 'Play'} aria-label={tour.playing ? 'Pause' : 'Play'}>
                  {tour.playing ? <Pause size={15} /> : <Play size={15} />}
                </button>
                <button type="button" onClick={endTour} title="Leave the walk (Esc)" aria-label="Leave the walk"><X size={15} /></button>
              </div>
              {stop && (
                <div className="rb-tour-card" key={stop.id}>
                  <span className="k" style={{ color: meta?.ink }}>{meta?.label ?? stop.type}</span>
                  <h2>{stop.title}</h2>
                  <p>{new Date(stop.updatedAt).toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' })}</p>
                </div>
              )}
            </div>
          )
        })()}

        {/* Top right */}
        <div className="rb-top-right">
          <div className="rb-search rb-panel">
            <Search size={14} className="ic" />
            <input
              ref={searchRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') { e.preventDefault(); jump(e.shiftKey ? -1 : 1) }
                if (e.key === 'Escape') { setQuery(''); (e.target as HTMLInputElement).blur() }
              }}
              placeholder="Search memories  /"
              aria-label="Search memories on the board"
            />
            {q && (
              <>
                <span className="count">{matches.length ? `${(((matchIdx - 1) % matches.length) + matches.length) % matches.length + 1}/${matches.length}` : '0'}</span>
                <button type="button" onClick={() => jump(-1)} aria-label="Previous match" disabled={!matches.length}><ChevronUp size={14} /></button>
                <button type="button" onClick={() => jump(1)} aria-label="Next match" disabled={!matches.length}><ChevronDown size={14} /></button>
                <button type="button" onClick={() => setQuery('')} aria-label="Clear search"><X size={13} /></button>
              </>
            )}
          </div>
          <div className="rb-stats rb-panel" title={truncated ? `The newest ${BOARD_LIMIT.toLocaleString()} memories are drawn. Older ones are still in Memories and in answers.` : undefined}>
            <b>{totalCards.toLocaleString()}</b> memories · <b>{links.filter((l) => !l.id.startsWith('tmp-')).length.toLocaleString()}</b> links
            {truncated && <span className="rb-trunc">newest {BOARD_LIMIT.toLocaleString()}</span>}
          </div>
        </div>

        {/* Type legend */}
        {presentTypes.length > 0 && (
          <div className="rb-legend rb-panel">
            {presentTypes.map((t) => {
              const off = hiddenTypes.has(t)
              return (
                <button
                  key={t}
                  type="button"
                  className={cn('lg', off && 'is-off')}
                  style={{ ['--ink' as string]: TYPE_META[t].ink }}
                  onClick={() => setHiddenTypes((prev) => { const n = new Set(prev); if (n.has(t)) n.delete(t); else n.add(t); return n })}
                  title={off ? `Show ${TYPE_META[t].plural.toLowerCase()}` : `Hide ${TYPE_META[t].plural.toLowerCase()}`}
                >
                  <span className="sw" />{TYPE_META[t].plural}<span className="n">{typeCounts.get(t)}</span>
                </button>
              )
            })}
            {hiddenTypes.size > 0 && (
              <button type="button" className="lg all" onClick={() => setHiddenTypes(new Set())}>Show all</button>
            )}
          </div>
        )}

        {/* Zoom */}
        <div className="rb-zoom rb-panel" role="group" aria-label="Zoom">
          <button type="button" onClick={() => startTour(false)} title="Walk down memory lane" aria-label="Walk down memory lane"><Play size={15} /></button>
          <button type="button" onClick={() => startTour(true)} title="Walk it and save a video" aria-label="Walk it and save a video"><Video size={15} /></button>
          <span className="sep" />
          <button type="button" className={cn(showMap && 'is-on')} onClick={() => setShowMap((v) => !v)} title="Mini map" aria-label="Toggle mini map">
            <MapIcon size={15} />
          </button>
          <span className="sep" />
          <button type="button" onClick={() => rf.zoomOut({ duration: 180 })} title="Zoom out (-)" aria-label="Zoom out"><Minus size={15} /></button>
          <button type="button" className="pct" onClick={() => rf.zoomTo(1, { duration: 250 })} title="Zoom to 100%">{zoomPct}%</button>
          <button type="button" onClick={() => rf.zoomIn({ duration: 180 })} title="Zoom in (+)" aria-label="Zoom in"><Plus size={15} /></button>
          <button type="button" onClick={() => rf.fitView({ padding: 0.12, maxZoom: 1, duration: 400 })} title="Fit everything (F)" aria-label="Fit to screen"><Maximize size={15} /></button>
        </div>

        {/* One quiet line in the margin */}
        <div className="rb-sky-note" aria-hidden="true">Thoughts<br />connect<br />worlds<i /></div>

        {/* Hint strip */}
        {hint ? (
          <div className="rb-hint">{hint}</div>
        ) : tipOpen && totalCards > 0 ? (
          <div className="rb-hint is-tip">
            <span>Double-click anywhere to add a memory · drag from a dot&apos;s edge onto another to link them · click a dot to read it · press B to blast</span>
            <button type="button" onClick={dismissTip}>Got it</button>
          </div>
        ) : null}

        {/* States */}
        {records === null && !err && (
          <div className="rb-center"><div className="rb-panel rb-state"><Loader2 size={16} className="animate-spin" /> Laying out your board…</div></div>
        )}
        {err && (
          <div className="rb-center">
            <div className="rb-panel rb-state is-error">
              <AlertCircle size={16} /> Could not load the board ({err}).
              <button type="button" onClick={() => { setErr(null); load() }}>Try again</button>
            </div>
          </div>
        )}
        {records !== null && records.length === 0 && !composer && (
          <div className="rb-center">
            <div className="rb-panel rb-empty">
              <StickyNote size={26} />
              <h2>Your board is empty</h2>
              <p>Double-click anywhere, or press <kbd>N</kbd>, to add your first memory. Memories you capture anywhere in Reattend land here too.</p>
              <button type="button" onClick={() => { const r = wrapRef.current?.getBoundingClientRect(); placeComposer((r?.width ?? 800) / 2, (r?.height ?? 600) / 2) }}>
                <Plus size={14} /> Add a memory
              </button>
            </div>
          </div>
        )}

        <MemoryDrawer
          node={drawerNode}
          links={links}
          titleOf={titleOf}
          onClose={() => setDrawerId(null)}
          onGoTo={(id) => goTo(id)}
          onEditLink={onEdgeLabel}
          onAsk={onAsk}
          onOpen={(id) => { const n = Number(id.replace(/^m:/, '')); if (Number.isFinite(n)) onOpenMemory(n) }}
          onConnectFrom={(id) => { setDrawerId(null); setTool('connect'); setConnectSource(id) }}
        />

        {picker && pickerFrom && pickerTo && (
          <RelationPicker
            key={picker.mode === 'new' ? `${picker.from}>${picker.to}` : picker.edgeId}
            x={picker.x}
            y={picker.y}
            fromTitle={titleOf(pickerFrom) || 'Memory'}
            toTitle={titleOf(pickerTo) || 'Memory'}
            current={pickerLink?.kind}
            onPick={pickRelation}
            onClose={() => setPicker(null)}
            onDelete={picker.mode === 'edit' ? removePickedLink : undefined}
            onReverse={picker.mode === 'edit' ? reversePickedLink : undefined}
          />
        )}
      </div>
    </BoardCtx.Provider>
  )
}

function RailButton({
  tip, keys, active, disabled, onClick, children, className,
}: {
  tip: string
  keys?: string
  active?: boolean
  disabled?: boolean
  onClick: () => void
  children: React.ReactNode
  className?: string
}) {
  return (
    <button
      type="button"
      className={cn('rb-tool', active && 'is-active', className)}
      onClick={onClick}
      disabled={disabled}
      aria-label={tip}
      aria-pressed={active}
    >
      {children}
      <span className="tip">{tip}{keys && <em>{keys}</em>}</span>
    </button>
  )
}

// Screen-space burst drawn over the canvas when the board blasts: a flash,
// two expanding rings and sparks in the memory-type colours.
const SPARK_COLORS = Object.values(TYPE_META).map((m) => m.ink)
function Shockwave({ x, y }: { x: number; y: number }) {
  const sparks = useMemo(() => {
    const r = seededRandom(Math.floor(x * 31 + y * 17))
    return Array.from({ length: 34 }, (_, i) => {
      const a = (i / 34) * Math.PI * 2 + (r() - 0.5) * 0.4
      const d = 160 + r() * 340
      return { dx: Math.cos(a) * d, dy: Math.sin(a) * d, s: 4 + r() * 7, delay: r() * 90, c: SPARK_COLORS[i % SPARK_COLORS.length] }
    })
  }, [x, y])
  return (
    <div className="rb-boom" style={{ left: x, top: y }} aria-hidden>
      <span className="flash" />
      <span className="ring" />
      <span className="ring r2" />
      {sparks.map((p, i) => (
        <span
          key={i}
          className="spark"
          style={{
            ['--dx' as string]: `${p.dx}px`, ['--dy' as string]: `${p.dy}px`,
            width: p.s, height: p.s, background: p.c, animationDelay: `${p.delay}ms`,
          }}
        />
      ))}
    </div>
  )
}
