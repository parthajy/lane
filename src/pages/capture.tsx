import { useEffect, useState } from 'react'
import { format } from 'date-fns'
import { Check, Mic, PenLine, Square } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { ActivitySheet } from '@/components/activity-sheet'
import { api, type RecordingReport } from '@/lib/api'

/** Capture: a typed note or a dictated one, straight into memory. */
export function CapturePage({ embedded = false }: { embedded?: boolean } = {}) {
  const [text, setText] = useState('')
  const [rec, setRec] = useState<RecordingReport | null>(null)
  const [decisions, setDecisions] = useState<{ text: string; title: string; activityId: number; at: number }[]>([])
  const [openId, setOpenId] = useState<number | null>(null)

  useEffect(() => {
    const tick = () => api.recordingStatus().then(setRec).catch(() => {})
    tick()
    const t = setInterval(tick, 1000)
    api.listDecisions(Date.now() - 30 * 86_400_000).then(setDecisions).catch(() => {})
    const un = api.onMemoriesChanged(() => api.listDecisions(Date.now() - 30 * 86_400_000).then(setDecisions).catch(() => {}))
    return () => {
      clearInterval(t)
      un.then((f) => f())
    }
  }, [])

  async function save() {
    const t = text.trim()
    if (!t) return
    try {
      await api.addNote(t)
      setText('')
      toast.success('Saved. Rabbit will file it in a couple of minutes.')
    } catch (e) {
      toast.error(String(e))
    }
  }

  const recording = rec?.recording ?? false

  return (
    <div className={embedded ? 'px-6 pb-6 max-w-3xl' : 'px-6 py-6 max-w-3xl'}>
      {!embedded && <h1 className="text-[26px] font-semibold tracking-tight leading-none mb-1">Capture</h1>}
      <p className="text-sm text-muted-foreground mb-4">Type or dictate anything you want Lane to remember. It becomes a memory with its people, tasks and decisions.</p>

      <div className="rounded-2xl border bg-card p-4 space-y-3">
        <Textarea
          rows={5}
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="A thought, a decision, something someone told you…  (⌘↩ to save)"
          onKeyDown={(e) => {
            if (e.metaKey && e.key === 'Enter') save()
          }}
        />
        <div className="flex items-center gap-2">
          <Button onClick={save} disabled={!text.trim()}>
            <PenLine className="h-4 w-4 mr-2" /> Save note
          </Button>
          {recording ? (
            <Button variant="destructive" onClick={() => api.stopMeeting()}>
              <Square className="h-4 w-4 mr-2" /> Stop · {Math.floor((rec?.micSeconds ?? 0) / 60)}:{String((rec?.micSeconds ?? 0) % 60).padStart(2, '0')}
            </Button>
          ) : (
            <Button variant="outline" disabled={!rec?.ready} onClick={() => api.startMeeting(undefined, true).catch((e) => toast.error(String(e)))}>
              <Mic className="h-4 w-4 mr-2" /> Record a voice note
            </Button>
          )}
          {recording && rec?.liveText && <span className="text-xs text-muted-foreground truncate max-w-xs">{rec.liveText.slice(-80)}</span>}
        </div>
      </div>

      {decisions.length > 0 && (
        <div className="mt-6">
          <h2 className="text-xs font-medium uppercase tracking-wide text-muted-foreground mb-2">Decisions, last 30 days</h2>
          <ul className="space-y-1.5">
            {decisions.slice(0, 20).map((d, i) => (
              <li key={i} className="flex gap-2 text-sm">
                <Check className="h-4 w-4 text-primary shrink-0 mt-0.5" />
                <span>
                  {d.text}{' '}
                  <button className="text-xs text-muted-foreground hover:underline" onClick={() => setOpenId(d.activityId)}>
                    ({d.title} · {format(d.at, 'd MMM')})
                  </button>
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
      <ActivitySheet id={openId} onClose={() => setOpenId(null)} onDeleted={() => {}} />
    </div>
  )
}
