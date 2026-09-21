import { useEffect, useState } from 'react'
import { check, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { Download, RefreshCw } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'

type State = 'idle' | 'checking' | 'none' | 'available' | 'installing' | 'error'

/**
 * Updates come from a signed manifest at lane.so, verified with the public
 * key built into the app. Nothing about the user is sent: the request
 * carries only the app's version, platform and architecture.
 */
export function UpdateCard({ auto = false }: { auto?: boolean }) {
  const [state, setState] = useState<State>('idle')
  const [update, setUpdate] = useState<Update | null>(null)
  const [detail, setDetail] = useState('')
  const [progress, setProgress] = useState<number | null>(null)

  async function lookForUpdate(quiet: boolean) {
    setState('checking')
    try {
      const u = await check({ timeout: 15_000 })
      if (u) {
        setUpdate(u)
        setState('available')
      } else {
        setState('none')
        if (!quiet) toast.success('Lane is up to date')
      }
    } catch (e) {
      setState('error')
      setDetail(String(e))
      if (!quiet) toast.error(`Could not check for updates: ${String(e)}`)
    }
  }

  useEffect(() => {
    if (auto) lookForUpdate(true)
  }, [auto])

  async function install() {
    if (!update) return
    setState('installing')
    let total = 0
    let done = 0
    try {
      await update.downloadAndInstall((ev) => {
        if (ev.event === 'Started') total = ev.data.contentLength ?? 0
        if (ev.event === 'Progress') {
          done += ev.data.chunkLength
          if (total) setProgress(Math.round((done / total) * 100))
        }
      })
      await relaunch()
    } catch (e) {
      setState('error')
      setDetail(String(e))
      toast.error(String(e))
    }
  }

  if (auto && state !== 'available') return null

  return (
    <div className={state === 'available' ? 'rounded-lg border border-primary/40 bg-primary/5 p-3' : ''}>
      {state === 'available' && update ? (
        <div className="flex items-start gap-3">
          <Download className="h-4 w-4 text-primary mt-0.5 shrink-0" />
          <div className="flex-1 text-sm">
            <p className="font-medium">Lane {update.version} is available</p>
            {update.body && <p className="text-xs text-muted-foreground mt-0.5 whitespace-pre-wrap">{update.body}</p>}
          </div>
          <Button size="sm" onClick={install}>Install and restart</Button>
        </div>
      ) : state === 'installing' ? (
        <p className="text-sm text-muted-foreground">Installing{progress != null ? ` · ${progress}%` : '…'} Lane restarts when done.</p>
      ) : (
        <div className="flex items-center gap-3">
          <Button size="sm" variant="outline" disabled={state === 'checking'} onClick={() => lookForUpdate(false)}>
            <RefreshCw className={`h-3.5 w-3.5 mr-1.5 ${state === 'checking' ? 'animate-spin' : ''}`} /> Check for updates
          </Button>
          <span className="text-xs text-muted-foreground">
            {state === 'none' && 'Up to date.'}
            {state === 'error' && `Could not check (${detail.length > 80 ? detail.slice(0, 80) + '…' : detail}).`}
            {state === 'idle' && 'Signed updates from lane.so; only the app version is sent.'}
          </span>
        </div>
      )}
    </div>
  )
}
