import { useCallback, useEffect, useState } from 'react'
import { format, isToday, isYesterday } from 'date-fns'
import { AlertTriangle, Brain, CheckSquare, ChevronDown, Clock, Layers, Network, Pencil, Pin, Search as SearchIcon, SlidersHorizontal, ThumbsDown, ThumbsUp, X } from 'lucide-react'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { EmptyState } from '@/components/ui/empty-state'
import { ActivityDetailView } from '@/components/activity-sheet'
import { TimelinePage } from '@/pages/timeline'
import { TasksPage } from '@/pages/tasks'
import { SearchPage } from '@/pages/search'
import { EditMemoryDialog } from '@/components/edit-memory'
import { SourceIcon } from '@/components/source-icon'
import { api, formatDuration, hostOf, type EngineReport, type Fact, type MemoryCard } from '@/lib/api'
import { cn } from '@/lib/utils'

function dayLabel(ts: number) {
  if (isToday(ts)) return 'Today'
  if (isYesterday(ts)) return 'Yesterday'
  return format(ts, 'EEEE, d MMMM')
}

function Chips({ items, className }: { items: string[]; className?: string }) {
  if (!items.length) return null
  return (
    <>
      {items.slice(0, 8).map((x, i) => (
        <span key={i} className={cn('inline-block rounded-md border bg-card px-1.5 py-0.5 text-[11px] leading-4', className)}>
          {x}
        </span>
      ))}
    </>
  )
}

export function Card({ m, onOpen, onFeedback, onEdit, onPin, selected }: {
  m: MemoryCard
  onOpen: (id: number) => void
  onFeedback: (ids: number[], f: 'keep' | 'ignore' | null) => void
  onEdit?: (m: MemoryCard) => void
  onPin?: (ids: number[], pinned: boolean) => void
  selected?: boolean
}) {
  const [open, setOpen] = useState(false)
  const host = hostOf(m.url)
  const overruled = m.feedback != null
  const facts = m.facts ?? []
  const tags = [...m.people, ...m.organizations, ...m.projects]
  const shown = open ? tags : tags.slice(0, 4)
  const note = !m.keep && !overruled ? 'judged not worth keeping'
    : m.feedback === 'keep' ? 'you kept this'
    : m.feedback === 'ignore' ? 'you ignored this'
    : m.pinned ? 'pinned'
    : m.editedAt != null ? 'in your words' : null

  return (
    <div className={cn('rounded-2xl border bg-card p-4 transition-shadow', !m.keep && !overruled && 'opacity-70', selected ? 'ring-2 ring-primary/45 border-primary/40' : 'hover:shadow-sm')}>
      <div className="flex items-center gap-2 mb-2.5 min-w-0">
        <span className="rounded-md bg-secondary px-2 py-1 text-[11px] font-medium capitalize shrink-0">{m.kind}</span>
        <SourceIcon app={m.appName} url={m.url} size={16} />
        <span className="text-[12.5px] truncate" title={`${m.appName}${host ? ` · ${host}` : ''}`}>{host ? host.replace(/^www\./, '') : m.appName}</span>
        <span className="text-[12px] text-muted-foreground truncate hidden sm:inline">
          · {format(m.startedAt, 'HH:mm')} · {formatDuration(m.totalMs)}{m.sessions > 1 && ` over ${m.sessions} sessions`}
        </span>
        {note && <span className={cn('text-[12px] truncate hidden md:inline', note === 'judged not worth keeping' || note === 'you ignored this' ? 'text-destructive' : 'text-primary')}>· {note}</span>}
        <span className="ml-auto flex items-center gap-1.5 shrink-0">
          <span className="rounded-md bg-secondary px-2 py-1 text-[11px] tabular-nums text-muted-foreground" title="How sure Rabbit was about keeping this, and how many extracted strings it dropped for not appearing in the source">
            {Math.round(m.confidence * 100)}%{m.dropped > 0 && ` · ${m.dropped}`}
          </span>
          <button onClick={(e) => { e.stopPropagation(); setOpen((v) => !v) }} className="p-1 rounded-md text-muted-foreground hover:bg-secondary" aria-label={open ? 'Less' : 'More'} title={open ? 'Less' : 'More'}>
            <ChevronDown className={cn('h-4 w-4 transition-transform', open && 'rotate-180')} />
          </button>
        </span>
      </div>

      <button onClick={() => onOpen(m.activityId)} className="text-left text-[15.5px] font-semibold leading-snug hover:underline">
        {m.title}
      </button>
      <p className={cn('text-[13.5px] text-muted-foreground mt-1.5 leading-relaxed', !open && 'line-clamp-3')}>{m.summary}</p>

      {(shown.length > 0 || m.decisions.length > 0) && (
        <div className="flex flex-wrap gap-1.5 mt-3">
          {shown.map((x, i) => (
            <span key={`${x}-${i}`} className="rounded-lg bg-secondary px-2.5 py-1 text-[11.5px]">{x}</span>
          ))}
          {!open && tags.length > shown.length && (
            <button onClick={() => setOpen(true)} className="rounded-lg bg-secondary px-2.5 py-1 text-[11.5px] text-muted-foreground hover:text-foreground">+{tags.length - shown.length}</button>
          )}
          {open && m.decisions.length > 0 && (
            <span className="inline-flex items-center gap-1 rounded-lg bg-primary/10 px-2.5 py-1 text-[11.5px] text-primary" title={m.decisions.join('\n')}>
              ✓ {m.decisions[0]}{m.decisions.length > 1 && ` +${m.decisions.length - 1}`}
            </span>
          )}
        </div>
      )}

      {open && (m.dates.length > 0 || m.numbers.length > 0) && (
        <div className="flex flex-wrap gap-1.5 mt-1.5 text-muted-foreground">
          {[...m.dates, ...m.numbers].map((x, i) => <span key={`${x}-${i}`} className="rounded-lg border px-2.5 py-1 text-[11.5px] tabular-nums">{x}</span>)}
        </div>
      )}

      {open && facts.length > 0 && (
        <div className="flex flex-wrap gap-1.5 mt-1.5">
          {facts.slice(0, 8).map((f) => (
            <span
              key={f.id}
              title={f.conflicts.length > 0 ? `Elsewhere: ${f.conflicts.map((c) => `${c.value} (${c.title})`).join('; ')}` : f.origin === 'user' ? 'Confirmed by you' : 'Stated in the source'}
              className={cn('inline-flex items-center gap-1 rounded-lg border px-2.5 py-1 text-[11.5px]', f.conflicts.length > 0 ? 'border-amber-500/50 text-amber-700 dark:text-amber-400' : 'border-border text-muted-foreground', f.origin === 'user' && 'border-primary/40 text-foreground')}
            >
              {f.conflicts.length > 0 && <AlertTriangle className="h-3 w-3" />}
              {f.subject} · {f.attribute}: <span className="font-medium tabular-nums">{f.value}</span>
            </span>
          ))}
        </div>
      )}

      {open && (
        <div className="flex items-center gap-1 mt-3 pt-3 border-t text-muted-foreground">
          <button title="Useful memory" onClick={(e) => { e.stopPropagation(); onFeedback(m.ids, m.feedback === 'keep' ? null : 'keep') }} className={cn('rounded-lg p-1.5 hover:bg-secondary', m.feedback === 'keep' && 'bg-primary/10 text-primary')}>
            <ThumbsUp className="h-3.5 w-3.5" />
          </button>
          <button title="Not worth keeping" onClick={(e) => { e.stopPropagation(); onFeedback(m.ids, m.feedback === 'ignore' ? null : 'ignore') }} className={cn('rounded-lg p-1.5 hover:bg-secondary', m.feedback === 'ignore' && 'bg-destructive/10 text-destructive')}>
            <ThumbsDown className="h-3.5 w-3.5" />
          </button>
          {onPin && (
            <button title={m.pinned ? 'Unpin' : 'Pin: never remade, ranked first'} onClick={(e) => { e.stopPropagation(); onPin(m.ids, !m.pinned) }} className={cn('rounded-lg p-1.5 hover:bg-secondary', m.pinned && 'bg-primary/10 text-primary')}>
              <Pin className="h-3.5 w-3.5" />
            </button>
          )}
          {onEdit && (
            <button title="Edit in your own words" onClick={(e) => { e.stopPropagation(); onEdit(m) }} className="rounded-lg p-1.5 hover:bg-secondary">
              <Pencil className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
      )}
    </div>
  )
}

export function EngineLine({ engine }: { engine: EngineReport | null }) {
  if (!engine) return null
  const c = engine.counts
  let text = engine.detail
  if (engine.downloadPercent != null) text = `downloading model ${engine.downloadPercent}%`
  else if (engine.available && !engine.busy) text = c.pending > 0 ? `${c.pending} waiting` : 'up to date'
  return (
    <div className="flex items-center gap-2 text-xs text-muted-foreground min-w-0">
      <Brain className={cn('h-3 w-3 shrink-0', engine.busy && 'animate-pulse-soft text-primary', !engine.available && 'text-destructive')} />
      <span className="truncate">Memory: {text}</span>
    </div>
  )
}

export type MemoriesView = 'memories' | 'timeline' | 'tasks' | 'everything'

const VIEW_ICON: Record<MemoriesView, typeof Brain> = {
  memories: Brain,
  timeline: Clock,
  tasks: CheckSquare,
  everything: Layers,
}

const VIEWS: { id: MemoriesView; label: string; hint: string }[] = [
  { id: 'memories', label: 'Memories', hint: 'What Rabbit made of each activity' },
  { id: 'timeline', label: 'Timeline', hint: 'Every activity, in order' },
  { id: 'tasks', label: 'Tasks', hint: 'Commitments found in your activity' },
  { id: 'everything', label: 'Everything', hint: 'Search the raw text of what you saw' },
]

export function MemoriesPage({ view, onViewChange, initialQuery, onQueryConsumed }: { view: MemoriesView; onViewChange: (v: MemoriesView) => void; initialQuery?: string; onQueryConsumed?: () => void }) {
  const [cards, setCards] = useState<MemoryCard[] | null>(null)
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState<'kept' | 'dropped' | 'all'>('kept')
  const [kind, setKind] = useState<string | null>(null)
  const showAll = filter !== 'kept'
  const [engine, setEngine] = useState<EngineReport | null>(null)
  const [selected, setSelected] = useState<MemoryCard | null>(null)
  const [editing, setEditing] = useState<MemoryCard | null>(null)

  useEffect(() => {
    if (initialQuery) {
      setQuery(initialQuery)
      onQueryConsumed?.()
    }
  }, [initialQuery, onQueryConsumed])

  const refresh = useCallback(() => {
    api.listMemories(query.trim() || undefined, !showAll, 200, filter === 'dropped').then(setCards).catch((e) => toast.error(String(e)))
    api.engineStatus().then(setEngine).catch(() => {})
  }, [query, showAll, filter])

  useEffect(() => {
    const t = setTimeout(refresh, 150)
    return () => clearTimeout(t)
  }, [refresh])

  useEffect(() => {
    const unlisten = api.onMemoriesChanged(refresh)
    const t = setInterval(() => api.engineStatus().then(setEngine).catch(() => {}), 3000)
    return () => {
      unlisten.then((fn) => fn())
      clearInterval(t)
    }
  }, [refresh])

  async function feedback(ids: number[], f: 'keep' | 'ignore' | null) {
    await api.memoryFeedback(ids, f)
    setCards((prev) => prev?.map((m) => (m.ids.some((i) => ids.includes(i)) ? { ...m, feedback: f } : m)) ?? null)
  }

  const kinds = new Map<string, number>()
  for (const m of cards ?? []) kinds.set(m.kind, (kinds.get(m.kind) ?? 0) + 1)
  const visible = (cards ?? []).filter((m) => !kind || m.kind === kind)
  const groups: { label: string; items: MemoryCard[] }[] = []
  for (const m of visible) {
    const label = dayLabel(m.startedAt)
    if (groups[groups.length - 1]?.label !== label) groups.push({ label, items: [] })
    groups[groups.length - 1].items.push(m)
  }
  const current = selected && visible.find((m) => m.id === selected.id) ? selected : null

  return (
    <div className="grid grid-cols-[200px_minmax(0,1fr)] h-full">
      {/* Left: views and filters */}
      <aside className="border-r p-3 flex flex-col min-h-0 overflow-y-auto gap-4">
        <h1 className="text-[19px] font-semibold tracking-tight px-1.5 pt-1">Memories</h1>
        <nav className="space-y-0.5">
          {VIEWS.map((v) => {
            const Icon = VIEW_ICON[v.id]
            return (
              <button key={v.id} onClick={() => onViewChange(v.id)} title={v.hint} className={cn('w-full flex items-center gap-2.5 rounded-xl px-2.5 py-2 text-[13.5px]', view === v.id ? 'bg-accent/60 font-medium text-foreground' : 'text-muted-foreground hover:bg-secondary hover:text-foreground')}>
                <Icon className={cn('h-4 w-4 shrink-0', view === v.id ? 'text-primary' : 'text-muted-foreground')} />
                {v.label}
              </button>
            )
          })}
        </nav>
        {view === 'memories' && (
          <>
            <div>
              <h3 className="text-[10.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground px-2.5 mb-1.5">Show</h3>
              {([['kept', 'Kept'], ['dropped', 'Dropped'], ['all', 'All']] as const).map(([id, lbl]) => (
                <button key={id} onClick={() => setFilter(id)} className={cn('w-full text-left rounded-lg px-2.5 py-1.5 text-[13.5px]', filter === id ? 'bg-accent/60 font-medium text-foreground' : 'text-muted-foreground hover:text-foreground hover:bg-secondary')} title={id === 'dropped' ? 'What Rabbit judged not worth keeping. Thumbs-up rescues one.' : undefined}>
                  {lbl}
                </button>
              ))}
            </div>
            {kinds.size > 1 && (
              <div>
                <h3 className="text-[10.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground px-2.5 mb-1.5">Kind</h3>
                <button onClick={() => setKind(null)} className={cn('w-full text-left rounded-lg px-2.5 py-1.5 text-[13.5px]', !kind ? 'bg-accent/60 font-medium text-foreground' : 'text-muted-foreground hover:text-foreground hover:bg-secondary')}>All</button>
                {[...kinds.entries()].sort((a, b) => b[1] - a[1]).map(([k, n]) => (
                  <button key={k} onClick={() => setKind(k)} className={cn('w-full text-left rounded-lg px-2.5 py-1.5 text-[13.5px] flex items-center justify-between gap-2 capitalize', kind === k ? 'bg-accent/60 font-medium text-foreground' : 'text-muted-foreground hover:text-foreground hover:bg-secondary')}>
                    <span className="truncate">{k}</span>
                    <span className="tabular-nums text-[12px] text-muted-foreground shrink-0">{n}</span>
                  </button>
                ))}
              </div>
            )}
            {engine && <div className="mt-auto px-2.5 pt-2"><EngineLine engine={engine} /></div>}
          </>
        )}
      </aside>

      {view === 'timeline' && <div className="overflow-y-auto"><TimelinePage /></div>}
      {view === 'tasks' && <div className="overflow-y-auto"><TasksPage /></div>}
      {view === 'everything' && <div className="overflow-y-auto"><SearchPage /></div>}

      {view === 'memories' && (
        <div className={cn('grid h-full min-h-0', current ? 'grid-cols-[minmax(0,1fr)_380px]' : 'grid-cols-1')}>
          {/* Centre: the cards */}
          <div className="px-5 py-5 overflow-y-auto min-w-0">
            <div className="flex items-center gap-2.5 mb-5">
              <div className="relative flex-1 min-w-0">
                <SearchIcon className="h-4 w-4 absolute left-3.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
                <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search memories: people, companies, numbers, topics…" className="w-full h-11 rounded-xl border bg-card pl-10 pr-3 text-[13.5px] outline-none placeholder:text-muted-foreground focus:ring-2 focus:ring-ring" />
              </div>
              <button
                onClick={() => { setKind(null); setFilter('kept'); setQuery('') }}
                title="Clear the filters: kept memories, every kind, no search"
                className="h-11 w-11 shrink-0 rounded-xl border bg-card grid place-items-center text-muted-foreground hover:bg-secondary hover:text-foreground"
              >
                <SlidersHorizontal className="h-4 w-4" />
              </button>
            </div>

            {engine && !engine.available && (
              <div className={cn('rounded-lg border p-3 text-sm mb-4', engine.downloadPercent != null || engine.busy ? 'border-amber-500/40 bg-amber-500/5' : 'border-destructive/30 bg-destructive/5')}>
                <span className="font-medium">Memory engine not ready:</span> {engine.detail}
                {engine.downloadPercent != null && ` (${engine.downloadPercent}%)`}. Capture continues; memories are made once the model is ready.
              </div>
            )}

            {cards && visible.length === 0 && (
              <EmptyState
                icon={Brain}
                title={query ? 'No memories match' : 'No memories yet'}
                description={query ? 'Try different words.' : engine?.counts.pending ? `${engine.counts.pending} activities are waiting to be processed. Each takes a few seconds on this Mac.` : 'Memories appear a couple of minutes after you finish an activity.'}
                action={engine?.counts.pending ? { label: 'Process now', onClick: () => api.processNow() } : undefined}
                tone="muted"
              />
            )}

            {groups.map((g) => (
              <section key={g.label} className="mb-6">
                <h2 className="text-xs font-medium uppercase tracking-wide text-muted-foreground mb-2">{g.label}</h2>
                <div className="space-y-2">
                  {g.items.map((m) => (
                    <div key={m.id} onClick={() => setSelected(m)} className="cursor-pointer">
                      <Card m={m} selected={current?.id === m.id} onOpen={() => setSelected(m)} onFeedback={feedback} onEdit={setEditing} onPin={(ids, p) => api.setPinned(ids, p).then(refresh).catch((e) => toast.error(String(e)))} />
                    </div>
                  ))}
                </div>
              </section>
            ))}

            {engine && engine.counts.pending > 0 && cards && cards.length > 0 && (
              <div className="flex items-center gap-3 text-xs text-muted-foreground">
                {engine.counts.pending} more waiting
                <Button size="sm" variant="outline" onClick={() => api.processNow()}>Process now</Button>
              </div>
            )}
          </div>

          {/* Right: the selected memory in full */}
          {current && (
            <aside className="border-l p-5 overflow-y-auto space-y-5">
              <div className="flex items-center gap-2 min-w-0">
                <span className="rounded-md bg-secondary px-2 py-1 text-[11px] font-medium capitalize shrink-0">{current.kind}</span>
                <SourceIcon app={current.appName} url={current.url} size={16} />
                <span className="text-[12.5px] truncate" title={current.appName}>{(hostOf(current.url) || current.appName).replace(/^www\./, '')}</span>
                <span className="text-[12px] text-muted-foreground truncate">· {format(current.startedAt, 'EEE d MMM, HH:mm')}</span>
                <button className="ml-auto shrink-0 rounded-lg p-1.5 text-muted-foreground hover:bg-secondary hover:text-foreground" onClick={() => setSelected(null)} title="Close" aria-label="Close">
                  <X className="h-4 w-4" />
                </button>
              </div>

              <div className="flex items-start gap-3">
                <h3 className="text-[19px] font-semibold leading-snug tracking-tight min-w-0 flex-1">{current.title}</h3>
                <span className="rounded-md bg-secondary px-2 py-1 text-[11px] tabular-nums text-muted-foreground shrink-0 mt-0.5">{Math.round(current.confidence * 100)}%</span>
              </div>

              <p className="text-[14px] leading-relaxed">{current.summary}</p>

              <div className="flex flex-wrap gap-2">
                <Button size="sm" variant="outline" onClick={() => setEditing(current)}><Pencil className="h-3.5 w-3.5 mr-1.5" /> Edit</Button>
                <Button size="sm" variant="outline" onClick={() => api.setPinned(current.ids, !current.pinned).then(refresh)}><Pin className="h-3.5 w-3.5 mr-1.5" /> {current.pinned ? 'Unpin' : 'Pin'}</Button>
                <Button size="sm" variant="outline" onClick={() => api.setBoardPosition(`m:${current.id}`, 200 + Math.random() * 200, 200 + Math.random() * 200).then(() => toast.success('Added to the Board'))}><Network className="h-3.5 w-3.5 mr-1.5" /> Add to Board</Button>
              </div>

              {current.facts.length > 0 && (
                <div>
                  <h4 className="text-[10.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground mb-2">Facts</h4>
                  <ul className="space-y-1.5 text-[13.5px]">
                    {current.facts.map((f: Fact) => (
                      <li key={f.id} className={cn(f.conflicts.length > 0 && 'text-amber-700 dark:text-amber-400')}>
                        {f.subject} · {f.attribute}: <span className="font-medium tabular-nums">{f.value}</span>
                        {f.conflicts.length > 0 && <span className="text-xs"> · elsewhere {f.conflicts.map((c) => c.value).join(', ')}</span>}
                      </li>
                    ))}
                  </ul>
                </div>
              )}

              {(current.people.length > 0 || current.organizations.length > 0 || current.projects.length > 0) && (
                <div>
                  <h4 className="text-[10.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground mb-2">Entities &amp; topics</h4>
                  <div className="flex flex-wrap gap-1.5">
                    {[...current.people, ...current.organizations, ...current.projects].map((x, i) => (
                      <span key={`${x}-${i}`} className="rounded-lg bg-secondary px-2.5 py-1 text-[11.5px]">{x}</span>
                    ))}
                  </div>
                </div>
              )}

              <div>
                <h4 className="text-[10.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground mb-2">What was on screen</h4>
                <ActivityDetailView id={current.activityId} header={false} onDeleted={() => { setSelected(null); refresh() }} />
              </div>
            </aside>
          )}
        </div>
      )}

      <EditMemoryDialog card={editing} onClose={() => setEditing(null)} onSaved={refresh} />
    </div>
  )
}
