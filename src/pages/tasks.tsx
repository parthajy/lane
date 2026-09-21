import { useCallback, useEffect, useState } from 'react'
import { format } from 'date-fns'
import { Check, ListChecks, RotateCcw, X } from 'lucide-react'
import { toast } from 'sonner'
import { EmptyState } from '@/components/ui/empty-state'
import { ActivitySheet } from '@/components/activity-sheet'
import { api, type Task } from '@/lib/api'
import { cn } from '@/lib/utils'

type Tab = 'open' | 'done' | 'dismissed'

export function TasksPage() {
  const [tab, setTab] = useState<Tab>('open')
  const [tasks, setTasks] = useState<Task[] | null>(null)
  const [openId, setOpenId] = useState<number | null>(null)

  const refresh = useCallback(() => {
    api.listTasks(tab).then(setTasks).catch((e) => toast.error(String(e)))
  }, [tab])

  useEffect(() => {
    refresh()
    const un = api.onMemoriesChanged(refresh)
    return () => {
      un.then((f) => f())
    }
  }, [refresh])

  async function set(t: Task, status: Tab) {
    await api.setTaskStatus(t.id, status)
    setTasks((prev) => prev?.filter((x) => x.id !== t.id) ?? null)
  }

  return (
    <div className="px-6 py-6 max-w-3xl">
      <div className="flex items-center justify-between mb-1">
        <h1 className="text-[26px] font-semibold tracking-tight leading-none">Tasks</h1>
        <div className="flex gap-1 text-xs">
          {(['open', 'done', 'dismissed'] as Tab[]).map((t) => (
            <button
              key={t}
              onClick={() => setTab(t)}
              className={cn('rounded-full px-2.5 py-1 capitalize', tab === t ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground')}
            >
              {t}
            </button>
          ))}
        </div>
      </div>
      <p className="text-sm text-muted-foreground mb-4">
        Commitments and to-dos Lane noticed in what you read and wrote, each with its source. Tick what's done; dismiss what isn't yours.
      </p>

      {tasks && tasks.length === 0 && (
        <EmptyState
          icon={ListChecks}
          title={tab === 'open' ? 'Nothing open' : `Nothing ${tab}`}
          description={tab === 'open' ? 'Tasks appear when a memory contains a commitment like “send the draft by Friday”.' : undefined}
          tone="muted"
        />
      )}

      <ul className="space-y-1.5">
        {tasks?.map((t) => (
          <li key={t.id} className="flex items-start gap-3 rounded-xl border bg-card px-3 py-2.5">
            {tab === 'open' ? (
              <button title="Done" onClick={() => set(t, 'done')} className="mt-0.5 h-5 w-5 rounded border hover:bg-primary/10 flex items-center justify-center">
                <Check className="h-3 w-3 opacity-0 hover:opacity-100" />
              </button>
            ) : (
              <button title="Reopen" onClick={() => set(t, 'open')} className="mt-0.5 h-5 w-5 rounded border flex items-center justify-center hover:bg-accent">
                <RotateCcw className="h-3 w-3" />
              </button>
            )}
            <div className="min-w-0 flex-1">
              <div className={cn('text-sm', tab !== 'open' && 'line-through text-muted-foreground')}>{t.text}</div>
              <button onClick={() => setOpenId(t.activityId)} className="text-xs text-muted-foreground hover:underline truncate block max-w-full text-left">
                {t.title} · {t.appName} · {format(t.startedAt, 'EEE d MMM, HH:mm')}
              </button>
            </div>
            {tab === 'open' && (
              <button title="Dismiss" onClick={() => set(t, 'dismissed')} className="rounded-md p-1 text-muted-foreground hover:bg-accent">
                <X className="h-3.5 w-3.5" />
              </button>
            )}
          </li>
        ))}
      </ul>

      <ActivitySheet id={openId} onClose={() => setOpenId(null)} onDeleted={refresh} />
    </div>
  )
}
