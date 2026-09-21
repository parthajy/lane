import { useEffect, useState } from 'react'
import { format } from 'date-fns'
import { Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { Sheet, SheetBody, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { api, type ActivityDetail, formatDuration } from '@/lib/api'

/** The picture kept with a snapshot, loaded on demand. */
function Shot({ path }: { path: string }) {
  const [src, setSrc] = useState<string | null>(null)
  useEffect(() => {
    let live = true
    api.snapshotImage(path).then((d) => live && setSrc(d)).catch(() => {})
    return () => { live = false }
  }, [path])
  if (!src) return null
  return <img src={src} alt="" className="w-full rounded-t-lg border-b object-cover max-h-72" />
}

/** The captured text of one activity, for a sheet or a detail pane. */
export function ActivityDetailView({ id, onDeleted, header = true }: { id: number | null; onDeleted?: () => void; header?: boolean }) {
  const [detail, setDetail] = useState<ActivityDetail | null>(null)

  useEffect(() => {
    setDetail(null)
    if (id != null) api.getActivity(id).then(setDetail).catch((e) => toast.error(String(e)))
  }, [id])

  async function remove() {
    if (id == null) return
    await api.deleteActivity(id)
    toast.success('Deleted from this Mac')
    onDeleted?.()
  }

  const a = detail?.activity
  if (id == null) return null
  return (
    <div className="space-y-4">
      {header && a && (
        <div>
          <h3 className="font-medium leading-snug">{a.windowTitle || a.appName}</h3>
          <p className="text-xs text-muted-foreground mt-0.5">{a.appName} · {format(a.startedAt, 'EEE d MMM, HH:mm')} · {formatDuration(a.endedAt - a.startedAt)}</p>
          {a.url && <p className="text-xs text-muted-foreground mt-1 break-all select-text">{a.url}</p>}
        </div>
      )}
      {detail && detail.snapshots.length === 0 && (
        <p className="text-sm text-muted-foreground">
          No text was captured for this activity. That happens when Accessibility access is off, when an app doesn't expose its text, or for very short visits.
        </p>
      )}
      {detail?.snapshots.map((s, i) => (
        <div key={s.id} className="rounded-xl border bg-card overflow-hidden">
          {s.image && <Shot path={s.image} />}
          <div className="flex items-center justify-between px-3 py-2 border-b">
            <Badge variant="secondary">Snapshot {i + 1}</Badge>
            <span className="text-xs text-muted-foreground tabular-nums">{format(s.capturedAt, 'HH:mm:ss')}</span>
          </div>
          <pre className="px-3 py-2 text-xs whitespace-pre-wrap break-words max-h-80 overflow-auto select-text font-sans leading-relaxed">{s.text}</pre>
          {s.raw !== s.text && (
            <details className="border-t px-3 py-2 text-xs text-muted-foreground">
              <summary className="cursor-pointer">Raw capture ({s.raw.length.toLocaleString()} characters, {s.text.length.toLocaleString()} kept)</summary>
              <pre className="mt-2 whitespace-pre-wrap break-words max-h-60 overflow-auto select-text font-sans leading-relaxed">{s.raw}</pre>
            </details>
          )}
        </div>
      ))}
      {detail && (
        <div className="pt-1">
          <Button variant="outline" size="sm" onClick={remove}><Trash2 className="h-3.5 w-3.5 mr-1.5" /> Delete this memory</Button>
        </div>
      )}
    </div>
  )
}

export function ActivitySheet({ id, onClose, onDeleted }: { id: number | null; onClose: () => void; onDeleted: () => void }) {
  const [title, setTitle] = useState<{ t: string; sub: string } | null>(null)
  useEffect(() => {
    setTitle(null)
    if (id != null) api.getActivity(id).then((d) => d && setTitle({ t: d.activity.windowTitle || d.activity.appName, sub: `${d.activity.appName} · ${format(d.activity.startedAt, 'EEE d MMM, HH:mm')} · ${formatDuration(d.activity.endedAt - d.activity.startedAt)}` })).catch(() => {})
  }, [id])
  return (
    <Sheet open={id != null} onOpenChange={(open) => !open && onClose()}>
      <SheetContent widthClass="w-full sm:max-w-2xl">
        <SheetHeader>
          <SheetTitle className="pr-8">{title?.t ?? 'Loading…'}</SheetTitle>
          {title && <SheetDescription>{title.sub}</SheetDescription>}
        </SheetHeader>
        <SheetBody>
          <ActivityDetailView id={id} header={false} onDeleted={() => { onDeleted(); onClose() }} />
        </SheetBody>
      </SheetContent>
    </Sheet>
  )
}
