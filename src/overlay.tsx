import { useEffect, useRef, useState } from 'react'
import { CornerDownLeft, Loader2, Search, MonitorUp, ListChecks } from 'lucide-react'
import { SourceIcon } from '@/components/source-icon'
import { api, type ScreenSummary, formatDuration, hostOf, type AskResult, type Entity, type FileHit, type MemoryCard, type Task } from '@/lib/api'
import { cn } from '@/lib/utils'
import './styles/globals.css'

/**
 * The recall panel: summoned anywhere with the global hotkey. Type to see
 * matching memories at once; press Enter to get an answer. Esc or clicking
 * away hides it. The window is marked protected, so it never appears in
 * screen shares or recordings.
 */
export default function Overlay() {
  const [query, setQuery] = useState('')
  const [hits, setHits] = useState<MemoryCard[]>([])
  const [files, setFiles] = useState<FileHit[]>([])
  const [people, setPeople] = useState<Entity[]>([])
  const [tasks, setTasks] = useState<Task[]>([])
  const [cursor, setCursor] = useState(-1)
  const [answer, setAnswer] = useState<string | null>(null)
  const [result, setResult] = useState<AskResult | null>(null)
  const [busy, setBusy] = useState(false)
  const [screen, setScreen] = useState<ScreenSummary | null>(null)
  const [useScreen, setUseScreen] = useState(true)
  const askId = useRef<number | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const [group, setGroup] = useState<'all' | 'memories' | 'people' | 'tasks' | 'files'>('all')

  useEffect(() => {
    const reset = () => {
      setQuery('')
      setHits([])
      setFiles([])
      setPeople([])
      setTasks([])
      setCursor(-1)
      setAnswer(null)
      setResult(null)
      setBusy(false)
      setUseScreen(true)
      api.screenContext().then(setScreen).catch(() => setScreen(null))
      setTimeout(() => inputRef.current?.focus(), 30)
    }
    reset()
    const a = api.onOverlayShown(reset)
    const b = api.onAskToken(({ id, token }) => {
      if (id === askId.current) setAnswer((prev) => (prev ?? '') + token)
    })
    const c = api.onAskDone((r) => {
      if (r.id !== askId.current) return
      setBusy(false)
      setResult(r)
      if (r.error) setAnswer(r.error)
      else setAnswer(r.answer)
    })
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') api.hideOverlay()
    }
    window.addEventListener('keydown', onKey)
    return () => {
      a.then((f) => f())
      b.then((f) => f())
      c.then((f) => f())
      window.removeEventListener('keydown', onKey)
    }
  }, [])

  useEffect(() => {
    const q = query.trim()
    if (!q) {
      setHits([])
      setFiles([])
      setPeople([])
      setTasks([])
      setCursor(-1)
      return
    }
    const t = setTimeout(() => {
      api.listMemories(q, true, 5).then(setHits).catch(() => {})
      api.searchFiles(q, 3).then(setFiles).catch(() => {})
      api.listEntities('person', q, 3).then(setPeople).catch(() => {})
      const words = q.toLowerCase().split(/\s+/).filter((w) => w.length > 2)
      if (words.length) api.listTasks('open', 200).then((all) => setTasks(all.filter((t) => words.every((w) => t.text.toLowerCase().includes(w))).slice(0, 3))).catch(() => {})
      else setTasks([])
    }, 200)
    return () => clearTimeout(t)
  }, [query])

  // Everything in the list, in order, for the arrow keys: ↑↓ move, ⌘↩ opens.
  const rows: { key: string; open: () => void }[] = [
    ...people.map((p) => ({ key: `p${p.id}`, open: () => api.openMain('people') })),
    ...(result?.sources.length ? result.sources.flatMap((s) => (s.card ? [s.card] : [])) : hits).map((m) => ({ key: `m${m.id}`, open: () => api.openMain('memories', m.activityId) })),
    ...tasks.map((t) => ({ key: `t${t.id}`, open: () => api.openMain('memories', t.activityId) })),
    ...(result?.sources.length ? result.sources.flatMap((s) => (s.file ? [s.file] : [])) : files).map((f) => ({ key: `f${f.fileId}`, open: () => api.openFile(f.path) })),
  ]
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'ArrowDown') { e.preventDefault(); setCursor((c) => Math.min(rows.length - 1, c + 1)) }
      else if (e.key === 'ArrowUp') { e.preventDefault(); setCursor((c) => Math.max(-1, c - 1)) }
      else if (e.key === 'Enter' && e.metaKey && cursor >= 0 && rows[cursor]) { e.preventDefault(); rows[cursor].open(); api.hideOverlay() }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [rows.length, cursor]) // eslint-disable-line react-hooks/exhaustive-deps
  const rowClass = (key: string, base: string) => cn(base, rows[cursor]?.key === key && 'is-sel')

  async function submit(e: React.FormEvent) {
    e.preventDefault()
    const q = query.trim()
    if (!q || busy) return
    setBusy(true)
    setAnswer('')
    setResult(null)
    askId.current = screen && useScreen ? await api.askScreen(q) : (await api.ask(q, [], 'answer', null, false)).id
  }

  async function quick(q: string) {
    if (busy) return
    setQuery(q)
    setBusy(true)
    setAnswer('')
    setResult(null)
    askId.current = await api.askScreen(q)
  }

  const shown = result?.sources.length ? result.sources.flatMap((s) => (s.card ? [s.card] : [])) : hits
  const shownFiles = result?.sources.length ? result.sources.flatMap((s) => (s.file ? [s.file] : [])) : files
  const counts = { memories: shown.length, people: people.length, tasks: tasks.length, files: shownFiles.length }
  const show = (g: typeof group) => group === 'all' || group === g

  return (
    <div className="enterprise-shell h-screen p-2 select-none">
      <div className="sp-card h-full flex flex-col overflow-hidden">
        <form onSubmit={submit} className="sp-search">
          {busy ? <Loader2 className="h-[22px] w-[22px] animate-spin text-muted-foreground" /> : <Search className="h-[22px] w-[22px] text-muted-foreground" />}
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={screen && useScreen ? 'Ask about this window, or recall anything…' : 'Recall anything, then ↩ to ask'}
            autoFocus
          />
          <span className="sp-hint">
            <CornerDownLeft className="h-3 w-3" /> ask · ↑↓ ⌘↩ open · esc
          </span>
        </form>

        {(counts.memories > 0 || counts.people > 0 || counts.tasks > 0 || counts.files > 0) && (
          <div className="sp-chips">
            {([['all', 'Everything', counts.memories + counts.people + counts.tasks + counts.files],
               ['memories', 'Memories', counts.memories],
               ['people', 'People', counts.people],
               ['tasks', 'Owed', counts.tasks],
               ['files', 'Files', counts.files]] as const)
              .filter(([id, , n]) => id === 'all' || n > 0)
              .map(([id, label, n]) => (
                <button key={id} type="button" onClick={() => setGroup(id)} className={cn('sp-chip', group === id && 'is-on')}>
                  {label} {id !== 'all' && <span className="n">{n}</span>}
                </button>
              ))}
          </div>
        )}

        {screen && (
          <div className="flex items-center gap-2 px-4 py-2 border-b text-xs">
            <button
              onClick={() => setUseScreen((v) => !v)}
              className={cn('inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1', useScreen ? 'bg-primary text-primary-foreground border-primary' : 'text-muted-foreground')}
              title={useScreen ? 'Questions are about this window. Click to ask your memories instead.' : 'Click to ask about this window.'}
            >
              <MonitorUp className="h-3 w-3" />
              {useScreen ? 'About this window' : 'Include this window'}: <span className="truncate max-w-[220px]">{screen.windowTitle || screen.appName}</span>
            </button>
            {useScreen && answer === null && (
              <div className="ml-auto flex gap-1">
                {['Summarise this', 'What should I do about this?', 'Draft a reply'].map((q) => (
                  <button key={q} onClick={() => quick(q)} className="rounded-full border px-2.5 py-1 text-muted-foreground hover:text-foreground hover:bg-accent">{q}</button>
                ))}
              </div>
            )}
          </div>
        )}
        <div className="flex-1 overflow-y-auto">
          {answer !== null && (
            <div className={cn('px-5 py-4 text-[15px] leading-relaxed whitespace-pre-wrap select-text border-b', result?.error && 'text-destructive')}>
              {answer || <span className="text-muted-foreground">Thinking…</span>}
            </div>
          )}
          {people.length > 0 && answer === null && show('people') && (
            <ul className="sp-list">
              {people.map((p) => (
                <li key={p.id}>
                  <button onClick={() => { api.openMain('people'); api.hideOverlay() }} className={rowClass(`p${p.id}`, 'sp-row')}>
                    <span className="sp-ic sp-ic-person">{p.name.charAt(0)}</span>
                    <span className="sp-t"><b>{p.name}</b><i>Person</i></span>
                    <span className="sp-w">{p.mentions} memories</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          {shown.length > 0 && show('memories') && (
            <ul className="sp-list">
              {shown.map((m, i) => {
                const host = hostOf(m.url)
                const n = result?.sources[i]?.n
                return (
                  <li key={m.id}>
                    <button
                      onClick={() => api.openMain('memories', m.activityId)}
                      className={rowClass(`m${m.id}`, 'sp-row')}
                    >
                      <SourceIcon app={m.appName} url={m.url} size={26} className="sp-ic-img" />
                      <span className="sp-t">
                        <b>{n ? `[${n}] ` : ''}{m.title}</b>
                        <i>{host || m.appName} · {m.summary}</i>
                      </span>
                      <span className="sp-w">{formatDuration(m.totalMs)}</span>
                    </button>
                  </li>
                )
              })}
            </ul>
          )}
          {tasks.length > 0 && answer === null && show('tasks') && (
            <ul className="sp-list">
              {tasks.map((t) => (
                <li key={t.id}>
                  <button onClick={() => { api.openMain('memories', t.activityId); api.hideOverlay() }} className={rowClass(`t${t.id}`, 'sp-row')}>
                    <span className="sp-ic sp-ic-task"><ListChecks className="h-3.5 w-3.5" /></span>
                    <span className="sp-t"><b>{t.text}</b><i>Owed · from {t.title}</i></span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          {shownFiles.length > 0 && show('files') && (
            <ul className="sp-list">
              {shownFiles.map((f) => (
                <li key={f.fileId}>
                  <button onClick={() => api.openFile(f.path)} className={rowClass(`f${f.fileId}`, 'sp-row')}>
                    <span className="sp-ic sp-ic-file">{(f.ext || 'doc').slice(0, 4)}</span>
                    <span className="sp-t"><b>{f.name}</b><i>{f.path.replace(/^\/Users\/[^/]+/, '~')}{f.snippet ? ` · ${f.snippet}` : ''}</i></span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          {!query && answer === null && (
            <p className="px-5 py-6 text-[13px] text-muted-foreground">
              Everything you've seen, worked on, or have on this Mac: memories, people, what you owe, documents. Type to find, Enter to ask. Invisible in screen shares.
            </p>
          )}
        </div>
      </div>
    </div>
  )
}
