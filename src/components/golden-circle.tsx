import { useEffect, useRef, useState } from 'react'
import { format } from 'date-fns'
import { toast } from 'sonner'
import { api, type Circle } from '@/lib/api'

/**
 * The golden circle: the why at the centre, the how as the middle ring,
 * the what (projects) as the outer ring, and the month's memories as dots
 * placed by how close they sit to the why. Distance is meaning.
 */
export function GoldenCircle({ onOpen, onWrite }: { onOpen?: (activityId: number) => void; onWrite?: () => void }) {
  const [data, setData] = useState<Circle | null>(null)
  const [hover, setHover] = useState<Circle['dots'][number] | null>(null)
  const canvas = useRef<HTMLCanvasElement>(null)
  const layout = useRef<{ x: number; y: number; r: number; dot: Circle['dots'][number] }[]>([])

  useEffect(() => {
    api.circle().then(setData).catch((e) => toast.error(String(e)))
    const un = api.onSignalsChanged(() => api.circle().then(setData).catch(() => {}))
    return () => { un.then((f) => f()) }
  }, [])

  useEffect(() => {
    const c = canvas.current
    if (!c || !data) return
    const dpr = Math.min(2, window.devicePixelRatio || 1)
    const W = c.clientWidth, H = Math.max(300, Math.min(420, c.clientWidth * 0.46))
    c.width = W * dpr; c.height = H * dpr; c.style.height = `${H}px`
    const ctx = c.getContext('2d')!
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.clearRect(0, 0, W, H)
    const dark = document.documentElement.classList.contains('dark') || window.matchMedia('(prefers-color-scheme: dark)').matches
    const ink = dark ? '#F1F2EE' : '#15181A'
    const moss = dark ? '#4FA57C' : '#1F5C43'
    const cx = W / 2, cy = H / 2, R = Math.min(W, H) / 2 - 24
    // Rings: why (0.22 R), how (0.55 R), what (R)
    for (const [k, label] of [[1, 'What'], [0.55, 'How'], [0.22, 'Why']] as [number, string][]) {
      ctx.beginPath(); ctx.arc(cx, cy, R * k, 0, Math.PI * 2)
      ctx.strokeStyle = ink; ctx.globalAlpha = k === 0.22 ? 0.35 : 0.12; ctx.lineWidth = k === 0.22 ? 1.2 : 0.8; ctx.stroke()
      ctx.globalAlpha = 0.5; ctx.fillStyle = ink; ctx.font = '600 10px ui-sans-serif, system-ui'; ctx.textAlign = 'center'
      ctx.fillText(label.toUpperCase(), cx, cy - R * k + 12)
    }
    ctx.globalAlpha = 1
    // Projects around the outer ring
    const projects = data.projects.slice(0, 8)
    const angleOf = (name: string | null, i: number) => {
      const idx = name ? projects.findIndex(([p]) => p === name) : -1
      if (idx >= 0) return (idx / Math.max(1, projects.length)) * Math.PI * 2 - Math.PI / 2
      return ((i * 137.5) % 360) * (Math.PI / 180)
    }
    ctx.font = '500 11px ui-sans-serif, system-ui'
    projects.forEach(([p, n], i) => {
      const a = (i / projects.length) * Math.PI * 2 - Math.PI / 2
      const x = cx + Math.cos(a) * (R + 12), y = cy + Math.sin(a) * (R + 12)
      ctx.fillStyle = ink; ctx.globalAlpha = 0.7
      ctx.textAlign = Math.cos(a) > 0.2 ? 'left' : Math.cos(a) < -0.2 ? 'right' : 'center'
      ctx.fillText(`${p} · ${n}`, x, y + 4)
    })
    ctx.globalAlpha = 1
    // Dots: radius from alignment (near = close to centre), angle from project or a spread
    layout.current = []
    data.dots.forEach((d, i) => {
      const dist = R * (0.24 + (1 - Math.min(1, d.alignment / 0.7)) * 0.72)
      const a = angleOf(d.project, i) + ((i % 7) - 3) * 0.06
      const x = cx + Math.cos(a) * dist, y = cy + Math.sin(a) * dist
      const near = d.alignment >= 0.45
      ctx.beginPath(); ctx.arc(x, y, near ? 3.2 : 2.2, 0, Math.PI * 2)
      ctx.fillStyle = near ? moss : ink; ctx.globalAlpha = near ? 0.9 : 0.35; ctx.fill()
      layout.current.push({ x, y, r: 6, dot: d })
    })
    ctx.globalAlpha = 1
    // The why, centred
    ctx.fillStyle = ink; ctx.textAlign = 'center'; ctx.font = '500 12px ui-serif, Georgia, serif'
    const why = data.why || 'Write your why'
    const words = why.split(' '); const lines: string[] = []; let cur = ''
    for (const w of words) { if ((cur + ' ' + w).trim().length > 26) { lines.push(cur.trim()); cur = w } else cur += ' ' + w; if (lines.length >= 3) break }
    if (cur && lines.length < 3) lines.push(cur.trim())
    lines.forEach((l, i) => ctx.fillText(l, cx, cy - (lines.length - 1) * 7 + i * 14))
  }, [data])

  function onMove(e: React.MouseEvent<HTMLCanvasElement>) {
    const rect = (e.target as HTMLCanvasElement).getBoundingClientRect()
    const x = e.clientX - rect.left, y = e.clientY - rect.top
    const hit = layout.current.find((p) => Math.hypot(p.x - x, p.y - y) <= p.r)
    setHover(hit?.dot ?? null)
  }

  if (!data) return null
  const near = data.dots.filter((d) => d.alignment >= 0.45).length
  return (
    <section className="rounded-2xl border bg-card p-4">
      <div className="flex items-baseline justify-between gap-3 mb-1">
        <h2 className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">The circle · 30 days</h2>
        <span className="text-xs text-muted-foreground tabular-nums">{data.why ? `${near} of ${data.dots.length} memories near your why` : 'No why yet'}</span>
      </div>
      {!data.why && (
        <p className="text-sm text-muted-foreground mb-2">Write your why on Today and every memory finds its place here: close to the centre when it serves it, far when it doesn't. {onWrite && <button className="underline" onClick={onWrite}>Write it</button>}</p>
      )}
      <canvas ref={canvas} className="w-full cursor-crosshair" onMouseMove={onMove} onMouseLeave={() => setHover(null)} onClick={() => hover && hover.activityId && onOpen?.(hover.activityId)} />
      <div className="h-9 text-xs text-muted-foreground mt-1 truncate">
        {hover ? `${hover.title} · ${format(hover.at, 'd MMM')} · ${Math.round(hover.alignment * 100)}% near${hover.project ? ` · ${hover.project}` : ''}` : data.how.length ? `How · ${data.how.join(' · ')}` : ''}
      </div>
      {data.days.length > 1 && (
        <div className="flex items-end gap-[3px] h-10 mt-1" title="Each day: how close its memories sat to your why">
          {data.days.map(([day, score, m]) => (
            <div key={day} className="flex-1 flex flex-col justify-end" title={`${day}: ${Math.round(score * 100)}% · ${m} memories`}>
              <div className="rounded-sm bg-primary/70" style={{ height: `${Math.max(2, score * 100)}%` }} />
            </div>
          ))}
        </div>
      )}
    </section>
  )
}
