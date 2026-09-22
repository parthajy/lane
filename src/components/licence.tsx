import { useEffect, useState } from 'react'
import { KeyRound, Loader2, ShieldCheck } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { api, type Licence } from '@/lib/api'
import { cn } from '@/lib/utils'

const BUY = 'https://lane.so/buy'

export function useLicence() {
  const [lic, setLic] = useState<Licence | null>(null)
  useEffect(() => {
    const read = () => api.licence().then(setLic).catch(() => {})
    read()
    const un = api.onLicence(read)
    const t = setInterval(read, 5 * 60_000)
    return () => { un.then((f) => f()); clearInterval(t) }
  }, [])
  return [lic, setLic] as const
}

/** The key box: the same control in Settings and behind the wall. */
export function LicenceKey({ onApplied, compact }: { onApplied: (l: Licence) => void; compact?: boolean }) {
  const [key, setKey] = useState('')
  const [busy, setBusy] = useState(false)
  async function apply() {
    setBusy(true)
    try {
      const l = await api.applyLicence(key)
      onApplied(l)
      setKey('')
      toast.success('Thank you. Lane is yours.')
    } catch (e) {
      toast.error(String(e))
    } finally {
      setBusy(false)
    }
  }
  return (
    <div className={cn('flex gap-2', compact ? 'flex-col' : 'items-center')}>
      <input
        value={key}
        onChange={(e) => setKey(e.target.value)}
        placeholder="Paste the key from your receipt"
        onKeyDown={(e) => { if (e.key === 'Enter' && key.trim()) apply() }}
        className="flex-1 min-w-0 h-11 rounded-xl border bg-background px-3.5 text-[13px] font-mono outline-none focus:ring-2 focus:ring-ring"
      />
      <Button disabled={busy || !key.trim()} onClick={apply}>
        {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <><KeyRound className="h-4 w-4 mr-1.5" /> Unlock</>}
      </Button>
    </div>
  )
}

/** Shown once the two months are up. Nothing is deleted; Lane simply stops. */
export function LicenceWall({ lic, onApplied }: { lic: Licence; onApplied: (l: Licence) => void }) {
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-background/90 backdrop-blur-xl p-6">
      <div className="w-full max-w-lg rounded-3xl border bg-card p-8 shadow-2xl">
        <span className="h-12 w-12 rounded-2xl tone-violet grid place-items-center text-primary"><ShieldCheck className="h-6 w-6" /></span>
        <h1 className="text-[26px] font-semibold tracking-tight mt-5">Your two months are up</h1>
        <p className="text-[14.5px] text-muted-foreground mt-2 leading-relaxed">
          Lane has stopped reading and stopped making memories. Nothing has been deleted: everything you have is still
          here, still on this Mac, and comes back the moment you unlock it.
        </p>
        <div className="mt-6 grid gap-2.5">
          <a href={BUY} target="_blank" rel="noreferrer" className="h-11 rounded-xl bg-primary text-primary-foreground grid place-items-center text-[14px] font-medium hover:brightness-110">
            $9 a month · $89 a year · $499 once
          </a>
          <LicenceKey onApplied={onApplied} compact />
        </div>
        <p className="text-[12px] text-muted-foreground mt-5">
          A key is a signed note checked on this Mac. There is no account, and nothing about you is sent anywhere.
        </p>
      </div>
    </div>
  )
}

/** What each plan is called, and what it costs. */
const PLANS = [
  { id: 'lifetime', name: 'Lifetime', price: '$499', per: 'once', note: 'Only 200 spots' },
  { id: 'yearly', name: 'Yearly', price: '$89', per: 'a year', note: 'Two months off' },
  { id: 'monthly', name: 'Monthly', price: '$9', per: 'a month', note: '' },
] as const

/** The card in Settings. */
export function LicenceCard({ lic, onChanged }: { lic: Licence; onChanged: (l: Licence) => void }) {
  const licensed = lic.state === 'licensed'
  const plan = PLANS.find((p) => p.id === lic.plan)

  if (licensed) {
    return (
      <div className="space-y-4">
        <div className="rounded-2xl bg-emerald-50 dark:bg-emerald-500/10 p-4 flex items-center gap-3.5 flex-wrap">
          <span className="h-11 w-11 shrink-0 rounded-xl bg-background grid place-items-center text-emerald-600 dark:text-emerald-400">
            <ShieldCheck className="h-[22px] w-[22px]" />
          </span>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2 flex-wrap">
              <span className="text-[15.5px] font-semibold">{plan?.name ?? 'Licensed'}</span>
              {plan && (
                <span className="rounded-full bg-emerald-600/10 text-emerald-700 dark:text-emerald-400 px-2 py-0.5 text-[11px] font-medium">
                  {plan.id === 'lifetime' ? 'Yours for good' : `Renews ${plan.per}`}
                </span>
              )}
            </div>
            <div className="text-[12.5px] text-muted-foreground mt-0.5 truncate">
              {lic.email.includes('@') ? lic.email : 'Thank you. Lane is yours.'}
            </div>
          </div>
          <Button size="sm" variant="ghost" className="text-muted-foreground" onClick={() => api.clearLicence().then(onChanged)}>
            Remove from this Mac
          </Button>
        </div>
        <p className="text-[12.5px] text-muted-foreground">
          Checked on this Mac. No account, and nothing about your licence is ever sent anywhere.
        </p>
      </div>
    )
  }

  const ended = lic.state === 'expired'
  return (
    <div className="space-y-5">
      <div className={cn('rounded-2xl p-4 flex items-center gap-3.5', ended ? 'bg-rose-50 dark:bg-rose-500/10' : 'tone-violet')}>
        <span className={cn('h-11 w-11 shrink-0 rounded-xl bg-background grid place-items-center', ended ? 'text-rose-600 dark:text-rose-400' : 'text-primary')}>
          <ShieldCheck className="h-[22px] w-[22px]" />
        </span>
        <div className="min-w-0">
          <div className="text-[15.5px] font-semibold">
            {ended ? 'Your two months are up' : `${lic.daysLeft} ${lic.daysLeft === 1 ? 'day' : 'days'} left of your trial`}
          </div>
          <div className="text-[12.5px] text-muted-foreground mt-0.5">
            {ended
              ? 'Nothing has been deleted. Everything comes back the moment you unlock it.'
              : 'Two months, everything switched on, no card to start.'}
          </div>
        </div>
      </div>

      <div className="grid gap-2 sm:grid-cols-3">
        {PLANS.map((p) => (
          <a
            key={p.id}
            href={`${BUY}/${p.id}`}
            target="_blank"
            rel="noreferrer"
            className={cn(
              'rounded-xl border p-3.5 transition hover:-translate-y-0.5 hover:shadow-sm',
              p.id === 'lifetime' ? 'border-primary/35 bg-accent/40' : 'hover:bg-secondary',
            )}
          >
            <div className="flex items-center justify-between gap-2">
              <span className="text-[13px] font-medium">{p.name}</span>
              {p.note && <span className="rounded-full bg-primary/10 text-primary px-1.5 py-0.5 text-[10px] font-medium">{p.note}</span>}
            </div>
            <div className="mt-1.5 text-[22px] font-semibold tracking-tight leading-none">
              {p.price}
              <span className="text-[12px] font-normal text-muted-foreground ml-1.5">{p.per}</span>
            </div>
          </a>
        ))}
      </div>

      <div className="space-y-2">
        <p className="text-[12.5px] text-muted-foreground">
          Bought already? Paste the key from your receipt, or press Open in Lane on the page you were sent to.
        </p>
        <LicenceKey onApplied={onChanged} />
      </div>
    </div>
  )
}
