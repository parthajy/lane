import { format } from 'date-fns'
import { Globe, FileText } from 'lucide-react'
import { type ActivitySummary, formatDuration, hostOf } from '@/lib/api'
import { cn } from '@/lib/utils'

/** Two-letter monogram tile for an app; stable colour-free styling. */
export function AppMonogram({ name, className }: { name: string; className?: string }) {
  const letters = name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0]?.toUpperCase())
    .join('')
  return (
    <div
      className={cn(
        'h-8 w-8 shrink-0 rounded-xl border bg-card flex items-center justify-center text-[11px] font-semibold text-muted-foreground',
        className,
      )}
    >
      {letters || '?'}
    </div>
  )
}

export function ActivityRow({
  activity,
  onOpen,
  children,
}: {
  activity: ActivitySummary
  onOpen: (id: number) => void
  children?: React.ReactNode
}) {
  const host = hostOf(activity.url)
  const title = activity.windowTitle || activity.appName
  return (
    <button
      onClick={() => onOpen(activity.id)}
      className="w-full text-left flex gap-3 px-3 py-2.5 rounded-lg hover:bg-accent/60 transition-colors"
    >
      <AppMonogram name={activity.appName} />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className="font-medium truncate">{title}</span>
          <span className="ml-auto shrink-0 text-xs text-muted-foreground tabular-nums">
            {format(activity.startedAt, 'HH:mm')} · {formatDuration(activity.endedAt - activity.startedAt)}
          </span>
        </div>
        <div className="flex items-center gap-1.5 text-xs text-muted-foreground mt-0.5">
          <span className="truncate">{activity.appName}</span>
          {host && (
            <>
              <span>·</span>
              {activity.url?.startsWith('file:') ? <FileText className="h-3 w-3" /> : <Globe className="h-3 w-3" />}
              <span className="truncate">{host}</span>
            </>
          )}
        </div>
        {children}
      </div>
    </button>
  )
}
