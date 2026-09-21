import { useCallback, useEffect, useRef, useState } from 'react'
import { format, isToday, isYesterday, subDays } from 'date-fns'
import { CalendarClock, CalendarDays, CheckSquare, ChevronLeft, ChevronRight, Clock, FileText, Lightbulb, ListChecks, Loader2, Lock, Quote, RefreshCw, Sunrise, Users } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { ActivitySheet } from '@/components/activity-sheet'
import { AppRows, Bars, Donut, StatCard } from '@/components/charts'
import { ThreeThings, WhyLine } from '@/components/three-things'
import { api, formatDuration, type AskResult, type CalendarEvent, type Entity, type MemoryCard, type Settings, type DayStats, type EngineReport, type Explore, type Gap, type Upcoming } from '@/lib/api'
import { cn } from '@/lib/utils'

function label(d: Date) {
  if (isToday(d)) return 'Today'
  if (isYesterday(d)) return 'Yesterday'
  return format(d, 'EEEE d MMMM')
}

function Pane({ title, icon: Icon, children, className, aside }: { title: string; icon?: typeof Sunrise; children: React.ReactNode; className?: string; aside?: React.ReactNode }) {
  return (
    <section className={cn('rounded-2xl bg-secondary/60 p-4 min-w-0', className)}>
      <h2 className="text-[11px] font-medium uppercase tracking-[0.1em] text-muted-foreground mb-2.5 flex items-center gap-1.5">
        {Icon && <Icon className="h-3.5 w-3.5" />} {title}
        {aside && <span className="ml-auto normal-case tracking-normal font-normal">{aside}</span>}
      </h2>
      {children}
    </section>
  )
}

/** One line per source under the briefing: enough to recognise it, a click to open it. */
function SourceRow({ n, m, onOpen }: { n: number; m: MemoryCard; onOpen: (id: number) => void }) {
  return (
    <button onClick={() => onOpen(m.activityId)} className="w-full text-left flex items-baseline gap-3 rounded-lg px-3 py-2 hover:bg-accent/60 min-w-0">
      <span className="text-[11px] font-semibold text-muted-foreground tabular-nums w-6 shrink-0">[{n}]</span>
      <span className="flex-1 min-w-0">
        <span className="block text-sm truncate">{m.title}</span>
        <span className="block text-xs text-muted-foreground truncate">{m.summary}</span>
      </span>
      <span className="text-[11px] text-muted-foreground shrink-0 tabular-nums">{m.appName} · {format(m.startedAt, 'HH:mm')}</span>
    </button>
  )
}

/** Start my day: what is coming, the briefing, and the shape of the day. Three panes. */
export function TodayPage({ onAsk, onOpenTasks }: { onAsk: (q: string) => void; onOpenTasks?: () => void }) {
  const [day, setDay] = useState(() => subDays(new Date(), 1))
  const [span, setSpan] = useState<'day' | 'week' | 'month'>('day')
  const [upcoming, setUpcoming] = useState<Upcoming[]>([])
  const [gaps, setGaps] = useState<Gap[]>([])
  const [text, setText] = useState('')
  const [result, setResult] = useState<AskResult | null>(null)
  const [stats, setStats] = useState<DayStats | null>(null)
  const [explore, setExplore] = useState<Explore | null>(null)
  const [busy, setBusy] = useState(false)
  const [openId, setOpenId] = useState<number | null>(null)
  const [events, setEvents] = useState<CalendarEvent[]>([])
  const [engine, setEngine] = useState<EngineReport | null>(null)
  const [latest, setLatest] = useState<MemoryCard[]>([])
  const [settings, setSettings] = useState<Settings | null>(null)
  const reqId = useRef<number | null>(null)

  useEffect(() => {
    if (!busy) return
    const tick = () => api.engineStatus().then(setEngine).catch(() => {})
    tick()
    const t = setInterval(tick, 3000)
    return () => clearInterval(t)
  }, [busy])

  useEffect(() => {
    const tick = () => api.upcomingEvents(24).then(setEvents).catch(() => setEvents([]))
    tick()
    const t = setInterval(tick, 5 * 60_000)
    return () => clearInterval(t)
  }, [])

  useEffect(() => {
    api.upcomingDates(30).then(setUpcoming).catch(() => {})
    api.memoryGaps().then(setGaps).catch(() => {})
    api.explore().then(setExplore).catch(() => {})
    api.getSettings().then(setSettings).catch(() => {})
    const loadLatest = () => api.listMemories(undefined, true, 8).then(setLatest).catch(() => {})
    loadLatest()
    const un = api.onMemoriesChanged(loadLatest)
    return () => { un.then((f) => f()) }
  }, [])

  const load = useCallback(
    async (force = false) => {
      const ms = day.getTime()
      setText('')
      setResult(null)
      setBusy(true)
      api.dayStats(ms).then(setStats).catch(() => {})
      reqId.current = await api.recap(ms, force, span)
    },
    [day, span],
  )

  useEffect(() => {
    load()
  }, [load])

  useEffect(() => {
    const a = api.onAskToken(({ id, token }) => {
      if (id === reqId.current) setText((t) => t + token)
    })
    const b = api.onAskDone((r) => {
      if (r.id !== reqId.current) return
      setBusy(false)
      setResult(r)
      if (r.error) toast.error(r.error)
      else setText(r.answer)
    })
    return () => {
      a.then((f) => f())
      b.then((f) => f())
    }
  }, [])

  const canForward = !isToday(day)
  const step = span === 'month' ? 30 : span === 'week' ? 7 : 1
  const title = span === 'month' ? format(day, 'MMMM yyyy') : span === 'week' ? `Week of ${format(day, 'd MMM')}` : label(day)
  const kinds = explore?.byKind.slice(0, 6) ?? []
  const perDay = explore?.memoriesPerDay.slice(-14) ?? []
  const timeByApp: [string, number][] = stats?.timeByApp.slice(0, 6) ?? []
  const perDayCounts = perDay.map((d) => d[1])
  const capturedMs = timeByApp.reduce((a, d) => a + d[1], 0) * 60000
  /* Week on week, from the same series the chart draws - no invented figures. */
  const weekDelta = (() => {
    if (perDayCounts.length < 14) return null
    const a = perDayCounts.slice(-7).reduce((x, y) => x + y, 0)
    const b = perDayCounts.slice(-14, -7).reduce((x, y) => x + y, 0)
    if (!b) return null
    return Math.round(((a - b) / b) * 100)
  })()
  const avgPerDay = perDayCounts.length ? Math.round(perDayCounts.reduce((x, y) => x + y, 0) / perDayCounts.length) : 0
  const kindTotal = kinds.reduce((a, k) => a + k[1], 0)
  const short = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(1)}K` : String(n))
  const people: Entity[] = explore?.topPeople.slice(0, 8) ?? []

  async function markDone(id: number) {
    await api.setTaskStatus(id, 'done')
    setStats((s) => (s ? { ...s, openTasks: s.openTasks.filter((t) => t.id !== id) } : s))
  }

  return (
    <div className="px-6 py-5">
      <div className="flex items-end justify-between mb-5 gap-4 flex-wrap">
        <div>
          <p className="text-[12.5px] text-muted-foreground mb-2">{format(day, 'EEE, d MMM yyyy')}</p>
          <h1 className="text-[34px] font-semibold tracking-[-0.03em] leading-none">{title}</h1>
          <p className="text-[13.5px] text-muted-foreground mt-1.5">What you did, what you owe, and what matters next.</p>
        </div>
        <div className="flex items-center gap-1.5">
          <div className="flex rounded-full bg-secondary p-1 mr-1">
            {(['day', 'week', 'month'] as const).map((sp) => (
              <button key={sp} onClick={() => setSpan(sp)} className={cn('text-[12.5px] rounded-full px-3.5 py-1.5 capitalize transition-colors', span === sp ? 'bg-foreground text-background font-medium' : 'text-muted-foreground hover:text-foreground')}>
                {sp}
              </button>
            ))}
          </div>
          <Button size="icon-sm" variant="ghost" onClick={() => setDay((d) => subDays(d, step))} title="Previous"><ChevronLeft className="h-4 w-4" /></Button>
          <Button size="icon-sm" variant="ghost" disabled={!canForward} onClick={() => setDay((d) => subDays(d, -step))} title="Next"><ChevronRight className="h-4 w-4" /></Button>
          <Button size="sm" variant="ghost" disabled={busy} onClick={() => load(true)} title="Write it again"><RefreshCw className="h-3.5 w-3.5 mr-1" /> Redo</Button>
        </div>
      </div>

      {/* The numbers of the day in one strip, so the columns below start level. */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-3 mb-4">
        <StatCard icon={FileText} tone="blue" n={stats?.memories ?? 0} label={span === 'day' ? 'memories' : 'memories · first day'} delta={weekDelta} series={perDayCounts} />
        <StatCard icon={CheckSquare} tone="violet" n={stats?.openTaskCount ?? 0} label="open commitments" />
        <StatCard icon={Clock} tone="amber" n={formatDuration(capturedMs) || '0m'} label="captured" />
      </div>

      <div className="grid grid-cols-[minmax(0,1fr)_320px] gap-4 items-start">
        {/* Left: the why, the three things, the briefing, its sources, the latest memories */}
        <div className="space-y-4 min-w-0">
          {/* Always about today, whichever day's briefing is open below. */}
          <WhyLine settings={settings} onSaved={setSettings} />
          <ThreeThings onOpen={setOpenId} onAsk={onAsk} />
          <div className="rounded-2xl bg-secondary/60 p-5 text-[15px] leading-relaxed whitespace-pre-wrap select-text min-h-[140px]">
            {text || (busy ? (
              <span className="text-muted-foreground flex items-center gap-2">
                <Loader2 className="h-4 w-4 animate-spin" />
                {!engine?.available && engine?.downloadPercent != null
                  ? `Downloading the memory model… ${engine.downloadPercent}%`
                  : engine?.busy && engine.counts.pending > 0
                    ? `Writing your briefing… (Rabbit is also catching up on ${engine.counts.pending} memories in the background)`
                    : 'Writing your briefing…'}
              </span>
            ) : <span className="text-muted-foreground">Nothing was captured for this {span}.</span>)}
          </div>
          <div className="flex flex-wrap gap-2">
            {['What should I follow up on?', 'Who did I deal with?', 'What numbers came up?'].map((q) => (
              <Button key={q} size="sm" variant="outline" onClick={() => onAsk(`${q} (${title.toLowerCase()})`)}>{q}</Button>
            ))}
          </div>
          {result && result.sources.some((s) => s.card) && (
            <Pane title={`Written from ${result.sources.filter((s) => s.card).length} memories`}>
              <div className="-mx-3 -mb-2 divide-y divide-border/60">
                {result.sources.map((s) => s.card && <SourceRow key={s.n} n={s.n} m={s.card} onOpen={setOpenId} />)}
              </div>
            </Pane>
          )}

          {people.length > 0 && (
            <Pane title="People this month" icon={Users}>
              <div className="flex flex-wrap gap-1.5">
                {people.map((p) => (
                  <button key={p.id} onClick={() => onAsk(`What is the latest with ${p.name}, and is anything open?`)} className="inline-flex items-center gap-1.5 rounded-full border bg-background px-2.5 py-1 text-xs hover:bg-accent" title="Ask about them">
                    <span className="h-4 w-4 rounded-full bg-primary/10 text-primary text-[10px] font-semibold flex items-center justify-center">{p.name.charAt(0)}</span>
                    {p.name}
                    <span className="text-muted-foreground tabular-nums">{p.mentions}</span>
                  </button>
                ))}
              </div>
            </Pane>
          )}

          {latest.length > 0 && (
            <Pane title="Latest memories" icon={Clock}>
              <div className="grid grid-cols-2 gap-2">
                {latest.map((m) => (
                  <button key={m.id} onClick={() => setOpenId(m.activityId)} className="text-left rounded-lg border bg-background p-3 hover:bg-accent/60 min-w-0">
                    <div className="text-sm font-medium leading-snug line-clamp-2">{m.title}</div>
                    <div className="text-xs text-muted-foreground mt-1 line-clamp-2">{m.summary}</div>
                    <div className="text-[11px] text-muted-foreground mt-1.5 truncate tabular-nums">{m.appName} · {isToday(m.startedAt) ? format(m.startedAt, 'HH:mm') : format(m.startedAt, 'EEE d MMM')}</div>
                  </button>
                ))}
              </div>
            </Pane>
          )}
        </div>

        {/* Right: the shape of the day first, then what is coming and owed */}
        <div className="space-y-3">
          <section className="rounded-2xl border bg-card p-4">
            <h2 className="flex items-baseline gap-2 mb-3">
              <span className="text-[15px] font-semibold tracking-tight">Time by app</span>
              <span className="ml-auto text-[12px] text-muted-foreground tabular-nums">{formatDuration(capturedMs) || '0m'} total</span>
            </h2>
            {timeByApp.length > 0 ? <AppRows data={timeByApp} format={(m) => formatDuration(m * 60000)} /> : <p className="text-xs text-muted-foreground">Nothing captured yet for this {span}.</p>}
          </section>
          <div className="grid grid-cols-2 gap-3">
            <section className="rounded-2xl border bg-card p-4 min-w-0">
              <h2 className="text-[13px] font-semibold tracking-tight mb-2">Kinds <span className="text-muted-foreground font-normal">· 30 d</span></h2>
              {kinds.length > 0
                ? <Donut data={kinds.slice(0, 4)} size={104} thickness={13} label={short(kindTotal)} sub="memories" className="flex-col items-stretch gap-3" />
                : <p className="text-xs text-muted-foreground">Nothing yet.</p>}
            </section>
            <section className="rounded-2xl border bg-card p-4 min-w-0">
              <h2 className="text-[13px] font-semibold tracking-tight mb-2">Per day <span className="text-muted-foreground font-normal">· 14 d</span></h2>
              {perDay.length > 1 ? (
                <>
                  <Bars data={perDay} height={72} format={(n) => `${n} memories`} />
                  <div className="mt-3">
                    <div className="text-[11.5px] text-muted-foreground">Average</div>
                    <div className="text-[17px] font-semibold tabular-nums leading-tight">{avgPerDay} memories</div>
                    {weekDelta != null && (
                      <div className={cn('text-[11.5px] font-medium tabular-nums mt-0.5', weekDelta >= 0 ? 'text-emerald-600 dark:text-emerald-400' : 'text-rose-600 dark:text-rose-400')}>
                        {weekDelta >= 0 ? '↑' : '↓'} {Math.abs(weekDelta)}%
                      </div>
                    )}
                  </div>
                </>
              ) : <p className="text-xs text-muted-foreground">Nothing yet.</p>}
            </section>
          </div>

          {weekDelta != null && (
            <button onClick={() => onAsk('What did I spend this week on, compared with last week?')} className="w-full rounded-2xl border bg-card p-3.5 flex items-center gap-3 text-left hover:bg-secondary/50">
              <span className="h-8 w-8 shrink-0 rounded-lg bg-secondary grid place-items-center text-primary"><Quote className="h-4 w-4" /></span>
              <span className="min-w-0 flex-1">
                <span className="block text-[13px] font-medium">
                  {weekDelta >= 5 ? "You're capturing more than usual" : weekDelta <= -5 ? 'A quieter week than the last' : "You're capturing consistently"}
                </span>
                <span className="block text-[12px] text-muted-foreground">
                  {weekDelta >= 0 ? `${weekDelta}% more` : `${Math.abs(weekDelta)}% fewer`} memories than the week before.
                </span>
              </span>
              <ChevronRight className="h-4 w-4 shrink-0 text-muted-foreground" />
            </button>
          )}

          <Pane title="Coming up" icon={CalendarClock}>
            {events.length === 0 && <p className="text-xs text-muted-foreground">Nothing in the next 24 hours, or Calendar is off in Integrations.</p>}
            <ul className="space-y-2.5">
              {events.slice(0, 5).map((e) => (
                <li key={e.id} className="text-sm min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-xs text-muted-foreground tabular-nums w-11 shrink-0">{e.allDay ? 'All day' : `${isToday(e.start) ? '' : format(e.start, 'EEE ')}${format(e.start, 'HH:mm')}`}</span>
                    <span className="truncate flex-1">{e.title}</span>
                    <Button size="sm" variant="outline" className="h-6 px-2 text-xs" onClick={() => onAsk(e.question)}>Prepare</Button>
                  </div>
                  {e.attendees.length > 0 && <div className="text-xs text-muted-foreground truncate pl-[52px]">{e.attendees.slice(0, 3).join(', ')}{e.attendees.length > 3 ? ` +${e.attendees.length - 3}` : ''}</div>}
                </li>
              ))}
            </ul>
          </Pane>

          <Pane title="Open commitments" icon={ListChecks} aside={onOpenTasks && <button className="text-xs text-muted-foreground hover:underline" onClick={onOpenTasks}>All</button>}>
            {(!stats || stats.openTasks.length === 0) && <p className="text-xs text-muted-foreground">Nothing owed from the last week.</p>}
            <ul className="space-y-2">
              {stats?.openTasks.slice(0, 6).map((t) => (
                <li key={t.id} className="text-sm flex items-start gap-2 min-w-0">
                  <input type="checkbox" className="mt-1" onChange={() => markDone(t.id)} title="Done" />
                  <span className="min-w-0">
                    <span className="block leading-snug line-clamp-2">{t.text}</span>
                    <button className="text-xs text-muted-foreground hover:underline truncate max-w-full" onClick={() => setOpenId(t.activityId)}>{t.title}</button>
                  </span>
                </li>
              ))}
            </ul>
          </Pane>

          {upcoming.length > 0 && (
            <Pane title="Dates in your memories" icon={CalendarDays}>
              <ul className="space-y-1.5">
                {upcoming.slice(0, 6).map((u) => (
                  <li key={`${u.memoryId}-${u.when}-${u.label}`} className="text-sm flex items-center gap-2 min-w-0">
                    <span className="text-xs text-muted-foreground tabular-nums w-14 shrink-0">{isToday(u.when) ? 'Today' : format(u.when, 'EEE d MMM')}</span>
                    <button className="truncate flex-1 text-left hover:underline" onClick={() => setOpenId(u.activityId)} title={`"${u.label}" · ${u.about}`}>
                      {u.about !== 'mentioned' ? `${u.about} · ` : ''}{u.title}
                    </button>
                  </li>
                ))}
              </ul>
            </Pane>
          )}

          {gaps.length > 0 && (
            <Pane title="Worth a note" icon={Lightbulb} className="border-amber-500/30">
              <ul className="space-y-2">
                {gaps.map((g, i) => (
                  <li key={i} className="text-sm">
                    <p className="leading-snug">{g.text}</p>
                    {g.question && <Button size="sm" variant="outline" className="mt-1 h-6 px-2 text-xs" onClick={() => onAsk(g.question)}>Recall</Button>}
                  </li>
                ))}
              </ul>
            </Pane>
          )}
        </div>
      </div>

      <div className="mt-6 pt-4 border-t flex items-center gap-3 text-[12px] text-muted-foreground flex-wrap">
        <span className="font-semibold text-foreground">Lane</span>
        <span>A more thoughtful you.</span>
        <span className="ml-auto flex items-center gap-1.5"><Lock className="h-3.5 w-3.5" /> Private by design. Everything stays on your Mac.</span>
      </div>

      <ActivitySheet id={openId} onClose={() => setOpenId(null)} onDeleted={() => load(true)} />
    </div>
  )
}
