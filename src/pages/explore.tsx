import { useEffect, useState } from 'react'
import { format } from 'date-fns'
import { BarChart3, Building2, FolderKanban, Users } from 'lucide-react'
import { toast } from 'sonner'
import { EmptyState } from '@/components/ui/empty-state'
import { api, formatDuration, type Explore } from '@/lib/api'
import { Donut } from '@/components/charts'
import { GoldenCircle } from '@/components/golden-circle'
import { ShareCard } from '@/components/share-card'
import { ActivitySheet } from '@/components/activity-sheet'

/** A day chart with a scale down the side and a date every third bar. */
function DayChart({ data, unit, axis }: { data: [string, number][]; unit: (n: number) => string; axis: (n: number) => string }) {
  const days = data.slice(-30)
  const max = Math.max(1, ...days.map((d) => d[1]))
  const step = max / 2
  return (
    <div className="flex gap-3">
      <div className="flex flex-col justify-between h-28 text-[11px] text-muted-foreground tabular-nums shrink-0 text-right w-10">
        <span>{axis(max)}</span><span>{axis(step)}</span><span>{axis(0)}</span>
      </div>
      <div className="min-w-0 flex-1">
        <div className="relative h-28">
          <div className="absolute inset-x-0 top-0 border-t border-dashed border-border" />
          <div className="absolute inset-x-0 top-1/2 border-t border-dashed border-border" />
          <div className="absolute inset-x-0 bottom-0 border-t border-border" />
          <div className="absolute inset-0 flex items-end gap-[3px]">
            {days.map(([day, n]) => (
              <div key={day} className="flex-1 flex flex-col justify-end h-full" title={`${format(new Date(day), 'EEE d MMM')}: ${unit(n)}`}>
                <div className="rounded-t-sm bg-primary/70 hover:bg-primary transition-colors" style={{ height: `${Math.max(2, (n / max) * 100)}%` }} />
              </div>
            ))}
          </div>
        </div>
        <div className="flex gap-[3px] mt-1.5">
          {days.map(([day], i) => (
            <div key={day} className="flex-1 text-[10.5px] text-muted-foreground text-center truncate">
              {i % 3 === 0 ? format(new Date(day), 'd MMM') : ''}
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

const KIND_DOT = ['#5a51e5', '#8b5cf6', '#2b9cf3', '#f5a524', '#f2555a', '#4f7bf5', '#a78bfa', '#8b8ba7']

function Stat({ n, label }: { n: number | string; label: string }) {
  return (
    <div className="rounded-xl border bg-card p-3">
      <div className="text-2xl font-semibold tabular-nums">{n}</div>
      <div className="text-xs text-muted-foreground">{label}</div>
    </div>
  )
}

/** What Lane has seen: volume, kinds, who and what came up, over the last 30 days. */
export function ExplorePage({ onAsk, onOpenPeople }: { onAsk: (q: string) => void; onOpenPeople?: () => void }) {
  const [data, setData] = useState<Explore | null>(null)
  const [insights, setInsights] = useState<string[]>([])
  const [openId, setOpenId] = useState<number | null>(null)
  useEffect(() => {
    api.explore().then(setData).catch((e) => toast.error(String(e)))
    api.insights().then(setInsights).catch(() => {})
  }, [])
  if (!data) return null
  if (data.memories === 0) {
    return (
      <div className="p-8">
        <EmptyState icon={BarChart3} title="Nothing to explore yet" description="Once Lane has made a few memories, this page shows what your days look like." />
      </div>
    )
  }
  const totalMin = data.minutesPerDay.reduce((a, d) => a + d[1], 0)
  const kindTotal = data.byKind.reduce((a, k) => a + k[1], 0)
  const LISTS: [string, Explore['topPeople'], string, typeof Users][] = [
    ['People', data.topPeople, 'Who is', Users],
    ['Organisations', data.topOrgs, 'What do I know about', Building2],
    ['Projects', data.topProjects, 'What is the state of', FolderKanban],
  ]

  return (
    <div className="px-6 py-6 max-w-[1100px] space-y-4">
      <div className="flex items-end justify-between gap-6 flex-wrap">
        <div className="flex items-start gap-4">
          <span className="h-14 w-14 shrink-0 rounded-2xl tone-violet grid place-items-center text-primary"><BarChart3 className="h-7 w-7" /></span>
          <div>
            <h1 className="text-[32px] font-semibold tracking-[-0.03em] leading-none">Explore</h1>
            <p className="text-[13.5px] text-muted-foreground mt-2">The last 30 days, from your own memories. Nothing here leaves this Mac.</p>
          </div>
        </div>
        <p className="text-[13px] text-muted-foreground italic">“A more thoughtful you, on your terms.”</p>
      </div>

      <ShareCard data={data} />

      <section className="rounded-2xl border bg-card p-5"><GoldenCircle onOpen={setOpenId} /></section>
      <ActivitySheet id={openId} onClose={() => setOpenId(null)} onDeleted={() => {}} />

      {insights.length > 0 && (
        <section className="rounded-2xl border bg-card p-5">
          <h2 className="text-[10.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground mb-3">This week, from the whole memory</h2>
          <ul className="space-y-1.5 text-[14px]">
            {insights.map((i) => <li key={i} className="flex gap-2.5"><span className="text-primary">•</span><span>{i}</span></li>)}
          </ul>
        </section>
      )}

      <section className="rounded-2xl border bg-card p-5">
        <div className="flex items-baseline justify-between gap-4 mb-4">
          <h2 className="text-[10.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground">Time captured per day</h2>
          <span className="text-[12.5px] text-muted-foreground">Total these 30 days: <b className="text-foreground font-medium">{formatDuration(totalMin * 60000) || '0m'}</b></span>
        </div>
        <DayChart data={data.minutesPerDay} unit={(n) => formatDuration(n * 60000)} axis={(n) => `${Math.round(n)}m`} />
      </section>

      <section className="rounded-2xl border bg-card p-5">
        <div className="flex items-baseline justify-between gap-4 mb-4">
          <h2 className="text-[10.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground">Memories per day</h2>
          <span className="text-[12.5px] text-muted-foreground"><b className="text-foreground font-medium">{data.kept.toLocaleString()}</b> kept of {data.memories.toLocaleString()}</span>
        </div>
        <DayChart data={data.memoriesPerDay} unit={(n) => `${n} memories`} axis={(n) => String(Math.round(n))} />
      </section>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        {LISTS.map(([title, list, verb, Icon]) => (
          <section key={title} className="rounded-2xl border bg-card p-5">
            <div className="flex items-center gap-2 mb-3">
              <Icon className="h-4 w-4 text-primary" />
              <h2 className="text-[10.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground">{title}</h2>
              {onOpenPeople && <button onClick={onOpenPeople} className="ml-auto rounded-lg border px-2.5 py-1 text-[11.5px] text-muted-foreground hover:bg-secondary hover:text-foreground">View all</button>}
            </div>
            <ul className="space-y-1.5">
              {list.map((e) => (
                <li key={e.id} className="flex items-center gap-2 text-[13.5px]">
                  <button className="truncate hover:underline text-left" onClick={() => onAsk(`${verb} ${e.name}?`)}>{e.name}</button>
                  <span className="ml-auto text-[12px] text-muted-foreground tabular-nums shrink-0">{e.mentions}</span>
                </li>
              ))}
              {list.length === 0 && <li className="text-xs text-muted-foreground">none yet</li>}
            </ul>
          </section>
        ))}
      </div>

      {data.byCategory.length > 0 && (
        <section className="rounded-2xl border bg-card p-5">
          <h2 className="text-[10.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground mb-4">Where the time went</h2>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-x-8 gap-y-3">
            <ul className="space-y-2.5">
              {data.byCategory.slice(0, 6).map(([label, mins], i) => {
                const max = Math.max(1, ...data.byCategory.map((c) => c[1]))
                return (
                  <li key={label}>
                    <div className="flex items-baseline gap-2 text-[13.5px]">
                      <span className="truncate">{label}</span>
                      <span className="ml-auto text-[12px] text-muted-foreground tabular-nums">{formatDuration(mins * 60000)}</span>
                    </div>
                    <div className="h-1.5 rounded-full bg-muted overflow-hidden mt-1.5">
                      <div className="h-full rounded-full" style={{ width: `${(mins / max) * 100}%`, background: KIND_DOT[i % KIND_DOT.length] }} />
                    </div>
                  </li>
                )
              })}
            </ul>
            <ul className="space-y-2">
              {data.topPlaces.slice(0, 6).map(([name, cat, mins], i) => (
                <li key={name} className="flex items-center gap-2.5 text-[13.5px]">
                  <span className="h-2 w-2 rounded-full shrink-0" style={{ background: KIND_DOT[i % KIND_DOT.length] }} />
                  <span className="truncate">{name}</span>
                  <span className="text-[11.5px] text-muted-foreground truncate hidden lg:inline">· {cat}</span>
                  <span className="ml-auto text-[12px] text-muted-foreground tabular-nums shrink-0">{formatDuration(mins * 60000)}</span>
                </li>
              ))}
            </ul>
          </div>
        </section>
      )}

      <section className="rounded-2xl border bg-card p-5">
        <h2 className="text-[10.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground mb-4">What kind of things</h2>
        <div className="grid grid-cols-1 lg:grid-cols-[auto_minmax(0,1fr)] gap-6 items-center">
          <Donut data={data.byKind.slice(0, 8)} size={150} thickness={20} className="shrink-0" />
          <div className="flex flex-wrap gap-2 content-start">
            {data.byKind.map(([k, n], i) => (
              <span key={k} className="inline-flex items-center gap-2 rounded-xl border px-3 py-1.5 text-[12.5px] capitalize">
                <span className="h-2 w-2 rounded-full" style={{ background: KIND_DOT[i % KIND_DOT.length] }} />
                {k}
                <span className="text-muted-foreground tabular-nums">{n}</span>
                <span className="text-muted-foreground/70 tabular-nums">{kindTotal ? `${Math.round((n / kindTotal) * 100)}%` : ''}</span>
              </span>
            ))}
          </div>
        </div>
        <p className="text-[12.5px] text-muted-foreground mt-4 pt-4 border-t">
          {data.files.toLocaleString()} documents indexed · {data.meetings} meetings · {data.facts.toLocaleString()} facts{data.conflicts ? ` · ${data.conflicts} disagree` : ''} · {data.tasksOpen} open tasks, {data.tasksDone} done · {data.pinned} pinned · {data.edited} in your words
        </p>
      </section>
    </div>
  )
}
