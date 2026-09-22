// Board parts - the pieces board-view.tsx composes: the sticky-note memory
// card, type frames, the floating connector with its relation label, the
// relation picker, the memory drawer and the on-canvas composer.

import { createContext, memo, useContext, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { api } from '@/lib/api'
import ReactMarkdown from 'react-markdown'
import {
  Handle, Position, BaseEdge, EdgeLabelRenderer, ViewportPortal, getStraightPath, useInternalNode,
  type Node, type NodeProps, type Edge, type EdgeProps,
} from '@xyflow/react'
import {
  X, ArrowRight, ArrowLeft, ExternalLink, Link2, MessageSquare, Trash2, ArrowLeftRight,
  Loader2, Paperclip, Users, Tag,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import {
  TYPE_META, RELATIONS, relationMeta, rimPoint, timeAgo, MIN_DOT,
  type RecordType, type GraphNode, type GraphEdge, type XY,
} from './board-model'

// ─── Shared UI state ───────────────────────────────────────────────────
// Read by every card and connector. Kept to values that change on a click
// or hover, never per animation frame (zoom goes through CSS variables).

export interface BoardUi {
  focus: Set<string> | null        // focused card + its neighbours
  focusId: string | null
  hoverId: string | null
  query: string
  showLabels: boolean
  connectSource: string | null
  drawerId: string | null
  selectedEdgeId: string | null
  onEdgeLabel: (edgeId: string, clientX: number, clientY: number) => void
}

export const BoardCtx = createContext<BoardUi>({
  focus: null, focusId: null, hoverId: null, query: '', showLabels: true, connectSource: null, drawerId: null,
  selectedEdgeId: null, onEdgeLabel: () => {},
})

// ─── Memory dot ────────────────────────────────────────────────────────
// Obsidian-style: a dot coloured by type and sized by how connected it is,
// title underneath. The node box is just the dot (the label hangs outside
// it), so connectors and hit-testing work on the circle.

export type MemoryData = {
  title: string
  type: RecordType
  updatedAt: string
  degree: number
  size: number
  hub?: boolean
  fresh?: boolean
}

// Compared without position props: a dot only re-renders when its data,
// selection or the shared UI state change, not on every frame of a flight.
const sameDot = (a: NodeProps<Node<MemoryData>>, b: NodeProps<Node<MemoryData>>) =>
  a.id === b.id && a.data === b.data && a.selected === b.selected && a.dragging === b.dragging

export const MemoryDot = memo(function MemoryDot({ id, data, selected }: NodeProps<Node<MemoryData>>) {
  const ui = useContext(BoardCtx)
  const meta = TYPE_META[data.type] ?? TYPE_META.note
  const matches = !ui.query || data.title.toLowerCase().includes(ui.query)
  const faded = !matches || (ui.focus !== null && !ui.focus.has(id))
  const lit = ui.focusId === id || ui.hoverId === id
  const title = data.title || 'Untitled memory'
  return (
    <div
      className={cn(
        'rb-dot',
        selected && 'is-selected',
        faded && 'is-faded',
        lit && 'is-lit',
        ui.query && matches && 'is-match',
        ui.connectSource === id && 'is-source',
        ui.drawerId === id && 'is-open',
        data.fresh && 'is-fresh',
        data.hub && 'is-hub',
      )}
      data-type={data.type}
      data-links={data.degree}
      style={{ ['--ink' as string]: meta.ink, width: data.size, height: data.size }}
      title={title}
    >
      <Handle id="r" type="source" position={Position.Right} className="rb-handle" />
      <span className="rb-dot-core" />
      <span className="rb-dot-label">
        {title.length > 64 ? `${title.slice(0, 62)}…` : title}
        {data.fresh && <em> · filing it…</em>}
      </span>
    </div>
  )
}, sameDot)

// ─── Connector ─────────────────────────────────────────────────────────
// Floating: it meets each card on the border facing the other card, so it
// stays tidy whichever way the cards are dragged. The relation label sits
// on the midpoint and is a button - click it to change or remove the link.

export type LinkData = { kind: string; manual?: boolean; pending?: boolean }

export function Connector({ id, source, target, data, markerEnd }: EdgeProps<Edge<LinkData>>) {
  const ui = useContext(BoardCtx)
  const s = useInternalNode(source)
  const t = useInternalNode(target)
  if (!s || !t || !data) return null

  const circle = (n: NonNullable<typeof s>) => {
    const d = n.measured.width ?? MIN_DOT
    return { c: { x: n.internals.positionAbsolute.x + d / 2, y: n.internals.positionAbsolute.y + d / 2 }, r: d / 2 + 3 }
  }
  const sa = circle(s), ta = circle(t)
  const p1 = rimPoint(sa.c, sa.r, ta.c)
  const p2 = rimPoint(ta.c, ta.r, sa.c)
  const [path, lx, ly] = getStraightPath({ sourceX: p1.x, sourceY: p1.y, targetX: p2.x, targetY: p2.y })

  const rel = relationMeta(data.kind)
  const touches = (x: string | null) => x !== null && (source === x || target === x)
  const touchesFocus = touches(ui.focusId)
  const active = ui.selectedEdgeId === id || touchesFocus || touches(ui.hoverId) || !!data.pending
  const faded = ui.focus !== null && !touchesFocus && !data.pending
  const showLabel = active || (ui.showLabels && !faded)

  return (
    <>
      <BaseEdge
        id={id}
        path={path}
        markerEnd={markerEnd}
        interactionWidth={18}
        style={{
          // On the night sky a link is a thread of light. It only takes the
          // relation's colour once you touch it; at rest the web should read
          // as one thing rather than as a scatter of coloured lines.
          stroke: active ? rel.color : 'rgba(188, 205, 255, 0.72)',
          strokeWidth: active ? 2 : 0.9,
          strokeDasharray: data.pending ? '7 6' : undefined,
          opacity: faded ? 0.07 : active ? 1 : 0.62,
          transition: 'opacity 160ms ease, stroke-width 160ms ease',
        }}
      />
      {showLabel && (
        <EdgeLabelRenderer>
          <div
            className="rb-edge-anchor"
            style={{ transform: `translate(-50%, -50%) translate(${lx}px, ${ly}px)` }}
          >
            <button
              type="button"
              className={cn('rb-edge-label nodrag nopan', active && 'is-active', data.pending && 'is-pending')}
              style={{ ['--c' as string]: rel.color }}
              onClick={(e) => { e.stopPropagation(); if (!data.pending) ui.onEdgeLabel(id, e.clientX, e.clientY) }}
              title={data.pending ? 'Pick a relation' : 'Change or remove this link'}
            >
              {data.pending ? 'pick a relation' : rel.label}
            </button>
          </div>
        </EdgeLabelRenderer>
      )}
    </>
  )
}

// ─── Relation picker ───────────────────────────────────────────────────

export function RelationPicker({
  x, y, fromTitle, toTitle, current, onPick, onClose, onDelete, onReverse,
}: {
  x: number
  y: number
  fromTitle: string
  toTitle: string
  current?: string
  onPick: (kind: string) => void
  onClose: () => void
  onDelete?: () => void
  onReverse?: () => void
}) {
  const ref = useRef<HTMLDivElement>(null)
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null)

  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    const r = el.getBoundingClientRect()
    const left = Math.min(Math.max(12, x - r.width / 2), window.innerWidth - r.width - 12)
    const below = y + 18
    const top = below + r.height > window.innerHeight - 12 ? Math.max(12, y - r.height - 18) : below
    setPos({ left, top })
  }, [x, y])

  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      if (ref.current && !ref.current.contains(e.target as globalThis.Node)) onClose()
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { e.stopPropagation(); onClose(); return }
      if ((e.key === 'Delete' || e.key === 'Backspace') && onDelete) { e.preventDefault(); onDelete(); return }
      const n = Number(e.key)
      if (Number.isInteger(n) && n >= 1 && n <= 9 && !e.metaKey && !e.ctrlKey) {
        e.preventDefault()
        onPick(RELATIONS[n - 1].kind)
      }
    }
    // Registered after the click that opened the picker has finished, so
    // that click never counts as "outside".
    const t = window.setTimeout(() => {
      document.addEventListener('pointerdown', onDown, true)
    }, 0)
    window.addEventListener('keydown', onKey, true)
    return () => {
      window.clearTimeout(t)
      document.removeEventListener('pointerdown', onDown, true)
      window.removeEventListener('keydown', onKey, true)
    }
  }, [onClose, onPick, onDelete])

  return (
    <div
      ref={ref}
      className="rb-picker"
      role="dialog"
      aria-label="Choose how these memories are related"
      style={{ left: pos?.left ?? x, top: pos?.top ?? y, visibility: pos ? 'visible' : 'hidden' }}
    >
      <div className="rb-picker-pair">
        <span className="t">{fromTitle}</span>
        <ArrowRight size={12} className="shrink-0" />
        <span className="t">{toTitle}</span>
      </div>
      <div className="rb-picker-q">{current ? 'Change the relation' : 'How are these related?'}</div>
      <div className="rb-picker-grid">
        {RELATIONS.map((r, i) => (
          <button
            key={r.kind}
            type="button"
            className={cn('rb-rel', current === r.kind && 'is-current')}
            style={{ ['--c' as string]: r.color }}
            onClick={() => onPick(r.kind)}
            title={r.hint}
          >
            <span className="sw" />
            <span className="l">{r.label}</span>
            {i < 9 && <kbd>{i + 1}</kbd>}
          </button>
        ))}
      </div>
      {(onDelete || onReverse) && (
        <div className="rb-picker-foot">
          {onReverse && (
            <button type="button" onClick={onReverse}>
              <ArrowLeftRight size={12} /> Reverse direction
            </button>
          )}
          {onDelete && (
            <button type="button" className="danger" onClick={onDelete}>
              <Trash2 size={12} /> Remove link
            </button>
          )}
        </div>
      )}
    </div>
  )
}

// ─── Memory drawer ─────────────────────────────────────────────────────

interface RecordDetail {
  id: string
  title: string
  summary: string | null
  content: string | null
  type: RecordType
  createdAt: string
  updatedAt: string
  source?: string | null
  tags: string[]
  entities: Array<{ kind: string; name: string }>
  project: { id: string; name: string } | null
  attachment: { fileName: string } | null
}

/** A Lane memory in the shape the drawer was written for. */
async function loadDetail(nodeId: string): Promise<RecordDetail> {
  const id = Number(nodeId.replace(/^m:/, ''))
  if (!Number.isFinite(id)) throw new Error('not a memory')
  const { card, text } = await api.memoryDetail(id)
  const facts = card.facts.map((f) => `${f.subject} · ${f.attribute}: ${f.value}`)
  return {
    id: nodeId,
    title: card.title,
    summary: card.summary,
    content: [facts.length ? `Facts\n${facts.map((f) => `- ${f}`).join('\n')}` : '', card.decisions.length ? `Decisions\n${card.decisions.map((d) => `- ${d}`).join('\n')}` : '', text ? `What was on screen\n\n${text}` : ''].filter(Boolean).join('\n\n'),
    type: 'note',
    createdAt: new Date(card.startedAt).toISOString(),
    updatedAt: new Date(card.createdAt).toISOString(),
    source: card.appName,
    tags: card.projects,
    entities: [...card.people.map((name) => ({ kind: 'person', name })), ...card.organizations.map((name) => ({ kind: 'org', name }))],
    project: null,
    attachment: null,
  }
}

export function MemoryDrawer({
  node, links, titleOf, onClose, onGoTo, onEditLink, onConnectFrom, onAsk, onOpen,
}: {
  node: GraphNode | null
  links: GraphEdge[]
  titleOf: (id: string) => string | null
  onClose: () => void
  onGoTo: (id: string) => void
  onEditLink: (edgeId: string, clientX: number, clientY: number) => void
  onConnectFrom: (id: string) => void
  onAsk: (question: string) => void
  onOpen: (id: string) => void
}) {
  const ref = useRef<HTMLElement>(null)
  // Keep the last memory rendered while the drawer slides out.
  const [shown, setShown] = useState<GraphNode | null>(node)
  useEffect(() => { if (node) setShown(node) }, [node])
  const open = node !== null

  const [detail, setDetail] = useState<RecordDetail | null>(null)
  const [loadErr, setLoadErr] = useState(false)
  useEffect(() => {
    if (!node) return
    let cancelled = false
    setDetail(null)
    setLoadErr(false)
    loadDetail(node.id)
      .then((d) => { if (!cancelled) setDetail(d) })
      .catch(() => { if (!cancelled) setLoadErr(true) })
    return () => { cancelled = true }
  }, [node?.id]) // eslint-disable-line react-hooks/exhaustive-deps

  // Close on any press outside the drawer. Pressing another memory card is
  // the exception: that swaps the drawer to the card instead.
  useEffect(() => {
    if (!open) return
    const onDown = (e: PointerEvent) => {
      const el = e.target as HTMLElement
      if (ref.current?.contains(el)) return
      if (el.closest?.('.react-flow__node-memory, .rb-picker, [data-sonner-toaster]')) return
      onClose()
    }
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose() }
    const t = window.setTimeout(() => document.addEventListener('pointerdown', onDown, true), 0)
    window.addEventListener('keydown', onKey)
    return () => {
      window.clearTimeout(t)
      document.removeEventListener('pointerdown', onDown, true)
      window.removeEventListener('keydown', onKey)
    }
  }, [open, onClose])

  const n = shown
  const meta = n ? (TYPE_META[n.type] ?? TYPE_META.note) : TYPE_META.note
  const mine = n ? links.filter((l) => l.from === n.id || l.to === n.id) : []
  const d = detail && n && detail.id === n.id ? detail : null
  const body = d?.content?.trim() || ''
  const summary = d?.summary?.trim() || ''
  const showSummary = summary && !body.startsWith(summary.slice(0, 80))
  const people = d?.entities?.filter((e) => e.kind === 'person') ?? []
  const topics = d?.entities?.filter((e) => e.kind !== 'person') ?? []

  return (
    <aside
      ref={ref}
      className={cn('rb-drawer', open && 'is-open')}
      aria-hidden={!open}
      style={{ ['--ink' as string]: meta.ink, ['--fill' as string]: meta.fill }}
    >
      {n && (
        <>
          <header className="rb-drawer-head">
            <div className="row">
              <span className="eyebrow">Memory</span>
              <button type="button" className="x" onClick={onClose} aria-label="Close">
                <X size={16} />
              </button>
            </div>
            <span className="type-chip" data-type={n.type}><span className="dot" />{meta.label}</span>
            <h2>{d?.title || n.title || 'Untitled memory'}</h2>
            <div className="meta">
              Saved {timeAgo(n.createdAt)}
              {n.updatedAt !== n.createdAt && <> · updated {timeAgo(n.updatedAt)}</>}
              {d?.project?.name && <> · {d.project.name}</>}
            </div>
          </header>

          <div className="rb-drawer-body">
            {!d && !loadErr && (
              <div className="rb-drawer-loading"><Loader2 size={14} className="animate-spin" /> Loading the memory…</div>
            )}
            {loadErr && (
              <div className="rb-drawer-note">Could not load the full text. The card details are below.</div>
            )}

            {showSummary && (
              <section>
                <h3>Summary</h3>
                <p className="summary">{summary}</p>
              </section>
            )}
            {body && (
              <section>
                <h3>What was saved</h3>
                <div className="content md">
                  <ReactMarkdown
                    components={{
                      a: ({ href, children }) => <a href={href} target="_blank" rel="noopener noreferrer">{children}</a>,
                    }}
                  >
                    {body}
                  </ReactMarkdown>
                </div>
              </section>
            )}
            {d?.attachment && (
              <section className="inline-row">
                <Paperclip size={13} /> {d.attachment.fileName}
              </section>
            )}
            {people.length > 0 && (
              <section>
                <h3><Users size={12} /> People</h3>
                <div className="chips">{people.slice(0, 12).map((p) => <span key={p.name} className="chip">{p.name}</span>)}</div>
              </section>
            )}
            {(topics.length > 0 || (n.tags?.length ?? 0) > 0) && (
              <section>
                <h3><Tag size={12} /> Topics</h3>
                <div className="chips">
                  {(n.tags ?? []).slice(0, 10).map((t) => <span key={`t-${t}`} className="chip">#{t}</span>)}
                  {topics.slice(0, 10).map((t) => <span key={`e-${t.name}`} className="chip soft">{t.name}</span>)}
                </div>
              </section>
            )}

            <section>
              <h3><Link2 size={12} /> Connections · {mine.length}</h3>
              {mine.length === 0 ? (
                <p className="empty">
                  Not linked yet. Drag from the dot on any edge of the card to another memory, or use Connect below.
                </p>
              ) : (
                <ul className="links">
                  {mine.map((l) => {
                    const outgoing = l.from === n.id
                    const other = outgoing ? l.to : l.from
                    const rel = relationMeta(l.kind)
                    return (
                      <li key={l.id}>
                        <button
                          type="button"
                          className="rel"
                          style={{ ['--c' as string]: rel.color }}
                          onClick={(e) => onEditLink(l.id, e.clientX, e.clientY)}
                          title="Change or remove this link"
                        >
                          {outgoing ? <ArrowRight size={11} /> : <ArrowLeft size={11} />}
                          {outgoing ? rel.label : `${rel.label} this`}
                        </button>
                        <button type="button" className="other" onClick={() => onGoTo(other)}>
                          {titleOf(other) ?? 'A memory not on this board'}
                        </button>
                        {!l.manual && <span className="auto" title="Found by Lane: these memories share people or a topic">auto</span>}
                      </li>
                    )
                  })}
                </ul>
              )}
            </section>
          </div>

          <footer className="rb-drawer-foot">
            <button type="button" className="ghost" onClick={() => onConnectFrom(n.id)}>
              <Link2 size={13} /> Connect
            </button>
            <button type="button" className="ghost" onClick={() => onAsk(`What do I know about "${(d?.title || n.title).slice(0, 80)}"?`)}>
              <MessageSquare size={13} /> Ask about it
            </button>
            <button type="button" className="primary" onClick={() => onOpen(n.id)}>
              Open <ExternalLink size={12} />
            </button>
          </footer>
        </>
      )}
    </aside>
  )
}

// ─── Composer ──────────────────────────────────────────────────────────
// A blank sticky note dropped straight onto the canvas. It lives in the
// flow's own coordinate space, so it pans and zooms with the board.

export function Composer({
  at, busy, onSave, onCancel,
}: {
  at: XY
  busy: boolean
  onSave: (text: string) => void
  onCancel: () => void
}) {
  const [text, setText] = useState('')
  const ref = useRef<HTMLTextAreaElement>(null)
  const boxRef = useRef<HTMLDivElement>(null)
  useEffect(() => { const t = setTimeout(() => ref.current?.focus(), 60); return () => clearTimeout(t) }, [])
  const save = () => { if (text.trim() && !busy) onSave(text.trim()) }

  // Click away saves what was typed (or drops an empty note), like a
  // sticky on a whiteboard. Refs keep the listener on the latest text.
  const latest = useRef({ text, busy, onSave, onCancel })
  latest.current = { text, busy, onSave, onCancel }
  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      if (boxRef.current?.contains(e.target as globalThis.Node)) return
      const { text: t, busy: b, onSave: s, onCancel: c } = latest.current
      if (b) return
      if (t.trim()) s(t.trim())
      else c()
    }
    const id = window.setTimeout(() => document.addEventListener('pointerdown', onDown, true), 0)
    return () => { window.clearTimeout(id); document.removeEventListener('pointerdown', onDown, true) }
  }, [])
  return (
    <ViewportPortal>
      <div
        ref={boxRef}
        className="rb-composer nodrag nopan nowheel"
        style={{ transform: `translate(${at.x}px, ${at.y}px)` }}
        onPointerDown={(e) => e.stopPropagation()}
      >
        <div className="rb-composer-type"><span className="dot" /> New memory</div>
        <textarea
          ref={ref}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); save() }
            if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); onCancel() }
          }}
          placeholder="What should Lane remember? A decision, a note, an idea…"
          maxLength={20000}
          disabled={busy}
        />
        <div className="rb-composer-foot">
          <span className="hint">⌘↵ to save · Esc to cancel</span>
          <button type="button" className="cancel" onClick={onCancel} disabled={busy}>Cancel</button>
          <button type="button" className="save" onClick={save} disabled={busy || !text.trim()}>
            {busy ? <Loader2 size={12} className="animate-spin" /> : 'Save'}
          </button>
        </div>
      </div>
    </ViewportPortal>
  )
}
