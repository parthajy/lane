import { useCallback, useEffect, useMemo, useState } from 'react'
import { format, isToday, isYesterday } from 'date-fns'
import { Clock } from 'lucide-react'
import { toast } from 'sonner'
import { EmptyState } from '@/components/ui/empty-state'
import { Button } from '@/components/ui/button'
import { ActivityRow } from '@/components/activity-row'
import { ActivitySheet } from '@/components/activity-sheet'
import { api, type ActivitySummary } from '@/lib/api'

const PAGE = 100

function dayLabel(ts: number) {
  if (isToday(ts)) return 'Today'
  if (isYesterday(ts)) return 'Yesterday'
  return format(ts, 'EEEE, d MMMM')
}

export function TimelinePage() {
  const [items, setItems] = useState<ActivitySummary[]>([])
  const [loading, setLoading] = useState(true)
  const [hasMore, setHasMore] = useState(false)
  const [openId, setOpenId] = useState<number | null>(null)

  const refresh = useCallback(async () => {
    try {
      const first = await api.listActivities(undefined, PAGE)
      setItems(first)
      setHasMore(first.length === PAGE)
    } catch (e) {
      toast.error(String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    refresh()
    const unlisten = api.onActivityChanged(refresh)
    return () => {
      unlisten.then((fn) => fn())
    }
  }, [refresh])

  async function loadMore() {
    const last = items[items.length - 1]
    if (!last) return
    const next = await api.listActivities(last.startedAt, PAGE)
    setItems((prev) => [...prev, ...next])
    setHasMore(next.length === PAGE)
  }

  const groups = useMemo(() => {
    const out: { label: string; items: ActivitySummary[] }[] = []
    for (const a of items) {
      const label = dayLabel(a.startedAt)
      if (out[out.length - 1]?.label !== label) out.push({ label, items: [] })
      out[out.length - 1].items.push(a)
    }
    return out
  }, [items])

  if (!loading && items.length === 0) {
    return (
      <div className="p-8">
        <EmptyState
          icon={Clock}
          title="Nothing captured yet"
          description="Keep working as usual. Lane records the apps and windows you use, and the text in them once Accessibility access is on. Everything stays on this Mac."
        />
      </div>
    )
  }

  return (
    <div className="px-6 py-6 max-w-3xl">
      <h1 className="text-[26px] font-semibold tracking-tight leading-none mb-4">Timeline</h1>
      {groups.map((g) => (
        <section key={g.label} className="mb-6">
          <h2 className="text-xs font-medium uppercase tracking-wide text-muted-foreground px-3 mb-1">{g.label}</h2>
          <div className="space-y-0.5">
            {g.items.map((a) => (
              <ActivityRow key={a.id} activity={a} onOpen={setOpenId}>
                {a.preview && <p className="text-xs text-muted-foreground mt-1 line-clamp-2">{a.preview}</p>}
              </ActivityRow>
            ))}
          </div>
        </section>
      ))}
      {hasMore && (
        <Button variant="outline" size="sm" onClick={loadMore}>
          Load earlier
        </Button>
      )}
      <ActivitySheet id={openId} onClose={() => setOpenId(null)} onDeleted={refresh} />
    </div>
  )
}
