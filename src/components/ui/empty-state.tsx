import type { LucideIcon } from 'lucide-react'
import { Button } from './button'
import { cn } from '@/lib/utils'

// Shared empty-state block. Use anywhere a page/panel can legitimately have
// no data yet. Every empty state should tell the user (a) what this screen
// is for and (b) the single best next action.
export function EmptyState({
  icon: Icon,
  title,
  description,
  action,
  secondary,
  className,
  tone = 'default',
}: {
  icon?: LucideIcon
  title: string
  description?: string
  action?: { label: string; href?: string; onClick?: () => void }
  secondary?: { label: string; href?: string; onClick?: () => void }
  className?: string
  tone?: 'default' | 'primary' | 'muted'
}) {
  const toneStyles =
    tone === 'primary'
      ? 'from-primary/5 to-transparent border-primary/20'
      : tone === 'muted'
        ? 'from-muted/30 to-transparent border-border'
        : 'from-muted/20 to-transparent border-border'

  return (
    <div
      className={cn(
        'rounded-2xl border bg-gradient-to-br flex flex-col items-center justify-center text-center px-6 py-10',
        toneStyles,
        className,
      )}
    >
      {Icon && (
        <div className="h-12 w-12 rounded-full bg-primary/10 flex items-center justify-center mb-3">
          <Icon className="h-5 w-5 text-primary" />
        </div>
      )}
      <h3 className="font-semibold text-base">{title}</h3>
      {description && (
        <p className="text-sm text-muted-foreground mt-1.5 max-w-md leading-relaxed">
          {description}
        </p>
      )}
      {(action || secondary) && (
        <div className="flex items-center gap-2 mt-4">
          {action && (
            action.href ? (
              <Button asChild size="sm">
                <a href={action.href}>{action.label}</a>
              </Button>
            ) : (
              <Button size="sm" onClick={action.onClick}>{action.label}</Button>
            )
          )}
          {secondary && (
            secondary.href ? (
              <Button asChild variant="ghost" size="sm">
                <a href={secondary.href}>{secondary.label}</a>
              </Button>
            ) : (
              <Button variant="ghost" size="sm" onClick={secondary.onClick}>
                {secondary.label}
              </Button>
            )
          )}
        </div>
      )}
    </div>
  )
}
