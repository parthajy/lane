import { useEffect, useState } from 'react'
import { CheckCircle2, ChevronRight, Loader2, RotateCw, ShieldAlert } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { api, type PermissionReport } from '@/lib/api'
import { cn } from '@/lib/utils'

/** Polls permission state. Fast while the user is in System Settings. */
export function usePermissions(intervalMs = 1000) {
  const [report, setReport] = useState<PermissionReport | null>(null)
  useEffect(() => {
    let alive = true
    const tick = () => api.permissions().then((r) => alive && setReport(r)).catch(() => {})
    tick()
    const t = setInterval(tick, intervalMs)
    return () => {
      alive = false
      clearInterval(t)
    }
  }, [intervalMs])
  return report
}

export type AccessState = 'on' | 'restart' | 'off'

export function accessState(r: PermissionReport | null): AccessState | null {
  if (!r) return null
  if (r.accessibilityWorks) return 'on'
  if (r.accessibilityTrusted) return 'restart'
  return 'off'
}

function Step({ n, children }: { n: number; children: React.ReactNode }) {
  return (
    <li className="flex gap-3">
      <span className="h-6 w-6 shrink-0 rounded-full border bg-card flex items-center justify-center text-xs font-semibold">
        {n}
      </span>
      <div className="pt-0.5 text-sm leading-relaxed">{children}</div>
    </li>
  )
}

/**
 * Everything a first-time Mac user needs to turn on Accessibility access:
 * exact steps, the exact name to look for, live detection, and fixes for
 * the ways it goes wrong.
 */
export function AccessibilityPanel({ report }: { report: PermissionReport | null }) {
  const state = accessState(report)
  const [opening, setOpening] = useState(false)
  const name = report?.listedAs ?? 'Lane'

  async function open() {
    setOpening(true)
    try {
      await api.requestAccessibility()
    } catch (e) {
      toast.error(String(e))
    } finally {
      setTimeout(() => setOpening(false), 1500)
    }
  }

  async function reset() {
    try {
      await api.resetAccessibility()
      toast.success('Old entry removed. Click “Open System Settings” to add Lane again.')
    } catch (e) {
      toast.error(String(e))
    }
  }

  if (state === 'on') {
    return (
      <div className="rounded-xl border border-primary/30 bg-primary/5 p-4 flex gap-3">
        <CheckCircle2 className="h-5 w-5 text-primary shrink-0 mt-0.5" />
        <div>
          <p className="font-medium">Access is on</p>
          <p className="text-sm text-muted-foreground mt-0.5">
            Lane can read window titles and on-screen text. You can turn this off any time in System Settings →
            Privacy &amp; Security → Accessibility.
          </p>
        </div>
      </div>
    )
  }

  if (state === 'restart') {
    return (
      <div className="rounded-xl border border-amber-500/40 bg-amber-500/5 p-4 flex gap-3">
        <RotateCw className="h-5 w-5 text-amber-600 shrink-0 mt-0.5" />
        <div className="space-y-2">
          <p className="font-medium">Almost there: restart Lane</p>
          <p className="text-sm text-muted-foreground">
            macOS says access is on, but it only takes effect after Lane restarts.
          </p>
          <Button size="sm" onClick={() => api.restartApp()}>
            Restart Lane
          </Button>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2 text-sm">
        <span className={cn('h-2 w-2 rounded-full', report ? 'bg-destructive animate-pulse-soft' : 'bg-muted-foreground')} />
        <span className="text-muted-foreground">
          {report ? 'Waiting for access… this updates by itself.' : 'Checking…'}
        </span>
      </div>

      {report?.staleGrant && (
        <div className="rounded-lg border border-amber-500/40 bg-amber-500/5 p-3 text-sm">
          <span className="font-medium">Lane was updated.</span> macOS ties this permission to the exact version of an app, so the
          switch you turned on earlier no longer applies. Lane has removed the old entry: click{' '}
          <span className="font-medium">Open System Settings</span> and turn the switch for <span className="font-semibold">{name}</span> on
          again. If it still shows as on, turn it off and on.
        </div>
      )}

      {report && !report.bundled && (
        <div className="rounded-lg border bg-muted/40 p-3 text-sm">
          <span className="font-medium">Development build.</span> macOS gives this permission to the app that started
          Lane, so look for <span className="font-semibold">{name}</span> in the list, not “Lane”.
        </div>
      )}

      <ol className="space-y-3">
        <Step n={1}>
          Click <span className="font-medium">Open System Settings</span> below. If macOS shows a pop-up about
          accessibility, choose <span className="font-medium">Open System Settings</span> there too.
        </Step>
        <Step n={2}>
          In <span className="font-medium">Privacy &amp; Security → Accessibility</span>, find{' '}
          <span className="font-semibold">{name}</span> and turn its switch on.
        </Step>
        <Step n={3}>If your Mac asks for your password or Touch ID, confirm. This is macOS asking, not Lane.</Step>
        <Step n={4}>Come back to this window. You don't need to click anything else.</Step>
      </ol>

      <Button onClick={open} disabled={opening}>
        {opening ? <Loader2 className="h-4 w-4 mr-2 animate-spin" /> : <ChevronRight className="h-4 w-4 mr-1" />}
        Open System Settings
      </Button>

      <details className="rounded-lg border p-3 text-sm group">
        <summary className="cursor-pointer font-medium flex items-center gap-2">
          <ShieldAlert className="h-4 w-4 text-muted-foreground" /> Not working?
        </summary>
        <ul className="mt-3 space-y-2.5 text-muted-foreground list-disc pl-5">
          <li>
            <span className="text-foreground">{name} isn't in the list:</span> click the <b>+</b> button under the list,
            go to Applications, choose {name}, then turn its switch on.
          </li>
          <li>
            <span className="text-foreground">The switch is already on but this screen still waits:</span> turn it off
            and on again. If that doesn't help, select {name}, click <b>−</b> to remove it, then click Open System
            Settings here again.
            {report?.bundled && (
              <Button size="sm" variant="outline" className="mt-2" onClick={reset}>
                Remove old entry for me
              </Button>
            )}
          </li>
          <li>
            <span className="text-foreground">The switch won't move:</span> click the lock or confirm with your
            password. On a Mac managed by your organisation, IT may need to allow it.
          </li>
          <li>
            <span className="text-foreground">Still stuck:</span>{' '}
            <button className="underline" onClick={() => api.restartApp()}>
              restart Lane
            </button>
            .
          </li>
        </ul>
      </details>
    </div>
  )
}
