import { useCallback, useEffect, useRef, useState } from 'react'
import { format } from 'date-fns'
import { Circle, Copy, FileDown, FileText, FolderOpen, Loader2, Mic, Play, RefreshCw, Send, Square, Trash2, Users } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'
import { EmptyState } from '@/components/ui/empty-state'
import { ActivitySheet } from '@/components/activity-sheet'
import { api, formatDuration, type Meeting, type NotionStatus, type RecordingReport, type Settings } from '@/lib/api'
import { cn } from '@/lib/utils'

function mmss(s: number) {
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
}

/** The notes are light Markdown: paragraphs, **headings**, "- " bullets and "- [ ]" items. */
function Notes({ text }: { text: string }) {
  const blocks = text.split(/\n{2,}/)
  return (
    <div className="space-y-3 text-sm leading-relaxed select-text">
      {blocks.map((b, i) => {
        const lines = b.split('\n')
        const heading = lines[0].match(/^\*\*(.+)\*\*$/)
        const items = (heading ? lines.slice(1) : lines).filter((l) => l.startsWith('- '))
        if (heading || items.length) {
          return (
            <div key={i}>
              {heading && <h3 className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground mb-1">{heading[1]}</h3>}
              <ul className="space-y-1">
                {items.map((l, j) => {
                  const todo = l.startsWith('- [ ] ')
                  return (
                    <li key={j} className="flex gap-2">
                      <span className="text-muted-foreground shrink-0">{todo ? '☐' : '•'}</span>
                      <span>{l.replace(/^- (\[ \] )?/, '')}</span>
                    </li>
                  )
                })}
              </ul>
            </div>
          )
        }
        return <p key={i}>{b}</p>
      })}
    </div>
  )
}

export function MeetingsPage() {
  const [rec, setRec] = useState<RecordingReport | null>(null)
  const [meetings, setMeetings] = useState<Meeting[] | null>(null)
  const [notion, setNotion] = useState<NotionStatus | null>(null)
  const [settings, setSettings] = useState<Settings | null>(null)
  const [title, setTitle] = useState('')
  const [selected, setSelected] = useState<number | null>(null)
  const [openId, setOpenId] = useState<number | null>(null)
  const [busy, setBusy] = useState(false)
  const [renaming, setRenaming] = useState<string | null>(null)
  const [naming, setNaming] = useState<{ label: string; value: string } | null>(null)
  const [confirmDelete, setConfirmDelete] = useState(false)

  const refresh = useCallback(() => {
    api.listMeetings().then(setMeetings).catch(() => {})
    api.recordingStatus().then(setRec).catch(() => {})
  }, [])

  useEffect(() => {
    refresh()
    api.notionStatus().then(setNotion).catch(() => {})
    api.getSettings().then(setSettings).catch(() => {})
    const t = setInterval(() => api.recordingStatus().then(setRec).catch(() => {}), 1000)
    const un = api.onMeetingsChanged(refresh)
    return () => {
      clearInterval(t)
      un.then((f) => f())
    }
  }, [refresh])

  // The newest meeting is selected when the list first loads and whenever a
  // new one appears (a recording just started), so its notes land in view.
  const newest = useRef<number | null>(null)
  useEffect(() => {
    if (!meetings || meetings.length === 0) return
    const top = meetings[0].id
    if (selected == null || !meetings.some((m) => m.id === selected) || (newest.current != null && top !== newest.current)) setSelected(top)
    newest.current = top
  }, [meetings, selected])

  async function start() {
    setBusy(true)
    try {
      await api.startMeeting(title.trim() || undefined)
      setTitle('')
    } catch (e) {
      toast.error(String(e))
    } finally {
      setBusy(false)
      refresh()
    }
  }

  async function stop() {
    setBusy(true)
    try {
      await api.stopMeeting()
    } finally {
      setBusy(false)
      refresh()
    }
  }

  async function keepAudio(v: boolean) {
    if (!settings) return
    const next = await api.updateSettings({ ...settings, keepAudio: v })
    setSettings(next)
  }

  const recording = rec?.recording ?? false
  const elapsed = rec?.startedAt ? Math.max(0, Math.round((Date.now() - rec.startedAt) / 1000)) : 0
  const m = meetings?.find((x) => x.id === selected) ?? null
  const name = (x: Meeting) => x.memoryTitle ?? x.title

  return (
    <div className="grid grid-cols-[340px_minmax(0,1fr)] h-full min-h-0">
      {/* Left: record, then the list */}
      <aside className="border-r flex flex-col min-h-0">
        <div className="p-4 border-b">
          <h1 className="text-[26px] font-semibold tracking-tight leading-none mb-3">Meetings</h1>
          <div className={cn('rounded-xl border p-3', recording ? 'border-destructive/40 bg-destructive/5' : 'bg-card')}>
            {recording ? (
              <div className="flex items-center gap-3">
                <Circle className="h-3 w-3 fill-destructive text-destructive animate-pulse-soft shrink-0" />
                <div className="flex-1 min-w-0">
                  <div className="font-medium tabular-nums">Recording · {mmss(elapsed)}</div>
                  <div className="text-xs text-muted-foreground truncate">
                    you {rec?.micOk ? mmss(rec.micSeconds) : 'off'} · them {rec?.systemOk ? mmss(rec.systemSeconds) : 'off'}
                  </div>
                </div>
                <Button size="sm" variant="destructive" onClick={stop} disabled={busy}><Square className="h-3.5 w-3.5 mr-1.5" /> Stop</Button>
              </div>
            ) : rec?.detail === 'transcribing' ? (
              <div className="flex items-center gap-2 text-sm"><Loader2 className="h-4 w-4 animate-spin" /> Transcribing and writing notes…</div>
            ) : (
              <div className="space-y-2">
                <Input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Title (optional, else the calendar event)" className="h-9" />
                <div className="flex items-center gap-2">
                  <Button size="sm" onClick={start} disabled={busy || !rec?.ready}><Mic className="h-3.5 w-3.5 mr-1.5" /> Record</Button>
                  <span className="text-xs text-muted-foreground truncate">{rec?.detailReady}</span>
                </div>
              </div>
            )}
            {recording && rec?.liveText && (
              <pre className="mt-3 max-h-40 overflow-y-auto whitespace-pre-wrap text-xs font-sans leading-relaxed text-muted-foreground border-t pt-2 select-text">{rec.liveText.slice(-1500)}</pre>
            )}
          </div>
          <p className="text-[11px] text-muted-foreground mt-2 leading-snug">
            Hears both sides, only while recording. Transcribed and written up on this Mac; no bot joins and nothing shows on screen.
          </p>
        </div>
        <ul className="overflow-y-auto min-h-0 flex-1 p-2 space-y-0.5">
          {meetings?.map((x) => (
            <li key={x.id}>
              <button onClick={() => { setSelected(x.id); setRenaming(null); setNaming(null); setConfirmDelete(false) }} className={cn('w-full text-left rounded-lg px-3 py-2', selected === x.id ? 'bg-accent' : 'hover:bg-accent/60')}>
                <div className="flex items-baseline gap-2">
                  <span className="truncate font-medium text-sm flex-1">{name(x)}</span>
                  {x.status === 'recording' && <Circle className="h-2 w-2 fill-destructive text-destructive shrink-0" />}
                  {x.status === 'transcribing' && <Loader2 className="h-3 w-3 animate-spin text-muted-foreground shrink-0" />}
                </div>
                <div className="text-[11px] text-muted-foreground tabular-nums">
                  {format(x.startedAt, 'EEE d MMM, HH:mm')}{x.endedAt && ` · ${formatDuration(x.endedAt - x.startedAt)}`}{x.attendees.length > 0 && ` · ${x.attendees.length} on screen`}
                </div>
              </button>
            </li>
          ))}
          {meetings && meetings.length === 0 && <li className="px-3 py-2 text-xs text-muted-foreground">Press Record before a call.</li>}
        </ul>
        {settings && (
          <div className="border-t px-4 py-2.5 flex items-center justify-between gap-3">
            <div className="min-w-0">
              <div className="text-xs font-medium">Keep audio</div>
              <div className="text-[11px] text-muted-foreground truncate">Play it back later. Stays on this Mac.</div>
            </div>
            <Switch checked={settings.keepAudio} onCheckedChange={keepAudio} />
          </div>
        )}
      </aside>

      {/* Right: the selected meeting */}
      <div className="overflow-y-auto min-h-0">
        {!m && meetings && meetings.length === 0 && (
          <EmptyState icon={Mic} title="No meetings yet" description="Press Record before a call. When you stop, the transcript becomes notes and a memory with its people and action items." tone="muted" />
        )}
        {m && (
          <div className="px-6 py-5 max-w-3xl">
            <div className="flex items-start gap-3 mb-1">
              {renaming != null ? (
                <form className="flex-1 flex gap-2" onSubmit={(e) => { e.preventDefault(); api.renameMeeting(m.id, renaming).then(() => { setRenaming(null); refresh() }).catch((er) => toast.error(String(er))) }}>
                  <Input autoFocus value={renaming} onChange={(e) => setRenaming(e.target.value)} className="h-9" />
                  <Button size="sm" type="submit">Save</Button>
                  <Button size="sm" variant="ghost" type="button" onClick={() => setRenaming(null)}>Cancel</Button>
                </form>
              ) : (
                <h2 className="text-[26px] font-semibold tracking-tight leading-none flex-1 min-w-0 cursor-text" title="Click to rename" onClick={() => setRenaming(m.title)}>{name(m)}</h2>
              )}
            </div>
            <p className="text-sm text-muted-foreground tabular-nums mb-3">
              {format(m.startedAt, 'EEEE d MMMM, HH:mm')}{m.endedAt && ` · ${formatDuration(m.endedAt - m.startedAt)}`}
              {m.status === 'done' && ` · ${m.detail}`}
              {m.status === 'failed' && <span className="text-destructive"> · failed: {m.detail}</span>}
            </p>

            {m.speakers.length > 0 && (
              <div className="flex flex-wrap items-center gap-1.5 mb-2">
                <Mic className="h-3.5 w-3.5 text-muted-foreground" />
                {m.speakers.map((sp) => (
                  naming && naming.label === sp.label ? (
                    <form
                      key={sp.label}
                      className="flex items-center gap-1"
                      onSubmit={(e) => {
                        e.preventDefault()
                        const name = naming.value.trim()
                        if (!name) return
                        api.renameSpeaker(m.id, sp.name || sp.label, name).then((n) => { toast.success(`${sp.name || sp.label} is now ${name} in ${n} transcript parts`); setNaming(null); refresh() }).catch((e) => toast.error(String(e)))
                      }}
                    >
                      <Input autoFocus value={naming.value} onChange={(e) => setNaming({ ...naming, value: e.target.value })} placeholder={`Who is ${sp.label}?`} className="h-7 w-40 text-xs" onKeyDown={(e) => { if (e.key === 'Escape') setNaming(null) }} />
                      <Button size="sm" type="submit" className="h-7 px-2 text-xs">Name</Button>
                    </form>
                  ) : (
                    <button
                      key={sp.label}
                      className="rounded-full border bg-card px-2 py-0.5 text-xs hover:bg-accent"
                      title="Click to give this voice a name"
                      onClick={() => setNaming({ label: sp.label, value: sp.name || (m.attendees[m.speakers.indexOf(sp)] ?? '') })}
                    >
                      {sp.name ? sp.name : sp.label}{sp.name ? '' : ' · name?'}
                    </button>
                  )
                ))}
                <span className="text-[11px] text-muted-foreground">voices on the call</span>
              </div>
            )}
            {m.attendees.length > 0 && (
              <div className="flex flex-wrap items-center gap-1.5 mb-4">
                <Users className="h-3.5 w-3.5 text-muted-foreground" />
                {m.attendees.map((a) => <span key={a} className="rounded-full border bg-card px-2 py-0.5 text-xs">{a}</span>)}
                <span className="text-[11px] text-muted-foreground">seen on screen</span>
              </div>
            )}

            {m.status === 'done' && (
              <div className="flex flex-wrap gap-1.5 mb-4">
                <Button size="sm" variant="outline" onClick={() => api.meetingSummary(m.id).then((t) => api.copyText(t)).then(() => toast.success('Notes copied. Paste them into an email or chat.')).catch((e) => toast.error(String(e)))}>
                  <Copy className="h-3.5 w-3.5 mr-1.5" /> Copy notes
                </Button>
                <Button size="sm" variant="outline" onClick={() => api.meetingSummary(m.id, true).then((t) => api.saveMarkdown(name(m), t)).then((p) => toast.success(`Saved ${p.split('/').pop()}`)).catch((e) => toast.error(String(e)))}>
                  <FileDown className="h-3.5 w-3.5 mr-1.5" /> Save with transcript
                </Button>
                {notion?.connected && (
                  <Button size="sm" variant="outline" onClick={() => api.meetingSummary(m.id).then((t) => api.exportToNotion(name(m), t)).then(() => toast.success('Sent to Notion')).catch((e) => toast.error(String(e)))}>
                    <Send className="h-3.5 w-3.5 mr-1.5" /> Notion
                  </Button>
                )}
                <Button size="sm" variant="outline" onClick={() => setOpenId(m.activityId)}>
                  <FileText className="h-3.5 w-3.5 mr-1.5" /> Transcript
                </Button>
                {m.hasAudio && (
                  <>
                    <Button size="sm" variant="outline" onClick={() => api.openMeetingAudio(m.id).catch((e) => toast.error(String(e)))}>
                      <Play className="h-3.5 w-3.5 mr-1.5" /> Play audio
                    </Button>
                    <Button size="sm" variant="ghost" title="Show in Finder" onClick={() => api.openMeetingAudio(m.id, true).catch((e) => toast.error(String(e)))}>
                      <FolderOpen className="h-3.5 w-3.5" />
                    </Button>
                  </>
                )}
                <Button size="sm" variant="ghost" title="Write the notes again" onClick={() => { toast.message('Writing notes…'); api.meetingNotes(m.id).catch((e) => toast.error(String(e))) }}>
                  <RefreshCw className="h-3.5 w-3.5" />
                </Button>
                {confirmDelete ? (
                  <span className="ml-auto flex items-center gap-1 text-xs">
                    Delete with transcript{m.hasAudio ? ' and audio' : ''}?
                    <Button size="sm" variant="destructive" className="h-7 px-2 text-xs" onClick={() => { setConfirmDelete(false); api.deleteMeeting(m.id).then(refresh).catch((e) => toast.error(String(e))) }}>Delete</Button>
                    <Button size="sm" variant="ghost" className="h-7 px-2 text-xs" onClick={() => setConfirmDelete(false)}>Keep</Button>
                  </span>
                ) : (
                  <Button size="sm" variant="ghost" className="ml-auto text-muted-foreground hover:text-destructive" title="Delete this meeting, its transcript and audio" onClick={() => setConfirmDelete(true)}>
                    <Trash2 className="h-3.5 w-3.5" />
                  </Button>
                )}
              </div>
            )}

            <div className="rounded-2xl border bg-card p-5">
              {m.status === 'recording' && <p className="text-sm text-muted-foreground">Recording. Live notes appear in the panel on the left after the first 30 seconds.</p>}
              {m.status === 'transcribing' && <p className="text-sm text-muted-foreground flex items-center gap-2"><Loader2 className="h-4 w-4 animate-spin" /> Transcribing, then writing the notes. A minute or two for a long call.</p>}
              {m.status === 'done' && m.notes && <Notes text={m.notes} />}
              {m.status === 'done' && !m.notes && (
                <div className="text-sm">
                  <p className="text-muted-foreground">{m.memorySummary ?? 'No notes yet.'}</p>
                  <Button size="sm" variant="outline" className="mt-3" onClick={() => { toast.message('Writing notes…'); api.meetingNotes(m.id).catch((e) => toast.error(String(e))) }}>
                    <RefreshCw className="h-3.5 w-3.5 mr-1.5" /> Write notes
                  </Button>
                </div>
              )}
              {m.status === 'failed' && <p className="text-sm text-destructive">{m.detail}</p>}
            </div>
            {m.status === 'done' && m.notes && m.memorySummary && (
              <p className="text-xs text-muted-foreground mt-3">Memory: {m.memorySummary}</p>
            )}
          </div>
        )}
      </div>

      <ActivitySheet id={openId} onClose={() => setOpenId(null)} onDeleted={refresh} />
    </div>
  )
}
