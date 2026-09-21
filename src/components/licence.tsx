import { useEffect, useState } from 'react'
import { KeyRound, Loader2, ShieldCheck } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { api, type Licence } from '@/lib/api'
import { cn } from '@/lib/utils'

const BUY = 'https://lane.so/#access'

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

/** Shown once the seven weeks are up. Nothing is deleted; Lane simply stops. */
export function LicenceWall({ lic, onApplied }: { lic: Licence; onApplied: (l: Licence) => void }) {
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-background/90 backdrop-blur-xl p-6">
      <div className="w-full max-w-lg rounded-3xl border bg-card p-8 shadow-2xl">
        <span className="h-12 w-12 rounded-2xl tone-violet grid place-items-center text-primary"><ShieldCheck className="h-6 w-6" /></span>
        <h1 className="text-[26px] font-semibold tracking-tight mt-5">Your seven weeks are up</h1>
        <p className="text-[14.5px] text-muted-foreground mt-2 leading-relaxed">
          Lane has stopped reading and stopped making memories. Nothing has been deleted: everything you have is still
          here, still on this Mac, and comes back the moment you unlock it.
        </p>
        <div className="mt-6 grid gap-2.5">
          <a href={BUY} target="_blank" rel="noreferrer" className="h-11 rounded-xl bg-primary text-primary-foreground grid place-items-center text-[14px] font-medium hover:brightness-110">
            $9 a month · $90 a year · $500 once
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

/** The card in Settings. */
export function LicenceCard({ lic, onChanged }: { lic: Licence; onChanged: (l: Licence) => void }) {
  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3 flex-wrap">
        <span className={cn('h-10 w-10 rounded-xl grid place-items-center', lic.state === 'licensed' ? 'bg-emerald-50 text-emerald-600 dark:bg-emerald-500/15 dark:text-emerald-400' : 'tone-violet text-primary')}>
          <ShieldCheck className="h-5 w-5" />
        </span>
        <div className="min-w-0">
          <div className="text-[15px] font-medium">
            {lic.state === 'licensed' ? `Licensed · ${lic.plan}` : lic.state === 'trial' ? `Trial · ${lic.daysLeft} ${lic.daysLeft === 1 ? 'day' : 'days'} left` : 'Trial ended'}
          </div>
          <div className="text-[12.5px] text-muted-foreground">
            {lic.state === 'licensed'
              ? (lic.email.includes('@') ? lic.email : 'Thank you. Lane is yours.')
              : 'Seven weeks, everything switched on, no card to start.'}
          </div>
        </div>
        {lic.state === 'licensed' && (
          <Button size="sm" variant="ghost" className="ml-auto text-muted-foreground" onClick={() => api.clearLicence().then(onChanged)}>
            Remove from this Mac
          </Button>
        )}
      </div>
      {lic.state !== 'licensed' && (
        <>
          <a href={BUY} target="_blank" rel="noreferrer" className="inline-flex h-10 items-center rounded-xl bg-primary px-4 text-[13.5px] font-medium text-primary-foreground hover:brightness-110">
            $9 a month · $90 a year · $500 once
          </a>
          <LicenceKey onApplied={onChanged} />
        </>
      )}
    </div>
  )
}
