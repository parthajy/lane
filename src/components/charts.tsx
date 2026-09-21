import { cn } from '@/lib/utils'

/** Small dependency-free SVG charts for the three-pane layouts. */

/* The app's chart hues, matching lane.so: indigo first, then the rest of the set. */
const PALETTE = ['#5a51e5', '#8b5cf6', '#2b9cf3', '#f5a524', '#f2555a', '#4f7bf5', '#a78bfa', '#8b8ba7']

export function Donut({ data, size = 120, thickness = 14, label, sub, className }: { data: [string, number][]; size?: number; thickness?: number; label?: string; sub?: string; className?: string }) {
  const total = data.reduce((a, d) => a + d[1], 0)
  const r = (size - thickness) / 2
  const c = 2 * Math.PI * r
  let offset = 0
  return (
    <div className={cn('flex items-center gap-3', className)}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} className="shrink-0">
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="currentColor" strokeOpacity={0.08} strokeWidth={thickness} />
        {total > 0 && data.map(([name, n], i) => {
          const len = (n / total) * c
          const el = (
            <circle
              key={name}
              cx={size / 2}
              cy={size / 2}
              r={r}
              fill="none"
              stroke={PALETTE[i % PALETTE.length]}
              strokeWidth={thickness}
              strokeDasharray={`${len} ${c - len}`}
              strokeDashoffset={-offset}
              transform={`rotate(-90 ${size / 2} ${size / 2})`}
            >
              <title>{`${name}: ${n}`}</title>
            </circle>
          )
          offset += len
          return el
        })}
        {label && (
          <text x="50%" y={sub ? '46%' : '50%'} dominantBaseline="middle" textAnchor="middle" className="fill-current text-base font-semibold">
            {label}
          </text>
        )}
        {sub && (
          <text x="50%" y="60%" dominantBaseline="middle" textAnchor="middle" className="fill-current text-[9px] opacity-60">
            {sub}
          </text>
        )}
      </svg>
      <ul className="text-xs space-y-1 min-w-0 flex-1">
        {data.slice(0, 6).map(([name, n], i) => (
          <li key={name} className="flex items-center gap-1.5 min-w-0">
            <span className="h-2 w-2 rounded-sm shrink-0" style={{ background: PALETTE[i % PALETTE.length] }} />
            <span className="truncate capitalize flex-1 min-w-0" title={name}>{name}</span>
            <span className="tabular-nums text-muted-foreground shrink-0">{total ? Math.round((n / total) * 100) : 0}%</span>
          </li>
        ))}
      </ul>
    </div>
  )
}

export function Bars({ data, height = 64, format, className }: { data: [string, number][]; height?: number; format: (n: number) => string; className?: string }) {
  const max = Math.max(1, ...data.map((d) => d[1]))
  return (
    <div className={cn('flex items-end gap-[3px]', className)} style={{ height }}>
      {data.map(([label, n]) => (
        <div key={label} className="flex-1 flex flex-col justify-end h-full" title={`${label}: ${format(n)}`}>
          <div className="rounded-sm bg-primary/70" style={{ height: Math.max(3, Math.round((n / max) * height)) }} />
        </div>
      ))}
    </div>
  )
}

/** Horizontal bars with labels: time by app, top people. */
export function HBars({ data, format, onClick, className }: { data: [string, number][]; format: (n: number) => string; onClick?: (label: string) => void; className?: string }) {
  const max = Math.max(1, ...data.map((d) => d[1]))
  return (
    <ul className={cn('space-y-1.5', className)}>
      {data.map(([label, n], i) => (
        <li key={label} className="text-xs">
          <div className="flex justify-between gap-2 mb-0.5">
            {onClick ? (
              <button className="truncate hover:underline text-left" onClick={() => onClick(label)}>{label}</button>
            ) : (
              <span className="truncate">{label}</span>
            )}
            <span className="tabular-nums text-muted-foreground shrink-0">{format(n)}</span>
          </div>
          <div className="h-1.5 rounded-full bg-muted overflow-hidden">
            <div className="h-full rounded-full" style={{ width: `${(n / max) * 100}%`, background: PALETTE[i % PALETTE.length] }} />
          </div>
        </li>
      ))}
    </ul>
  )
}

export function Stat({ n, label, hint, className }: { n: number | string; label: string; hint?: string; className?: string }) {
  return (
    <div className={cn('rounded-2xl bg-secondary/70 px-4 py-3.5', className)}>
      <div className="text-[26px] font-semibold tabular-nums leading-none tracking-tight">{n}</div>
      <div className="text-[12px] text-muted-foreground mt-1.5 flex items-center gap-1.5">
        {label}
        {hint && <span className="text-[11px] rounded-full bg-background px-1.5 py-0.5 text-foreground/70">{hint}</span>}
      </div>
    </div>
  )
}

/** A sparkline of bars, sized for the stat cards. */
export function Spark({ data, color, className }: { data: number[]; color: string; className?: string }) {
  const max = Math.max(1, ...data)
  return (
    <div className={cn('flex items-end gap-[2px] h-8', className)} aria-hidden="true">
      {data.map((n, i) => (
        <span key={i} className="w-[3px] rounded-[2px]" style={{ height: `${Math.max(12, (n / max) * 100)}%`, background: color, opacity: 0.35 + (i / Math.max(1, data.length - 1)) * 0.65 }} />
      ))}
    </div>
  )
}

const TONE = {
  blue: { card: 'tone-blue', chip: 'text-[#4b57d6]', bar: '#7b86f2' },
  violet: { card: 'tone-violet', chip: 'text-[#6a49dd]', bar: '#a78bfa' },
  amber: { card: 'tone-amber', chip: 'text-[#b87608]', bar: '#f0b34b' },
} as const

/** The headline numbers of the day: a tinted card, an icon, the figure, and
    a spark of the last two weeks where we actually have the series. */
export function StatCard({
  icon: Icon, tone, n, label, delta, series,
}: {
  icon: React.ComponentType<{ className?: string }>
  tone: keyof typeof TONE
  n: number | string
  label: string
  delta?: number | null
  series?: number[]
}) {
  const t = TONE[tone]
  return (
    <div className={cn('rounded-2xl p-4 flex items-center gap-3.5 min-w-0', t.card)}>
      <span className={cn('h-11 w-11 shrink-0 rounded-xl bg-background grid place-items-center', t.chip)}>
        <Icon className="h-[19px] w-[19px]" />
      </span>
      <div className="min-w-0 flex-1">
        <div className="text-[27px] font-semibold tabular-nums leading-none tracking-tight">{n}</div>
        <div className="text-[12.5px] text-muted-foreground mt-1 truncate">{label}</div>
        {delta != null && (
          <div className={cn('text-[11.5px] mt-1 font-medium tabular-nums', delta >= 0 ? 'text-emerald-600 dark:text-emerald-400' : 'text-rose-600 dark:text-rose-400')}>
            {delta >= 0 ? '↑' : '↓'} {Math.abs(delta)}%
          </div>
        )}
      </div>
      {series && series.length > 3 && <Spark data={series.slice(-14)} color={t.bar} className="shrink-0 w-[68px] justify-end" />}
    </div>
  )
}

/** Time by app, with a letter chip per app and its own bar underneath. */
export function AppRows({ data, format }: { data: [string, number][]; format: (n: number) => string }) {
  const max = Math.max(1, ...data.map((d) => d[1]))
  return (
    <ul className="space-y-2.5">
      {data.map(([name, n], i) => (
        <li key={name} className="min-w-0">
          <div className="flex items-center gap-2.5">
            <span className="h-7 w-7 shrink-0 rounded-lg grid place-items-center text-[11px] font-semibold text-white" style={{ background: PALETTE[i % PALETTE.length] }}>
              {name.charAt(0).toUpperCase()}
            </span>
            <span className="text-[13px] truncate flex-1">{name}</span>
            <span className="text-[12px] text-muted-foreground tabular-nums shrink-0">{format(n)}</span>
          </div>
          <div className="h-1.5 rounded-full bg-muted overflow-hidden mt-1.5 ml-[38px]">
            <div className="h-full rounded-full" style={{ width: `${(n / max) * 100}%`, background: PALETTE[i % PALETTE.length] }} />
          </div>
        </li>
      ))}
    </ul>
  )
}

/** The small ring beside "0 of 3 done". */
export function Ring({ done, total, size = 22 }: { done: number; total: number; size?: number }) {
  const r = (size - 3) / 2
  const c = 2 * Math.PI * r
  const p = total ? done / total : 0
  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden="true">
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="currentColor" strokeOpacity={0.15} strokeWidth={3} />
      <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="hsl(var(--primary))" strokeWidth={3} strokeLinecap="round"
        strokeDasharray={`${c * p} ${c}`} transform={`rotate(-90 ${size / 2} ${size / 2})`} />
    </svg>
  )
}
