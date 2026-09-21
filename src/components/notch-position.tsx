import { cn } from '@/lib/utils'

export type NotchPosition = 'top-center' | 'top-left' | 'top-right' | 'bottom-center' | 'left' | 'right'

const SPOTS: { id: NotchPosition; label: string; style: React.CSSProperties }[] = [
  { id: 'top-left', label: 'Top left', style: { top: 4, left: 10 } },
  { id: 'top-center', label: 'Top centre (the notch)', style: { top: 0, left: '50%', transform: 'translateX(-50%)' } },
  { id: 'top-right', label: 'Top right', style: { top: 4, right: 10 } },
  { id: 'left', label: 'Left edge', style: { top: '50%', left: 0, transform: 'translateY(-50%)', width: 6, height: 28 } },
  { id: 'right', label: 'Right edge', style: { top: '50%', right: 0, transform: 'translateY(-50%)', width: 6, height: 28 } },
  { id: 'bottom-center', label: 'Bottom centre', style: { bottom: 10, left: '50%', transform: 'translateX(-50%)' } },
]

/** A little screen with six spots; the chosen one shows the tab. */
export function NotchPositionPicker({ value, onChange, className }: { value: NotchPosition; onChange: (p: NotchPosition) => void; className?: string }) {
  return (
    <div className={cn('flex items-center gap-4', className)}>
      <div className="relative w-[220px] h-[140px] rounded-lg border bg-muted/40 overflow-hidden shrink-0" aria-label="Screen">
        <div className="absolute top-0 left-0 right-0 h-3 bg-foreground/10" />
        <div className="absolute bottom-0 left-1/2 -translate-x-1/2 h-2 w-24 rounded-t bg-foreground/10" />
        {SPOTS.map((s) => (
          <button
            key={s.id}
            type="button"
            title={s.label}
            aria-label={s.label}
            onClick={() => onChange(s.id)}
            className={cn('absolute rounded-sm transition-colors', value === s.id ? 'bg-primary' : 'bg-foreground/25 hover:bg-foreground/50')}
            style={{ width: 28, height: 6, ...s.style }}
          />
        ))}
      </div>
      <div className="text-sm">
        <div className="font-medium">{SPOTS.find((s) => s.id === value)?.label}</div>
        <p className="text-xs text-muted-foreground mt-1 leading-snug">Click a spot. The tab hangs there on every Space and over full-screen apps; it opens into a card on hover.</p>
      </div>
    </div>
  )
}
