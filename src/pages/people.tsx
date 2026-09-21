import { useCallback, useEffect, useRef, useState } from 'react'
import { format } from 'date-fns'
import { Building2, FolderKanban, Loader2, MessageSquare, Network, Plus, RefreshCw, Search as SearchIcon, Sparkles, Trash2, User, Users } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { EmptyState } from '@/components/ui/empty-state'
import { ActivitySheet } from '@/components/activity-sheet'
import { Card } from '@/pages/memories'
import { api, type Entity, type MemoryCard } from '@/lib/api'
import { cn } from '@/lib/utils'

/** Up to two initials, for the avatar. */
function initials(name: string) {
  const parts = name.trim().split(/\s+/).filter(Boolean)
  return ((parts[0]?.[0] ?? '') + (parts.length > 1 ? parts[parts.length - 1][0] : '')).toUpperCase() || '?'
}

const KINDS = [
  { id: 'person', label: 'People', icon: User },
  { id: 'org', label: 'Organisations', icon: Building2 },
  { id: 'project', label: 'Projects', icon: FolderKanban },
] as const

/** The directory: everyone and everything Lane knows, with a compiled profile. Three panes. */
export function PeoplePage({ onAsk }: { onAsk: (q: string) => void }) {
  const [kind, setKind] = useState<'person' | 'org' | 'project'>('person')
  const [query, setQuery] = useState('')
  const [list, setList] = useState<Entity[]>([])
  const [selected, setSelected] = useState<Entity | null>(null)
  const [confirmForget, setConfirmForget] = useState(false)
  const [cards, setCards] = useState<MemoryCard[]>([])
  const [aliases, setAliases] = useState<string[]>([])
  const [alias, setAlias] = useState('')
  const [adding, setAdding] = useState(false)
  const [order, setOrder] = useState<'relevant' | 'newest' | 'oldest'>('relevant')
  const [profile, setProfile] = useState('')
  const [profileBusy, setProfileBusy] = useState(false)
  const [openId, setOpenId] = useState<number | null>(null)
  const profileReq = useRef<number | null>(null)

  const load = useCallback(() => {
    api.listEntities(kind, query.trim() || undefined, 300).then(setList).catch((e) => toast.error(String(e)))
  }, [kind, query])

  useEffect(() => {
    const t = setTimeout(load, 120)
    return () => clearTimeout(t)
  }, [load])

  useEffect(() => {
    const a = api.onAskToken(({ id, token }) => { if (id === profileReq.current) setProfile((t) => t + token) })
    const b = api.onAskDone((r) => {
      if (r.id !== profileReq.current) return
      setProfileBusy(false)
      if (r.error) toast.error(r.error)
      else if (r.answer) setProfile(r.answer)
    })
    return () => { a.then((f) => f()); b.then((f) => f()) }
  }, [])

  async function pick(e: Entity, force = false) {
    setSelected(e)
    setProfile('')
    setProfileBusy(true)
    api.entityMemories(e.id).then(setCards).catch(() => setCards([]))
    api.aliasesOf(e.id).then(setAliases).catch(() => setAliases([]))
    profileReq.current = await api.entityProfile(e.id, force)
  }

  /* 'Most relevant' is the order the store returns; the others re-sort here. */
  const sortedCards = order === 'relevant' ? cards : [...cards].sort((a, b) => order === 'newest' ? b.startedAt - a.startedAt : a.startedAt - b.startedAt)

  const verb = kind === 'person' ? 'Who is' : kind === 'org' ? 'What do I know about' : 'What is the state of'

  return (
    <div className="grid grid-cols-[220px_minmax(0,1fr)] h-full">
      <aside className="border-r p-3 space-y-3 overflow-y-auto">
        <nav className="space-y-0.5">
          {KINDS.map((k) => (
            <button key={k.id} onClick={() => { setKind(k.id); setSelected(null) }} className={cn('w-full flex items-center gap-2.5 rounded-xl px-2.5 py-2 text-[13.5px]', kind === k.id ? 'bg-accent/60 font-medium text-foreground' : 'text-muted-foreground hover:bg-secondary hover:text-foreground')}>
              <k.icon className={cn('h-4 w-4 shrink-0', kind === k.id ? 'text-primary' : 'text-muted-foreground')} /> {k.label}
            </button>
          ))}
        </nav>
        <div className="relative">
          <SearchIcon className="h-4 w-4 absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground" />
          <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder={kind === 'person' ? 'Find people…' : kind === 'org' ? 'Find organisations…' : 'Find projects…'} className="w-full h-10 rounded-xl border bg-card pl-9.5 pr-3 text-[13px] outline-none placeholder:text-muted-foreground focus:ring-2 focus:ring-ring" style={{ paddingLeft: '2.4rem' }} />
        </div>
        <ul className="space-y-0.5">
          {list.map((e) => (
            <li key={e.id}>
              <button onClick={() => pick(e)} className={cn('w-full text-left rounded-xl px-3 py-2 text-[13.5px] flex items-center gap-2', selected?.id === e.id ? 'bg-accent/60 font-medium text-foreground' : 'text-foreground/90 hover:bg-secondary')}>
                <span className="truncate flex-1">{e.name}</span>
                <span className="text-[12px] text-muted-foreground tabular-nums shrink-0">{e.mentions}</span>
              </button>
            </li>
          ))}
          {list.length === 0 && <li className="text-xs text-muted-foreground px-2.5">Nobody yet.</li>}
        </ul>
      </aside>

      {!selected ? (
        <div className="p-8">
          <EmptyState icon={Users} title="Pick someone" description="Lane compiles what it knows: role, what you worked on together, numbers, dates and what is still open." tone="muted" />
        </div>
      ) : (
        <div className="grid grid-cols-[minmax(0,1fr)_360px] h-full min-h-0">
          <div className="px-6 py-6 overflow-y-auto min-w-0 space-y-5">
            <div className="flex items-start gap-4">
              <span className="h-16 w-16 shrink-0 rounded-full bg-accent/70 grid place-items-center text-[19px] font-semibold text-primary">
                {initials(selected.name)}
              </span>
              <div className="min-w-0">
                <h1 className="text-[30px] font-semibold tracking-[-0.03em] leading-none">{selected.name}</h1>
                <p className="text-[13px] text-muted-foreground mt-2">
                  {kind === 'org' ? 'Organisation' : kind === 'person' ? 'Person' : 'Project'} · mentioned {selected.mentions}× · {format(selected.firstSeen, 'd MMM yyyy')} to {format(selected.lastSeen, 'd MMM yyyy')} · {cards.length} {cards.length === 1 ? 'memory' : 'memories'}
                </p>
              </div>
            </div>

            <div className="flex flex-wrap items-center gap-2 text-[12.5px]">
              <span className="text-muted-foreground">Also known as:</span>
              {aliases.map((a) => (
                <span key={a} className="inline-flex items-center gap-1.5 rounded-full bg-secondary px-3 py-1.5">
                  {a}
                  <button className="text-muted-foreground hover:text-destructive" title="Remove this name" onClick={() => api.removeAlias(selected.id, a).then(setAliases)}>×</button>
                </span>
              ))}
              {adding ? (
                <input
                  autoFocus
                  className="h-8 rounded-full border bg-card px-3 text-[12.5px] w-40 outline-none focus:ring-2 focus:ring-ring"
                  placeholder="another name, then ↩"
                  value={alias}
                  onChange={(e) => setAlias(e.target.value)}
                  onBlur={() => { if (!alias.trim()) setAdding(false) }}
                  onKeyDown={(e) => {
                    if (e.key === 'Escape') { setAlias(''); setAdding(false) }
                    if (e.key === 'Enter' && alias.trim()) api.setAlias(selected.id, alias).then((l) => { setAliases(l); setAlias(''); setAdding(false) }).catch((er) => toast.error(String(er)))
                  }}
                />
              ) : (
                <button onClick={() => setAdding(true)} className="inline-flex items-center gap-1.5 rounded-full border bg-card px-3 py-1.5 hover:bg-secondary">
                  <Plus className="h-3.5 w-3.5 text-primary" /> add a name
                </button>
              )}
            </div>

            <div className="rounded-2xl border bg-card p-5 text-[15px] leading-relaxed whitespace-pre-wrap select-text min-h-[140px]">
              <div className="flex items-center gap-2 mb-3">
                <Sparkles className="h-4 w-4 text-primary" />
                <span className="text-[10.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground">What Lane knows</span>
                <button className="ml-auto rounded-lg p-1.5 text-muted-foreground hover:bg-secondary hover:text-foreground disabled:opacity-50" disabled={profileBusy} title="Compile again" aria-label="Compile again" onClick={() => pick(selected, true)}>
                  <RefreshCw className={cn('h-3.5 w-3.5', profileBusy && 'animate-spin')} />
                </button>
              </div>
              {profile || (profileBusy ? <span className="text-muted-foreground flex items-center gap-2 text-sm"><Loader2 className="h-3.5 w-3.5 animate-spin" /> Compiling…</span> : <span className="text-muted-foreground text-sm">No profile yet.</span>)}
            </div>

            <div className="flex flex-wrap items-center gap-2">
              <Button size="sm" variant="outline" onClick={() => onAsk(`${verb} ${selected.name}?`)}><MessageSquare className="h-3.5 w-3.5 mr-1.5" /> Ask about {kind === 'person' ? 'them' : 'it'}</Button>
              <Button size="sm" variant="outline" onClick={() => onAsk(`What is still open with ${selected.name}?`)}><Sparkles className="h-3.5 w-3.5 mr-1.5" /> What is open</Button>
              <Button size="sm" variant="outline" onClick={() => api.setBoardPosition(`e:${selected.id}`, 200 + Math.random() * 200, 200 + Math.random() * 200).then(() => toast.success('Added to the Board'))}><Network className="h-3.5 w-3.5 mr-1.5" /> Add to Board</Button>
              {confirmForget ? (
                <span className="flex items-center gap-1.5 text-[12.5px]">
                  Remove every mention of {selected.name}? Cannot be undone.
                  <Button size="sm" variant="destructive" className="h-7 px-2 text-xs" onClick={() => { setConfirmForget(false); api.forgetTerm(selected.name).then((r) => { toast.success(`Forgotten from ${r.memories} memories`); setSelected(null); load() }).catch((e) => toast.error(String(e))) }}>Forget</Button>
                  <Button size="sm" variant="ghost" className="h-7 px-2 text-xs" onClick={() => setConfirmForget(false)}>Keep</Button>
                </span>
              ) : (
                <Button size="sm" variant="ghost" className="text-destructive hover:text-destructive" onClick={() => setConfirmForget(true)}><Trash2 className="h-3.5 w-3.5 mr-1.5" /> Forget</Button>
              )}
            </div>
          </div>

          <aside className="border-l p-4 overflow-y-auto space-y-3">
            <div className="flex items-center gap-2">
              <h2 className="text-[15px] font-semibold tracking-tight">Memories</h2>
              <span className="rounded-full bg-secondary px-2 py-0.5 text-[11.5px] tabular-nums text-muted-foreground">{cards.length}</span>
              <div className="ml-auto flex items-center gap-1.5 rounded-full border bg-card px-2.5 py-1 text-[12px] text-muted-foreground">
                <select value={order} onChange={(e) => setOrder(e.target.value as typeof order)} className="bg-transparent outline-none cursor-pointer text-foreground">
                  <option value="relevant">Most relevant</option>
                  <option value="newest">Newest first</option>
                  <option value="oldest">Oldest first</option>
                </select>
              </div>
            </div>
            {sortedCards.map((m) => (
              <Card key={m.id} m={m} onOpen={setOpenId} onFeedback={(ids, f) => api.memoryFeedback(ids, f)} />
            ))}
            {cards.length === 0 && <p className="text-[13px] text-muted-foreground">No memories mention them yet.</p>}
          </aside>
        </div>
      )}
      <ActivitySheet id={openId} onClose={() => setOpenId(null)} onDeleted={() => selected && pick(selected)} />
    </div>
  )
}
