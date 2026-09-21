import { useEffect, useState } from 'react'
import { api } from '@/lib/api'
import { cn } from '@/lib/utils'

/** Resolved once per app+site for the life of the window. */
const CACHE = new Map<string, string | null>()
const PENDING = new Map<string, Promise<string | null>>()

function hostname(url?: string | null) {
  if (!url) return ''
  try { return new URL(url).hostname.replace(/^www\./, '') } catch { return '' }
}

const HUES = ['#5a51e5', '#8b5cf6', '#2b9cf3', '#f5a524', '#f2555a', '#4f7bf5']

/**
 * The favicon of the site a memory came from, or the icon of the app it was
 * read in. Both are found on disk; nothing is fetched. Falls back to a
 * coloured letter so a row never looks broken.
 */
export function SourceIcon({ app, url, className, size = 16 }: { app: string; url?: string | null; className?: string; size?: number }) {
  const key = `${app}|${hostname(url)}`
  const [src, setSrc] = useState<string | null>(() => CACHE.get(key) ?? null)

  useEffect(() => {
    let alive = true
    if (CACHE.has(key)) { setSrc(CACHE.get(key) ?? null); return }
    let p = PENDING.get(key)
    if (!p) {
      p = api.sourceIcon(app, url ?? null).catch(() => null)
      PENDING.set(key, p)
    }
    p.then((v) => {
      CACHE.set(key, v ?? null)
      PENDING.delete(key)
      if (alive) setSrc(v ?? null)
    })
    return () => { alive = false }
  }, [key, app, url])

  if (src) {
    return <img src={src} alt="" width={size} height={size} className={cn('rounded-[4px] object-contain shrink-0', className)} style={{ width: size, height: size }} />
  }
  const letter = (hostname(url) || app || '?').charAt(0).toUpperCase()
  const hue = HUES[(letter.charCodeAt(0) || 0) % HUES.length]
  return (
    <span
      className={cn('rounded-[4px] grid place-items-center text-white font-semibold shrink-0', className)}
      style={{ width: size, height: size, background: hue, fontSize: Math.round(size * 0.6) }}
      aria-hidden="true"
    >
      {letter}
    </span>
  )
}
