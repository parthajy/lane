import { useCallback, useEffect, useState } from 'react'
import { ArrowDown, ArrowUp, Check, ChevronDown, ChevronRight, Lightbulb, PenLine, Pin, RefreshCw, Sparkles, VolumeX } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Ring } from '@/components/charts'
import { api, type Settings, type Signal, type SignalsReport } from '@/lib/api'
import { cn } from '@/lib/utils'

/** The why, in the person's words, editable in place. */
export function WhyLine({ settings, onSaved }: { settings: Settings | null; onSaved: (s: Settings) => void }) {
  const [editing, setEditing] = useState(false)
  const [why, setWhy] = useState('')
  const [how, setHow] = useState('')
  useEffect(() => {
    if (settings) {
      setWhy(settings.purposeWhy)
      setHow(settings.purposeHow.join('\n'))
    }
  }, [settings])
  if (!settings) return null
  async function save() {
    try {
      const next = await api.updateSettings({ ...settings!, purposeWhy: why.trim(), purposeHow: how.split('\n').map((l) => l.trim()).filter(Boolean) })
      onSaved(next)
      setEditing(false)
      toast.success(next.purposeWhy ? 'Your why is set. Today is being re-read against it.' : 'Cleared')
    } catch (e) {
      toast.error(String(e))
    }
  }
  if (editing) {
    return (
      <div className="rounded-2xl bg-secondary/60 p-4 space-y-3">
        <div>
          <div className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground mb-1">Why · what you are building and for whom</div>
          <textarea autoFocus value={why} onChange={(e) => setWhy(e.target.value)} rows={3} className="w-full rounded-lg border bg-background p-3 text-[15px] leading-relaxed" placeholder="I'm building Lane so knowledge stops dying inside organisations." />
        </div>
        <div>
          <div className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground mb-1">How · your principles, one per line</div>
          <textarea value={how} onChange={(e) => setHow(e.target.value)} rows={3} className="w-full rounded-lg border bg-background p-3 text-sm leading-relaxed" placeholder={'Ship weekly.\nNothing leaves the user\'s Mac.\nSay no by default.'} />
        </div>
        <div className="flex gap-2">
          <Button size="sm" onClick={save}>Save</Button>
          <Button size="sm" variant="ghost" onClick={() => setEditing(false)}>Cancel</Button>
        </div>
      </div>
    )
  }
  if (!settings.purposeWhy) {
    return (
      <div className="rounded-2xl tone-violet p-4 flex items-center gap-3.5">
        <span className="h-10 w-10 shrink-0 rounded-xl bg-background grid place-items-center text-primary"><Sparkles className="h-[18px] w-[18px]" /></span>
        <div className="min-w-0 flex-1">
          <div className="text-[14.5px] font-semibold">Write your why.</div>
          <div className="text-[13px] text-muted-foreground mt-0.5">One paragraph on what you are building and for whom, and a few principles. Lane ranks every day against it and tells you when you drift.</div>
        </div>
        <Button size="sm" variant="outline" className="shrink-0 bg-background" onClick={() => setEditing(true)}><PenLine className="h-3.5 w-3.5 mr-1.5" /> Write</Button>
      </div>
    )
  }
  return (
    <button onClick={() => setEditing(true)} className="w-full text-left rounded-2xl bg-secondary/60 px-4 py-3 hover:bg-accent/60 min-w-0" title="Click to edit">
      <div className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">Why</div>
      <div className="text-[15px] leading-snug line-clamp-2">{settings.purposeWhy}</div>
      {settings.purposeHow.length > 0 && <div className="text-xs text-muted-foreground mt-1 truncate">How · {settings.purposeHow.join(' · ')}</div>}
    </button>
  )
}

/** High / Medium / Low, taken from the features the ranking already used. */
function Priority({ s }: { s: Signal }) {
  /* Read it off the reason the ranking already wrote, so the chip says what
     the line underneath says. */
  const r = (s.reason || '').toLowerCase()
  const level = /overdue|due today|due tomorrow/.test(r) ? 'High'
    : /due in [1-3] days|is waiting/.test(r) ? 'Medium'
    : 'Low'
  const tone = level === 'High' ? 'bg-rose-50 text-rose-600 dark:bg-rose-500/15 dark:text-rose-300'
    : level === 'Medium' ? 'bg-blue-50 text-blue-600 dark:bg-blue-500/15 dark:text-blue-300'
    : 'bg-secondary text-muted-foreground'
  return (
    <span className={cn('hidden sm:inline-flex items-center gap-1.5 shrink-0 rounded-full px-2.5 py-1 text-[11.5px] font-medium', tone)} title={s.reason}>
      <span className="h-1.5 w-1.5 rounded-full bg-current" /> {level}
    </span>
  )
}

const KIND: Record<Signal['kind'], string> = { task: 'commitment', event: 'meeting', date: 'date', person: 'person', thread: 'thread', memory: 'memory' }

/** The three things that matter today, ranked with reasons; the rest folded away. */
export function ThreeThings({ onOpen, onAsk, compact = false }: { onOpen?: (activityId: number) => void; onAsk?: (q: string) => void; compact?: boolean }) {
  const [report, setReport] = useState<SignalsReport | null>(null)
  const [busy, setBusy] = useState(false)
  const [showRest, setShowRest] = useState(false)
  const load = useCallback(() => api.signals().then(setReport).catch(() => {}), [])
  useEffect(() => {
    load()
    const un = api.onSignalsChanged(load)
    return () => { un.then((f) => f()) }
  }, [load])

  async function act(s: Signal, state: Signal['state']) {
    await api.setSignalState(s.id, state).catch((e) => toast.error(String(e)))
    load()
  }
  async function move(s: Signal, dir: -1 | 1) {
    if (!report) return
    const order = report.signals.filter((x) => x.state !== 'noise').map((x) => x.id)
    const i = order.indexOf(s.id)
    const j = i + dir
    if (i < 0 || j < 0 || j >= order.length) return
    ;[order[i], order[j]] = [order[j], order[i]]
    await api.rankSignals(order).catch((e) => toast.error(String(e)))
    load()
  }
  async function refresh() {
    setBusy(true)
    try { await api.refreshSignals(); await load() } catch (e) { toast.error(String(e)) } finally { setBusy(false) }
  }

  const live = (report?.signals ?? []).filter((s) => s.state !== 'noise')
  const top = live.slice(0, 3)
  const rest = live.slice(3)
  const noise = (report?.signals ?? []).filter((s) => s.state === 'noise')
  const done = top.filter((s) => s.state === 'done').length

  return (
    <section className="rounded-2xl border bg-card p-1 min-w-0">
      <div className="flex items-center gap-2 px-4 pt-3.5 pb-2.5">
        <h2 className="text-[17px] font-semibold tracking-tight">Three things</h2>
        <span className="ml-auto flex items-center gap-2.5 text-[12px] text-muted-foreground">
          {report?.alignment && report.whySet && <span title="Of today's memories, how many sat near your why">{report.alignment[2]} of {report.alignment[1]} near your why</span>}
          {top.length > 0 && <span className="tabular-nums">{done} of {top.length} done</span>}
          {top.length > 0 && <Ring done={done} total={top.length} />}
          <button className="hover:text-foreground" aria-label="Rank again" title="Rank again" onClick={refresh}><RefreshCw className={cn('h-3.5 w-3.5', busy && 'animate-spin')} /></button>
        </span>
      </div>
      {top.length === 0 && <p className="px-4 pb-4 text-sm text-muted-foreground">Nothing stands out yet. As commitments, dates and threads build up, the three that matter appear here each morning.</p>}
      <ol className="divide-y">
        {top.map((s, i) => (
          <li key={s.id} className={cn('flex items-start gap-3 px-3 py-3', s.state === 'done' && 'opacity-55', s.state === 'pinned' && 'bg-primary/[0.04]')}>
            <span className="h-7 w-7 shrink-0 rounded-lg bg-secondary grid place-items-center text-[13px] font-semibold tabular-nums text-muted-foreground">{i + 1}</span>
            <div className="min-w-0 flex-1">
              <button className={cn('text-left text-[14.5px] leading-snug font-medium', s.state === 'done' && 'line-through', s.activityId && 'hover:underline')} onClick={() => s.activityId && onOpen?.(s.activityId)}>
                {s.title}
              </button>
              <div className="text-[12px] text-muted-foreground mt-1">
                <span className="capitalize">{KIND[s.kind]}</span>{s.reason && ` · ${s.reason}`}
              </div>
            </div>
            {!compact && <Priority s={s} />}
            {!compact && (
              <div className="flex items-center gap-0.5 shrink-0 text-muted-foreground">
                <button className="p-1.5 rounded-lg hover:bg-secondary" aria-label="Move up" title="Move up" onClick={() => move(s, -1)} disabled={i === 0}><ArrowUp className="h-3.5 w-3.5" /></button>
                <button className="p-1.5 rounded-lg hover:bg-secondary" aria-label="Move down" title="Move down" onClick={() => move(s, 1)}><ArrowDown className="h-3.5 w-3.5" /></button>
                <button className={cn('p-1.5 rounded-lg hover:bg-secondary', s.state === 'pinned' && 'text-primary')} aria-label={s.state === 'pinned' ? 'Unpin' : 'Pin: keep it in the three'} title={s.state === 'pinned' ? 'Unpin' : 'Pin: keep it in the three'} onClick={() => act(s, s.state === 'pinned' ? 'open' : 'pinned')}><Pin className="h-3.5 w-3.5" /></button>
                <button className={cn('p-1.5 rounded-lg hover:bg-secondary', s.state === 'done' && 'text-primary')} aria-label="Done" title="Done" onClick={() => act(s, s.state === 'done' ? 'open' : 'done')}><Check className="h-3.5 w-3.5" /></button>
                <button className="p-1.5 rounded-lg hover:bg-secondary" aria-label="Noise: not a thing that matters" title="Noise: not a thing that matters" onClick={() => act(s, 'noise')}><VolumeX className="h-3.5 w-3.5" /></button>
              </div>
            )}
            {compact && s.state !== 'done' && (
              <button className="p-1 rounded hover:bg-accent shrink-0" aria-label="Done" title="Done" onClick={() => act(s, 'done')}><Check className="h-3.5 w-3.5" /></button>
            )}
          </li>
        ))}
      </ol>
      {!compact && (rest.length > 0 || noise.length > 0) && (
        <div className="border-t px-4 py-2.5">
          <button className="text-[12.5px] text-muted-foreground hover:text-foreground flex items-center gap-1" onClick={() => setShowRest((v) => !v)}>
            {showRest ? 'Hide' : 'Show'} the rest · {rest.length}{noise.length ? ` · ${noise.length} called noise` : ''}
            <ChevronDown className={cn('h-3.5 w-3.5 transition-transform', showRest && 'rotate-180')} />
          </button>
          {showRest && (
            <ul className="mt-2 space-y-1.5">
              {rest.map((s) => (
                <li key={s.id} className="flex items-start gap-2 text-sm">
                  <span className="min-w-0 flex-1">
                    <span className="block leading-snug">{s.title}</span>
                    <span className="block text-xs text-muted-foreground"><span className="capitalize">{KIND[s.kind]}</span>{s.reason && ` · ${s.reason}`}</span>
                  </span>
                  <button className="p-1 rounded hover:bg-accent" aria-label="Lift into the three" title="Lift into the three" onClick={() => act(s, 'pinned')}><ArrowUp className="h-3.5 w-3.5" /></button>
                  <button className="p-1 rounded hover:bg-accent" aria-label="Noise" title="Noise" onClick={() => act(s, 'noise')}><VolumeX className="h-3.5 w-3.5" /></button>
                </li>
              ))}
              {noise.map((s) => (
                <li key={s.id} className="flex items-start gap-2 text-sm opacity-50">
                  <span className="min-w-0 flex-1 line-through leading-snug">{s.title}</span>
                  <button className="text-xs hover:underline" onClick={() => act(s, 'open')}>not noise</button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
      {onAsk && top.length > 0 && (
        <button onClick={() => onAsk('What should I do first today, and why?')} className="m-1 w-[calc(100%-0.5rem)] flex items-center gap-3 rounded-xl bg-secondary/70 px-3.5 py-3 text-left hover:bg-secondary">
          <span className="h-8 w-8 shrink-0 rounded-lg bg-background grid place-items-center text-primary"><Lightbulb className="h-4 w-4" /></span>
          <span className="min-w-0 flex-1">
            <span className="block text-[13.5px] font-medium">Why these three?</span>
            <span className="block text-[12px] text-muted-foreground truncate">Lane ranks this list on what is due, who is waiting and what sits near your why.</span>
          </span>
          <ChevronRight className="h-4 w-4 shrink-0 text-muted-foreground" />
        </button>
      )}
    </section>
  )
}
