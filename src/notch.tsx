import { useEffect, useRef, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { ArrowUpRight, AudioLines, CalendarClock, ChevronRight, ExternalLink, FileText, History, Leaf, ListChecks, Mic, Send, Settings as SettingsIcon, Square, Sparkles, Target, User, Zap } from 'lucide-react'
import { format, isToday } from 'date-fns'
import { api, formatDuration, type CalendarEvent, type DayStats, type MemoryCard, type NotchContext, type RecordingReport, type Signal, type Status, type Task } from '@/lib/api'
import { cn } from '@/lib/utils'

/** What the engine pushes to the notch. */
export interface NotchState {
  kind: 'idle' | 'recording' | 'reminder' | 'brief' | 'help' | 'dictation'
  title: string
  lines: string[]
  at: number
}

type Position = 'top-center' | 'top-left' | 'top-right' | 'bottom-center' | 'left' | 'right'

// The tab: a sliver at the screen edge. The card: what opens on hover.
const CARD = { w: 760, h: 470 }
const tabSize = (p: Position) => (p === 'left' || p === 'right' ? { w: 14, h: 180 } : { w: 180, h: 14 })

const IDLE: NotchState = { kind: 'idle', title: '', lines: [], at: 0 }

/** Four things worth a single click, each one a real screen or a real question. */
const QUICK: { label: string; icon: typeof CalendarClock; tone: string; run: (ask: (q: string) => void) => void }[] = [
  { label: 'What is coming up?', icon: CalendarClock, tone: 'ic-violet', run: () => api.openMain('today') },
  { label: 'Show recent files', icon: FileText, tone: 'ic-blue', run: () => api.openMain('files') },
  { label: 'What do I owe people?', icon: ListChecks, tone: 'ic-amber', run: (ask) => ask('What do I owe people right now?') },
  { label: 'Summarise my day', icon: User, tone: 'ic-green', run: (ask) => ask('Summarise my day so far.') },
]

/** The Lane mark: the wave, which is the logo. (The rabbit is Rabbit, the
 *  model inside; it is not the product's mark.) */
function LaneMark({ className }: { className?: string }) {
  return (
    <span className={cn('nc-mark', className)}>
      <img src="/white.png" alt="" draggable={false} />
    </span>
  )
}

function ago(ms: number) {
  const d = Date.now() - ms
  if (d < 3_600_000) return `${Math.max(1, Math.round(d / 60_000))} min ago`
  if (d < 86_400_000) return `${Math.round(d / 3_600_000)} h ago`
  const days = Math.round(d / 86_400_000)
  return days === 1 ? 'yesterday' : `${days} days ago`
}

/**
 * A small tab pinned to a screen edge at all times. Hover: a card with
 * what is happening now, the next event, open commitments, what Lane
 * knows about the window in front, and a line to ask memory. Reminders,
 * briefs and meeting help open the card by themselves for a while. Never
 * in a screen share (content protection is set by the shell).
 */
export default function Notch() {
  const [state, setState] = useState<NotchState>(IDLE)
  const [rec, setRec] = useState<RecordingReport | null>(null)
  const [status, setStatus] = useState<Status | null>(null)
  const [events, setEvents] = useState<CalendarEvent[]>([])
  const [tasks, setTasks] = useState<Task[]>([])
  const [three, setThree] = useState<Signal[]>([])
  const [ctx, setCtx] = useState<NotchContext | null>(null)
  const [recent, setRecent] = useState<MemoryCard[]>([])
  const [day, setDay] = useState<DayStats | null>(null)
  const [streak, setStreak] = useState(0)
  const [position, setPosition] = useState<Position>('top-center')
  const [hover, setHover] = useState(false)
  const [pinned, setPinned] = useState(false) // opened by an event, closes on its own
  const [held, setHeld] = useState(false) // clicked open: stays until clicked or closed
  const [grown, setGrown] = useState(false) // the window has the card's size
  const [question, setQuestion] = useState('')
  const [answer, setAnswer] = useState<string | null>(null)
  const [asking, setAsking] = useState(false)
  const askId = useRef<number | null>(null)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const leave = useRef<ReturnType<typeof setTimeout> | null>(null)
  const openedAt = useRef(0)

  const open = hover || pinned || held
  // Open is "the window should be big"; shown is "the card may be seen".
  const shown = open && grown
  const topApps: [string, number][] = (day?.timeByApp ?? []).slice(0, 4)
  const capturedMin = (day?.timeByApp ?? []).reduce((a, d) => a + d[1], 0)
  const ask2 = (q: string) => { setQuestion(q); askNow(q) }
  const vertical = position === 'left' || position === 'right'

  function enter() {
    if (leave.current) clearTimeout(leave.current)
    if (!hover) openedAt.current = Date.now()
    setHover(true)
  }
  function exit() {
    if (leave.current) clearTimeout(leave.current)
    const wait = Date.now() - openedAt.current < 600 ? 600 : 220
    leave.current = setTimeout(() => setHover(false), wait)
  }
  function closeCard() {
    if (leave.current) clearTimeout(leave.current)
    if (timer.current) clearTimeout(timer.current)
    setHover(false)
    setPinned(false)
    setHeld(false)
  }

  useEffect(() => {
    api.getSettings().then((s) => setPosition(s.notchPosition)).catch(() => {})
    const a = listen<NotchState>('notch-update', (e) => setState(e.payload))
    // The shell watches the pointer against the window frame; the webview's
    // own mouseleave is unreliable once the card has grown under the pointer.
    const b = listen<{ inside: boolean }>('notch-mouse', (e) => {
      if (e.payload.inside) enter()
      else { if (leave.current) clearTimeout(leave.current); setHover(false) }
    })
    const c = api.onNotchPosition((p) => setPosition(p as Position))
    const d = api.onAskToken(({ id, token }) => { if (id === askId.current) setAnswer((prev) => (prev ?? '') + token) })
    const e = api.onAskDone((r) => {
      if (r.id !== askId.current) return
      setAsking(false)
      setAnswer(r.error ? r.error : r.answer)
    })
    const tick = () => api.recordingStatus().then(setRec).catch(() => {})
    tick()
    const t = setInterval(tick, 3000)
    return () => {
      a.then((f) => f()); b.then((f) => f()); c.then((f) => f()); d.then((f) => f()); e.then((f) => f())
      clearInterval(t)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Card contents refresh while it is open and once a minute otherwise; the
  // window-in-front context is read each time the card opens.
  useEffect(() => {
    const load = () => {
      api.status().then(setStatus).catch(() => {})
      api.upcomingEvents(12).then(setEvents).catch(() => setEvents([]))
      api.listTasks('open', 20).then(setTasks).catch(() => {})
      api.signals().then((r) => setThree(r.signals.filter((s) => s.state !== 'noise').slice(0, 3))).catch(() => setThree([]))
      api.listMemories(undefined, true, 3).then(setRecent).catch(() => setRecent([]))
      const start = new Date(); start.setHours(0, 0, 0, 0)
      api.dayStats(start.getTime()).then(setDay).catch(() => {})
      api.explore()
        .then((e) => {
          // Days in a row, counting back from the most recent day with anything in it.
          let n = 0
          for (const [, count] of [...e.memoriesPerDay].reverse()) { if (count > 0) n++; else break }
          setStreak(n)
        })
        .catch(() => {})
    }
    load()
    // The position is re-read on every open as well as on the change event,
    // so a missed event can never leave the tab in the wrong orientation.
    api.getSettings().then((s) => setPosition(s.notchPosition)).catch(() => {})
    if (open) api.notchContext().then(setCtx).catch(() => setCtx(null))
    const t = setInterval(load, open ? 15_000 : 60_000)
    return () => clearInterval(t)
  }, [open])

  // The window has to be the card's size before the card is drawn, or the
  // unfurl would be clipped by a 14pt-tall window. So: resize, wait for the
  // frame that carries the new size, then let it open.
  useEffect(() => {
    let alive = true
    const s = open ? CARD : tabSize(position)
    if (!open) setGrown(false)
    api.notchResize(s.w, s.h)
      .then(() => {
        if (!alive || !open) return
        requestAnimationFrame(() => requestAnimationFrame(() => { if (alive) setGrown(true) }))
      })
      .catch(() => { if (alive && open) setGrown(true) })
    return () => { alive = false }
  }, [open, position])

  useEffect(() => {
    if (timer.current) clearTimeout(timer.current)
    if (state.kind === 'dictation') {
      setPinned(true)
      return
    }
    if (state.kind === 'reminder' || state.kind === 'brief' || state.kind === 'help') {
      setPinned(true)
      timer.current = setTimeout(() => setPinned(false), state.kind === 'help' ? 25_000 : 15_000)
    }
    return () => { if (timer.current) clearTimeout(timer.current) }
  }, [state])

  useEffect(() => {
    if (!rec?.dictating && state.kind === 'dictation') setPinned(false)
  }, [rec?.dictating, state.kind])

  async function askNow(text: string) {
    const q = text.trim()
    if (!q || asking) return
    setAsking(true)
    setAnswer('')
    setHeld(true)
    try {
      const start = await api.ask(q, [], 'answer', null, false)
      askId.current = start.id
    } catch (err) {
      setAsking(false)
      setAnswer(String(err))
    }
  }

  function ask(e: React.FormEvent) {
    e.preventDefault()
    askNow(question)
  }

  const recording = !!rec?.recording
  const dictating = !!rec?.dictating
  const cap = status?.capture
  const capturing = !!cap && cap.trusted && !cap.idle && !cap.excluded && !status?.paused
  const tone = dictating ? 'bg-sky-400' : recording ? 'bg-red-500 animate-pulse-soft' : capturing ? 'bg-emerald-400' : 'bg-white/40'
  const next = events.find((e) => e.start > Date.now() - 5 * 60_000)
  const nowLine = dictating
    ? 'Listening · press ⌥⇧Space to insert'
    : recording
      ? `Recording${rec?.micSeconds ? ` · ${Math.floor(rec.micSeconds / 60)}:${String(rec.micSeconds % 60).padStart(2, '0')}` : ''}`
      : status?.paused
        ? 'Capture paused'
        : capturing
          ? `Remembering ${cap?.currentApp ?? ''}`.trim()
          : cap && !cap.trusted
            ? 'Needs Accessibility'
            : 'Idle'

  // The third tile: an answer being written, the latest word from Lane,
  // the live transcript, or what Lane knows about the window in front.
  const fromLane = (() => {
    if (answer !== null) return { title: 'Answer', body: <p className="text-[12px] leading-snug line-clamp-5 whitespace-pre-wrap">{answer || 'Thinking…'}</p> }
    if (state.kind !== 'idle' && state.lines.length > 0) return { title: state.title, body: <ul className="space-y-0.5">{state.lines.slice(0, 3).map((l, i) => <li key={i} className="text-[12px] leading-snug line-clamp-2">{l}</li>)}</ul> }
    if (recording && rec) return { title: 'Live', body: <p className="text-[12px] text-white/70 line-clamp-4 leading-snug">{rec.liveText.slice(-240) || 'Listening…'}</p> }
    if (ctx?.person) {
      const p = ctx.person
      return {
        title: p.name, icon: User,
        body: (
          <div className="text-[12px] leading-snug space-y-0.5">
            {p.owed.slice(0, 2).map((o, i) => <p key={i} className="line-clamp-2">You owe: {o}</p>)}
            {p.lastTitle && <p className="text-white/60 line-clamp-2">Last{p.lastAt ? ` ${ago(p.lastAt)}` : ''}: {p.lastTitle}</p>}
            {p.owed.length === 0 && !p.lastTitle && <p className="text-white/60">Known, nothing open.</p>}
          </div>
        ),
      }
    }
    if (ctx?.last) {
      return {
        title: `Last time here · ${ago(ctx.last.at)}`, icon: History,
        body: (
          <div>
            <button className="text-left text-[12px] leading-snug line-clamp-3" onClick={() => api.openMain('memories', ctx.last!.activityId)}>{ctx.last.summary || ctx.last.title}</button>
            {ctx.connects.length > 0 && <p className="text-[11px] text-white/55 mt-1 truncate">Connects to {ctx.connects.map(([n]) => n).join(', ')}</p>}
          </div>
        ),
      }
    }
    if (ctx && ctx.connects.length > 0) {
      return {
        title: 'Connects to', icon: History,
        body: <ul className="text-[12px] leading-snug space-y-0.5">{ctx.connects.map(([n, w]) => <li key={n}>{n} <span className="text-white/45">· {w} together</span></li>)}</ul>,
      }
    }
    return { title: 'Lane', body: <p className="text-[12px] text-white/55 leading-snug">Reminders, your brief and live meeting help land here. Ask below.</p> }
  })()
  const FromIcon = fromLane.icon ?? Sparkles

  return (
    <div
      className={cn('notch-root select-none', `pos-${position}`, shown && 'is-open')}
      onMouseEnter={enter}
      onMouseLeave={exit}
    >
     <div className={cn('notch-surface', shown && 'is-open')}>
      {/* The tab: a dot says what Lane is doing. Click holds the card open; × closes. */}
      <div className={cn('notch-tab', vertical && !shown && 'is-vertical')} onClick={() => (open ? closeCard() : setHeld(true))} title={open ? 'Click to close' : 'Click to keep open'}>
        <span className={cn('notch-dot', tone)} />
        {shown && <LaneMark className="tab" />}
        {shown && <span className="text-[10px] tracking-wide text-white/60 uppercase">Lane</span>}
        {shown && <button className="notch-x" title="Close" onClick={(e) => { e.stopPropagation(); closeCard() }}>×</button>}
      </div>

      {open && (
        <div className="notch-body">
          {/* Who this is and the way out */}
          <div className="nc-head">
            <LaneMark />
            <span className="nc-name">Lane</span>
            <span className="nc-tag">{nowLine}</span>
            <span className="nc-count">{status ? `${status.stats.activitiesToday} today` : ''}</span>
            <button className="nc-icon" title="Settings" onClick={() => api.openMain('settings')}><SettingsIcon className="h-3.5 w-3.5" /></button>
            <button className="nc-open" onClick={() => api.openMain('today')}>Open app <ArrowUpRight className="h-3.5 w-3.5" /></button>
          </div>

          {/* Ask */}
          <form onSubmit={ask} className="nc-ask">
            <Sparkles className="h-4 w-4 shrink-0 nc-spark" />
            <input
              value={question}
              onChange={(e) => setQuestion(e.target.value)}
              placeholder={asking ? 'Answering…' : 'Ask Lane anything…'}
              disabled={asking}
              onKeyDown={(e) => { if (e.key === 'Escape') { setQuestion(''); setAnswer(null) } }}
            />
            <span className="nc-kbd">↩</span>
            <button type="submit" className="nc-mic" disabled={asking || !question.trim()} title="Ask"><Send className="h-3.5 w-3.5" /></button>
          </form>

          {answer && (
            <div className="nc-answer">{answer}</div>
          )}

          {!answer && (
            <>
              {/* Three things you might do right now */}
              <div className="nc-acts">
                {recording ? (
                  <button className="nc-act is-rec" onClick={() => api.stopMeeting().then(() => api.recordingStatus().then(setRec))}>
                    <span className="ic"><Square className="h-4 w-4" /></span>
                    <span><b>Stop recording</b><i>{rec?.detail || 'Saving when you stop'}</i></span>
                  </button>
                ) : (
                  <button className="nc-act" disabled={dictating} onClick={() => api.startMeeting('').then(() => api.recordingStatus().then(setRec)).catch(() => {})}>
                    <span className="ic ic-rose"><Mic className="h-4 w-4" /></span>
                    <span><b>Record</b><i>This call or the room</i></span>
                  </button>
                )}
                <button className={cn('nc-act', dictating && 'is-live')} disabled={recording} onClick={() => api.dictationToggle().catch(() => {})}>
                  <span className="ic ic-violet"><AudioLines className="h-4 w-4" /></span>
                  <span><b>{dictating ? 'Insert' : 'Dictate'}</b><i>{dictating ? 'Listening, ⌥⇧Space to place' : 'Hands-free input'}</i></span>
                </button>
                <button className="nc-act" onClick={() => api.openMain('memories')}>
                  <span className="ic ic-blue"><ExternalLink className="h-4 w-4" /></span>
                  <span><b>Open Lane</b><i>Go to your memory</i></span>
                </button>
              </div>

              {/* The three panels */}
              <div className="nc-grid">
                <section className="nc-panel">
                  <header onClick={() => api.openMain('memories')}><History className="h-3.5 w-3.5" /> Recent memories <ChevronRight className="h-3.5 w-3.5 ml-auto" /></header>
                  {recent.length === 0 && <p className="nc-empty">Nothing yet today.</p>}
                  {recent.map((m) => (
                    <button key={m.id} className="nc-row" onClick={() => api.openMain('memories')}>
                      <span className="nc-row-ic"><FileText className="h-3.5 w-3.5" /></span>
                      <span className="nc-row-t">
                        <b>{m.title}</b>
                        <i>{m.appName}</i>
                      </span>
                      <span className="nc-row-w">{isToday(m.startedAt) ? format(m.startedAt, 'HH:mm') : format(m.startedAt, 'EEE')}</span>
                    </button>
                  ))}
                </section>

                <section className="nc-panel">
                  <header onClick={() => api.openMain('today')}><Target className="h-3.5 w-3.5" /> Today <ChevronRight className="h-3.5 w-3.5 ml-auto" /></header>
                  <div className="nc-today">
                    <div className="nc-ring" style={{ ['--p' as string]: `${Math.min(100, Math.round((capturedMin / 480) * 100))}%` }}>
                      <b>{formatDuration(capturedMin * 60000) || '0m'}</b>
                      <i>captured</i>
                    </div>
                    <ul className="nc-legend">
                      {topApps.map(([name, min], i) => (
                        <li key={name}><span className="d" data-i={i} /> <span className="n">{name}</span> <span className="v">{formatDuration(min * 60000)}</span></li>
                      ))}
                      {topApps.length === 0 && <li className="nc-empty">Nothing captured yet.</li>}
                    </ul>
                  </div>
                  <button className="nc-streak" onClick={() => api.openMain('explore')}>
                    <span className="ic"><Leaf className="h-3.5 w-3.5" /></span>
                    <span><b>{streak > 1 ? `${streak} days in a row` : 'A fresh start'}</b><i>{three.length > 0 ? `${three.length} things worth doing` : 'Nothing pressing'}</i></span>
                    <ChevronRight className="h-3.5 w-3.5 ml-auto" />
                  </button>
                </section>

                <section className="nc-panel">
                  <header><Zap className="h-3.5 w-3.5" /> Quick actions</header>
                  {QUICK.map((a) => (
                    <button key={a.label} className="nc-quick" onClick={() => a.run(ask2)}>
                      <span className={cn('ic', a.tone)}><a.icon className="h-3.5 w-3.5" /></span>
                      {a.label}
                      <ChevronRight className="h-3.5 w-3.5 ml-auto" />
                    </button>
                  ))}
                </section>
              </div>
            </>
          )}

          <div className="nc-foot">
            <LaneMark className="sm" />
            Lane — remember more, look for less.
            <span className="ml-auto nc-kbd">⌥Space anywhere</span>
          </div>
        </div>
      )}
     </div>
    </div>
  )
}
