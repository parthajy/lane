// Board model - types, relation kinds, the frame layout and saved positions.
// Pure functions only (plus one tiny theme hook); board-view.tsx owns state
// and board-parts.tsx owns the rendering.

import { useEffect, useState } from 'react'

// True while the app is in dark mode. Reads the `dark` class on <html>,
// which is what every dark style in the app keys off, and follows toggles.
export function useIsDark(): boolean {
  const [dark, setDark] = useState(false)
  useEffect(() => {
    const el = document.documentElement
    const sync = () => setDark(el.classList.contains('dark'))
    sync()
    const mo = new MutationObserver(sync)
    mo.observe(el, { attributes: true, attributeFilter: ['class'] })
    return () => mo.disconnect()
  }, [])
  return dark
}

export type RecordType =
  | 'decision' | 'insight' | 'meeting' | 'idea' | 'context' | 'tasklike' | 'note' | 'transcript'

export interface GraphNode {
  id: string
  title: string
  type: RecordType
  workspaceId: string
  createdAt: string
  updatedAt: string
  tags: string[]
}

export interface GraphEdge {
  id: string
  from: string
  to: string
  kind: string
  weight: number | null
  explanation?: string | null
  manual?: boolean
}

export type XY = { x: number; y: number }

// Sticky-note palette per memory type. `fill` is the card, `ink` the accent
// (type label, left bar, minimap). Dark mode swaps fills in board.css via
// the data-type attribute, so only the light values live here.
// The board is a night sky, so these are the colours of stars rather than of
// ink on paper: each one has to glow on near-black and stay apart from its
// neighbours at the size of a dot.
export const TYPE_META: Record<RecordType, { label: string; plural: string; ink: string; fill: string }> = {
  decision:   { label: 'Decision',   plural: 'Decisions',   ink: '#a855f7', fill: '#ece6ff' },
  meeting:    { label: 'Meeting',    plural: 'Meetings',    ink: '#3b82f6', fill: '#e1e8ff' },
  insight:    { label: 'Insight',    plural: 'Insights',    ink: '#2dd4bf', fill: '#d9f4f0' },
  idea:       { label: 'Idea',       plural: 'Ideas',       ink: '#f5a524', fill: '#fff1b8' },
  tasklike:   { label: 'Task',       plural: 'Tasks',       ink: '#fb7185', fill: '#ffe3d6' },
  note:       { label: 'Note',       plural: 'Notes',       ink: '#a3e635', fill: '#eaf5d6' },
  context:    { label: 'Context',    plural: 'Context',     ink: '#e6ebff', fill: '#eceff3' },
  transcript: { label: 'Transcript', plural: 'Transcripts', ink: '#f472b6', fill: '#fde2ee' },
}

export const TYPE_ORDER: RecordType[] = ['decision', 'meeting', 'insight', 'idea', 'tasklike', 'note', 'transcript', 'context']

// Relation kinds a person can pick, most common first. 'temporal' is
// inferred-only (same time window) so it is labelled but never offered.
export const RELATIONS: Array<{ kind: string; label: string; color: string; hint: string }> = [
  { kind: 'related_to',      label: 'related to',  color: '#7b8494', hint: 'Loosely connected' },
  { kind: 'supports',        label: 'supports',    color: '#2b9a66', hint: 'Backs it up' },
  { kind: 'contradicts',     label: 'contradicts', color: '#e5484d', hint: 'Says the opposite' },
  { kind: 'leads_to',        label: 'leads to',    color: '#3559e0', hint: 'Comes next' },
  { kind: 'causes',          label: 'causes',      color: '#6d4aff', hint: 'Is the reason for' },
  { kind: 'depends_on',      label: 'depends on',  color: '#0b7fb3', hint: 'Needs it first' },
  { kind: 'blocks',          label: 'blocks',      color: '#e8590c', hint: 'Stands in the way' },
  { kind: 'part_of',         label: 'part of',     color: '#0e8a82', hint: 'Belongs inside' },
  { kind: 'continuation_of', label: 'continues',   color: '#5b5bd6', hint: 'Picks up from' },
  { kind: 'same_topic',      label: 'same topic',  color: '#8e8c99', hint: 'About the same thing' },
  { kind: 'same_people',     label: 'same people', color: '#c2255c', hint: 'Same people involved' },
]

const TEMPORAL = { kind: 'temporal', label: 'same time', color: '#a0a4ad', hint: 'Happened around the same time' }

export function relationMeta(kind: string) {
  return RELATIONS.find((r) => r.kind === kind) ?? (kind === 'temporal' ? TEMPORAL : { ...TEMPORAL, kind, label: kind.replace(/_/g, ' ') })
}

// ─── Layout ────────────────────────────────────────────────────────────
// A small force simulation that settles memories into a round, brain-like
// cloud (the Obsidian graph look): linked memories pull together, every
// memory pushes its neighbours away, a gentle pull keeps the whole thing
// circular, and each type drifts into its own wedge so colours form lobes.
// Past ~40 memories a soft centre line splits it into two hemispheres.
// Seeded from each memory's id, so the same board lays out the same way.

export const MIN_DOT = 14

// Dot diameter grows with how connected a memory is.
export function dotSize(degree: number): number {
  return Math.round(MIN_DOT + Math.min(26, Math.sqrt(degree) * 7))
}

export interface BrainLayout {
  centers: Map<string, XY>
  radius: number
  sector: Map<RecordType, number>
  spacing: number // typical distance between neighbouring dots, in flow units
}

function hashSeed(s: string): number {
  let h = 2166136261
  for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619) }
  return h >>> 0
}
export function seededRandom(seed: number | string): () => number {
  let a = typeof seed === 'string' ? hashSeed(seed) : seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

export function brainLayout(nodes: GraphNode[], edges: GraphEdge[]): BrainLayout {
  const n = nodes.length
  const centers = new Map<string, XY>()
  const sector = new Map<RecordType, number>()
  if (n === 0) return { centers, radius: 0, sector, spacing: 100 }

  const typeOf = (g: GraphNode) => (TYPE_META[g.type] ? g.type : 'note') as RecordType
  const index = new Map(nodes.map((g, i) => [g.id, i]))
  const deg = new Float64Array(n)
  const links: Array<[number, number]> = []
  for (const e of edges) {
    const a = index.get(e.from), b = index.get(e.to)
    if (a === undefined || b === undefined || a === b) continue
    links.push([a, b])
    deg[a]++
    deg[b]++
  }
  const rad = nodes.map((_, i) => dotSize(deg[i]) / 2)

  // Each type owns a wedge sized by its share, starting at 12 o'clock.
  const counts = new Map<RecordType, number>()
  for (const g of nodes) counts.set(typeOf(g), (counts.get(typeOf(g)) ?? 0) + 1)
  const span = new Map<RecordType, number>()
  let acc = 0
  for (const t of TYPE_ORDER) {
    const c = counts.get(t)
    if (!c) continue
    const share = (c / n) * Math.PI * 2
    sector.set(t, -Math.PI / 2 + acc + share / 2)
    span.set(t, share)
    acc += share
  }

  const R = Math.max(140, Math.sqrt(n) * 58)
  const x = new Float64Array(n), y = new Float64Array(n)
  const vx = new Float64Array(n), vy = new Float64Array(n)
  const home = new Float64Array(n)
  nodes.forEach((g, i) => {
    const r = seededRandom(g.id)
    const t = typeOf(g)
    const th = (sector.get(t) ?? 0) + (r() - 0.5) * (span.get(t) ?? 1) * 0.85
    const rr = R * Math.sqrt(0.08 + 0.92 * r())
    x[i] = Math.cos(th) * rr
    y[i] = Math.sin(th) * rr
    home[i] = sector.get(t) ?? 0
  })

  const fissure = n >= 40 ? Math.max(34, R * 0.08) : 0
  const iters = n > 250 ? 260 : 320
  const decay = 1 - Math.pow(0.001, 1 / iters)
  const linkDist = 80
  const linkK = 0.22
  const gravity = 0.055
  const angular = 0.075
  const maxRep = 420 * 420
  let alpha = 1

  for (let it = 0; it < iters; it++) {
    // Many-body repulsion (bigger dots push harder).
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        let dx = x[j] - x[i], dy = y[j] - y[i]
        let d2 = dx * dx + dy * dy
        if (d2 > maxRep) continue
        if (d2 < 1) { dx = (i % 7) - 3 + 0.5; dy = (j % 5) - 2 + 0.5; d2 = dx * dx + dy * dy }
        const f = (-(55 + rad[i] + rad[j]) * alpha) / d2
        vx[i] += dx * f; vy[i] += dy * f
        vx[j] -= dx * f; vy[j] -= dy * f
      }
    }
    // Springs along links.
    for (const [a, b] of links) {
      const dx = x[b] - x[a], dy = y[b] - y[a]
      const d = Math.sqrt(dx * dx + dy * dy) || 1
      const f = ((d - linkDist) / d) * linkK * alpha
      const ba = deg[a] / (deg[a] + deg[b])
      vx[b] -= dx * f * (1 - ba); vy[b] -= dy * f * (1 - ba)
      vx[a] += dx * f * ba; vy[a] += dy * f * ba
    }
    for (let i = 0; i < n; i++) {
      // Round overall shape.
      vx[i] -= x[i] * gravity * alpha
      vy[i] -= y[i] * gravity * alpha
      // Drift toward the type's wedge (tangential only).
      const r = Math.sqrt(x[i] * x[i] + y[i] * y[i]) || 1
      const th = Math.atan2(y[i], x[i])
      let dth = home[i] - th
      while (dth > Math.PI) dth -= Math.PI * 2
      while (dth < -Math.PI) dth += Math.PI * 2
      vx[i] += (-y[i] / r) * dth * r * angular * alpha
      vy[i] += (x[i] / r) * dth * r * angular * alpha
      // Hemispheres: keep a soft gap along the vertical centre line.
      if (fissure) {
        const side = Math.cos(home[i]) >= 0 ? 1 : -1
        const want = side * fissure
        if (side * x[i] < fissure) vx[i] += (want - x[i]) * 0.2 * alpha
      }
    }
    for (let i = 0; i < n; i++) {
      vx[i] *= 0.6; vy[i] *= 0.6
      x[i] += vx[i]; y[i] += vy[i]
      // In the back half of the run the centre gap becomes firm, so the
      // hemispheres read clearly (links still cross it, like a bridge).
      if (fissure && it > iters * 0.35) {
        const side = Math.cos(home[i]) >= 0 ? 1 : -1
        if (side * x[i] < fissure) x[i] += (side * fissure - x[i]) * 0.35
      }
    }
    alpha -= alpha * decay
  }

  // Collision pass: no two dots (plus room for a label) overlap. Small
  // boards get more air so every title can show at a normal zoom.
  const pad = n <= 30 ? 120 : n <= 80 ? 50 : 24
  for (let pass = 0; pass < 6; pass++) {
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        const min = rad[i] + rad[j] + pad
        const dx = x[j] - x[i], dy = y[j] - y[i]
        const d2 = dx * dx + dy * dy
        if (d2 >= min * min) continue
        const d = Math.sqrt(d2) || 0.01
        const push = ((min - d) / d) * 0.5
        x[i] -= dx * push; y[i] -= dy * push
        x[j] += dx * push; y[j] += dy * push
      }
    }
  }

  let maxR = 0
  nodes.forEach((g, i) => {
    // A touch taller than wide, like a brain seen from above.
    centers.set(g.id, { x: x[i], y: y[i] * 1.08 })
    maxR = Math.max(maxR, Math.sqrt(x[i] * x[i] + y[i] * y[i]))
  })
  return { centers, radius: maxR, sector, spacing: Math.sqrt((Math.PI * maxR * maxR * 1.08) / n) }
}

// Where a memory that arrived after the layout was made should appear:
// just outside the cloud, in its own type's wedge.
export function arrivalPoint(layout: BrainLayout, type: RecordType, id: string): XY {
  const r = seededRandom(id)
  const th = (layout.sector.get(type) ?? -Math.PI / 2) + (r() - 0.5) * 0.5
  const rr = (layout.radius || 200) + 60 + r() * 50
  return { x: Math.cos(th) * rr, y: Math.sin(th) * rr * 1.08 }
}

// ─── Saved positions ───────────────────────────────────────────────────
// Where a person drags a card is remembered per browser and per scope
// (each org, and Personal, get their own board). Kept client-side on
// purpose: the layout is a personal view, not org data.

// Values are dot centres (v3; v2 held top-left corners of the old cards).
const POS_KEY = (scope: string) => `reattend:board:v3:pos:${scope}`

export function loadPositions(scope: string): Map<string, XY> {
  try {
    const raw = window.localStorage.getItem(POS_KEY(scope))
    if (!raw) return new Map()
    const obj = JSON.parse(raw) as Record<string, [number, number]>
    return new Map(Object.entries(obj).map(([id, [x, y]]) => [id, { x, y }]))
  } catch {
    return new Map()
  }
}

export function savePositions(scope: string, positions: Map<string, XY>) {
  try {
    const obj: Record<string, [number, number]> = {}
    positions.forEach((p, id) => { obj[id] = [Math.round(p.x), Math.round(p.y)] })
    window.localStorage.setItem(POS_KEY(scope), JSON.stringify(obj))
  } catch { /* storage full or blocked - positions just won't stick */ }
}

export function clearPositions(scope: string) {
  try { window.localStorage.removeItem(POS_KEY(scope)) } catch { /* ignore */ }
}

// Point on a dot's rim facing `to`, so a connector meets the dot's edge
// rather than its centre.
export function rimPoint(c: XY, r: number, to: XY): XY {
  const dx = to.x - c.x, dy = to.y - c.y
  const d = Math.sqrt(dx * dx + dy * dy)
  if (d < 0.001) return c
  return { x: c.x + (dx / d) * r, y: c.y + (dy / d) * r }
}

export function timeAgo(iso: string): string {
  const t = new Date(iso).getTime()
  if (!Number.isFinite(t)) return ''
  const s = Math.max(0, (Date.now() - t) / 1000)
  if (s < 60) return 'just now'
  if (s < 3600) return `${Math.floor(s / 60)}m ago`
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`
  if (s < 86400 * 30) return `${Math.floor(s / 86400)}d ago`
  return new Date(iso).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' })
}
