import { useCallback, useEffect, useRef, useState } from 'react'
import { Loader2, MessageSquare, Plus, Send, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { ActivitySheet } from '@/components/activity-sheet'
import { Card } from '@/pages/memories'
import { format, isToday, isYesterday } from 'date-fns'
import { FileRow } from '@/components/file-row'
import { api, type AskResult, type AskSource, type Conversation } from '@/lib/api'
import { cn } from '@/lib/utils'

interface Turn {
  id: number
  question: string
  answer: string
  result: AskResult | null
}

function when(ms: number) {
  if (isToday(ms)) return format(ms, 'HH:mm')
  if (isYesterday(ms)) return 'Yesterday'
  return format(ms, 'd MMM')
}

/** Ask: a chat over your memories. Every conversation is kept, like any chat app, on this Mac. */
export function AskPage({ initialQuestion, onConsumed }: { initialQuestion?: string | null; onConsumed?: () => void }) {
  const [question, setQuestion] = useState('')
  const [turns, setTurns] = useState<Turn[]>([])
  const [busy, setBusy] = useState(false)
  const [mode, setMode] = useState<'answer' | 'draft'>('answer')
  const [openId, setOpenId] = useState<number | null>(null)
  const [chats, setChats] = useState<Conversation[]>([])
  const [current, setCurrent] = useState<number | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const bottomRef = useRef<HTMLDivElement>(null)
  const seq = useRef(-1)

  const refreshChats = useCallback(() => api.listConversations(60).then(setChats).catch(() => {}), [])

  useEffect(() => {
    inputRef.current?.focus()
    refreshChats()
  }, [refreshChats])

  useEffect(() => {
    const a = api.onAskToken(({ id, token }) =>
      setTurns((prev) => prev.map((t) => (t.id === id ? { ...t, answer: t.answer + token } : t))),
    )
    const b = api.onAskDone((r) => {
      setTurns((prev) => prev.map((t) => (t.id === r.id ? { ...t, answer: r.error ? t.answer : r.answer, result: r } : t)))
      setBusy(false)
      refreshChats()
    })
    const c = api.onAskFollowups(({ id, followups }) => {
      setTurns((prev) => prev.map((t) => (t.id === id && t.result ? { ...t, result: { ...t.result, followups } } : t)))
    })
    return () => {
      a.then((fn) => fn())
      b.then((fn) => fn())
      c.then((fn) => fn())
    }
  }, [refreshChats])

  useEffect(() => bottomRef.current?.scrollIntoView({ behavior: 'smooth' }), [turns])

  // A question handed over from another page (Today, People, the board).
  useEffect(() => {
    if (initialQuestion && !busy) {
      onConsumed?.()
      startNew()
      ask(initialQuestion, null)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialQuestion])

  function startNew() {
    setCurrent(null)
    setTurns([])
    setTimeout(() => inputRef.current?.focus(), 30)
  }

  async function open(c: Conversation) {
    if (busy) return
    try {
      const msgs = await api.conversationMessages(c.id)
      const out: Turn[] = []
      let pending: string | null = null
      for (const m of msgs) {
        if (m.role === 'user') {
          if (pending) out.push({ id: seq.current--, question: pending, answer: '', result: null })
          pending = m.content
        } else if (pending) {
          let sources: AskSource[] = []
          try { sources = JSON.parse(m.sources) } catch { /* old rows */ }
          out.push({ id: seq.current--, question: pending, answer: m.content, result: { id: 0, answer: m.content, sources, error: null, followups: [], unverified: [], checked: false } })
          pending = null
        }
      }
      if (pending) out.push({ id: seq.current--, question: pending, answer: '', result: { id: 0, answer: '', sources: [], error: 'No answer was saved for this question.', followups: [], unverified: [], checked: false } })
      setCurrent(c.id)
      setTurns(out)
    } catch (e) {
      toast.error(String(e))
    }
  }

  async function remove(c: Conversation) {
    await api.deleteConversation(c.id).catch((e) => toast.error(String(e)))
    if (current === c.id) startNew()
    refreshChats()
  }

  async function ask(q: string, conversation: number | null = current) {
    if (!q.trim() || busy) return
    setBusy(true)
    setQuestion('')
    const history = turns.filter((t) => t.result && !t.result.error).map((t) => ({ question: t.question, answer: t.answer }))
    try {
      const start = await api.ask(q, history, mode, conversation)
      if (start.conversationId != null) setCurrent(start.conversationId)
      setTurns((prev) => [...prev, { id: start.id, question: q, answer: '', result: null }])
      refreshChats()
    } catch (e) {
      setBusy(false)
      toast.error(String(e))
    }
  }

  async function submit(e: React.FormEvent) {
    e.preventDefault()
    ask(question.trim())
  }

  return (
    <div className="grid grid-cols-[262px_minmax(0,1fr)] h-full">
      {/* Recent chats */}
      <aside className="border-r p-3 flex flex-col min-h-0 gap-3">
        <button onClick={startNew} className="w-full h-11 rounded-xl border bg-card flex items-center gap-2.5 px-3.5 text-[14px] font-medium hover:bg-secondary">
          <Plus className="h-4 w-4 text-primary" /> New chat
        </button>
        <div>
          <h2 className="text-[10.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground px-1 mb-1.5">Recent</h2>
          <ul className="space-y-1 overflow-y-auto min-h-0">
            {chats.map((c) => (
              <li key={c.id} className="group">
                <button onClick={() => open(c)} className={cn('w-full text-left rounded-xl px-2.5 py-2.5 flex items-start gap-2.5', current === c.id ? 'bg-accent/60' : 'hover:bg-secondary')}>
                  <span className={cn('h-8 w-8 shrink-0 rounded-lg grid place-items-center', current === c.id ? 'bg-background text-primary' : 'bg-secondary text-muted-foreground')}>
                    <MessageSquare className="h-4 w-4" />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className={cn('block truncate text-[13.5px]', current === c.id && 'font-medium')}>{c.title}</span>
                    <span className="block text-[11.5px] text-muted-foreground">{when(c.updatedAt)} · {Math.floor(c.messages / 2)} {Math.floor(c.messages / 2) === 1 ? 'question' : 'questions'}</span>
                  </span>
                  <span role="button" title="Delete" className="opacity-0 group-hover:opacity-100 text-muted-foreground hover:text-destructive mt-1" onClick={(e) => { e.stopPropagation(); remove(c) }}>
                    <Trash2 className="h-3.5 w-3.5" />
                  </span>
                </button>
              </li>
            ))}
            {chats.length === 0 && <li className="text-xs text-muted-foreground px-2">Your questions will be kept here.</li>}
          </ul>
        </div>
      </aside>

      {/* Conversation */}
      <div className="px-6 py-5 flex flex-col min-h-0 overflow-y-auto">
        <div className="flex items-center gap-4 mb-2 flex-wrap">
          <h1 className="text-[32px] font-semibold tracking-[-0.03em] leading-none">Ask</h1>
          <span className="flex rounded-full bg-secondary p-1 text-[13px]">
            {(['answer', 'draft'] as const).map((m) => (
              <button key={m} onClick={() => setMode(m)} className={cn('rounded-full px-4 py-1.5 transition-colors', mode === m ? 'bg-primary text-primary-foreground font-medium shadow-sm' : 'text-muted-foreground hover:text-foreground')}>
                {m === 'answer' ? 'Answer' : 'Write a draft'}
              </button>
            ))}
          </span>
        </div>
        <p className="text-[13.5px] text-muted-foreground mb-5">
          Answers come only from your memories, with the ones used shown underneath. Follow-ups work. Everything runs and stays on this Mac.
        </p>

        {turns.length === 0 && (
          <div className="rounded-3xl border bg-gradient-to-b from-secondary/50 to-background px-6 py-14 text-center">
            <span className="mx-auto h-[72px] w-[72px] rounded-full bg-accent/70 grid place-items-center text-primary">
              <MessageSquare className="h-7 w-7" />
            </span>
            <h2 className="mt-5 text-[21px] font-semibold tracking-tight">Ask about anything you've worked on</h2>
            <div className="mt-5 flex flex-wrap justify-center gap-2.5 max-w-xl mx-auto">
              {['What was the Vatsalya budget?', 'Which domains are expiring?', 'What do I owe Sarah?', 'What happened with the tender?'].map((q) => (
                <button key={q} disabled={busy} onClick={() => ask(q)} className="rounded-full border bg-card px-4 py-2.5 text-[13.5px] hover:bg-secondary disabled:opacity-50">
                  “{q}”
                </button>
              ))}
            </div>
          </div>
        )}

        <div className="space-y-6 flex-1">
          {turns.map((t) => (
            <div key={t.id}>
              <p className="font-medium mb-2">{t.question}</p>
              <div className="rounded-2xl border bg-card p-4 text-sm leading-relaxed whitespace-pre-wrap select-text">
                {t.answer || (t.result?.error ? <span className="text-destructive">{t.result.error}</span> : <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />)}
              </div>
              {t.result && t.result.checked && (
                <p className={cn('mt-1.5 text-[11px] flex items-center gap-1', t.result.unverified.length ? 'text-amber-700 dark:text-amber-400' : 'text-muted-foreground')}>
                  {t.result.unverified.length === 0
                    ? `✓ Every figure and date checked against the sources${t.result.scopeFixed ? ' · answer re-read against what you asked' : ''}`
                    : `Not found in the sources, treat with care: ${t.result.unverified.join(', ')}`}
                </p>
              )}
              {t.result && t.result.followups.length > 0 && (
                <div className="mt-2 flex flex-wrap gap-2">
                  {t.result.followups.map((q) => (
                    <Button key={q} size="sm" variant="outline" disabled={busy} onClick={() => ask(q)}>{q}</Button>
                  ))}
                </div>
              )}
              {t.result && t.result.sources.length > 0 && (
                <details className="mt-2" open={t.result.sources.length <= 4}>
                  <summary className="text-xs text-muted-foreground cursor-pointer">{t.result.sources.length} sources</summary>
                  <div className="mt-2 space-y-2">
                    {t.result.sources.map((s) => (
                      <div key={s.n} className="flex gap-2">
                        <span className="text-xs font-semibold text-muted-foreground pt-4 w-6 shrink-0">[{s.n}]</span>
                        <div className="flex-1 min-w-0">
                          {s.card && <Card m={s.card} onOpen={setOpenId} onFeedback={(ids, f) => api.memoryFeedback(ids, f)} />}
                          {s.file && <div className="rounded-2xl border bg-card"><FileRow f={s.file} /></div>}
                          {s.task && (
                            <div className="rounded-2xl border bg-card px-4 py-3 text-sm">
                              <span className="text-xs uppercase tracking-wide text-muted-foreground mr-2">Open task</span>
                              {s.task.text}
                              <div className="text-xs text-muted-foreground mt-1">from {s.task.title} · {format(s.task.startedAt, 'd MMM')}</div>
                            </div>
                          )}
                          {s.entity && (
                            <div className="rounded-2xl border bg-card px-4 py-3 text-sm">
                              <span className="text-xs uppercase tracking-wide text-muted-foreground mr-2">{s.entity.kind === 'org' ? 'Organisation' : s.entity.kind}</span>
                              {s.entity.name}
                              <div className="text-xs text-muted-foreground mt-1">mentioned {s.entity.mentions}× · everything known about them was used</div>
                            </div>
                          )}
                        </div>
                      </div>
                    ))}
                  </div>
                </details>
              )}
            </div>
          ))}
          <div ref={bottomRef} />
        </div>

        <form onSubmit={submit} className="sticky bottom-0 bg-background pt-4 pb-2 flex items-center gap-3">
          <div className="relative flex-1 min-w-0">
            <MessageSquare className="absolute left-4 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
            <input
              ref={inputRef}
              value={question}
              onChange={(e) => setQuestion(e.target.value)}
              placeholder={busy ? (mode === 'draft' ? 'Writing…' : 'Answering…') : mode === 'draft' ? 'e.g. Reply to Sarah about the tender deadline, friendly, short' : current ? 'Follow up…' : 'Ask about your memories'}
              disabled={busy}
              className="w-full h-14 rounded-2xl border bg-card pl-11 pr-4 text-[14.5px] outline-none placeholder:text-muted-foreground focus:ring-2 focus:ring-ring disabled:opacity-60"
            />
          </div>
          <button type="submit" disabled={busy || !question.trim()} className="h-14 w-14 shrink-0 rounded-2xl bg-primary text-primary-foreground grid place-items-center hover:brightness-110 disabled:opacity-40" title="Ask">
            {busy ? <Loader2 className="h-5 w-5 animate-spin" /> : <Send className="h-5 w-5" />}
          </button>
        </form>
      </div>

      <ActivitySheet id={openId} onClose={() => setOpenId(null)} onDeleted={() => {}} />
    </div>
  )
}
