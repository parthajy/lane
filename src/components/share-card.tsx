import { useEffect, useRef, useState } from 'react'
import { Download, Share2 } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { api, formatDuration, type Explore } from '@/lib/api'

const HUES = ['#5a51e5', '#8b5cf6', '#2b9cf3', '#f5a524', '#f2555a', '#4f7bf5', '#a78bfa', '#8b8ba7']

/**
 * A month of your own attention, drawn to a canvas so it can leave the app as
 * a picture. Nothing is uploaded: the image is written to your Desktop and it
 * is yours to post or not.
 */
export function ShareCard({ data }: { data: Explore }) {
  const ref = useRef<HTMLCanvasElement>(null)
  const [saving, setSaving] = useState(false)
  const cats = data.byCategory.slice(0, 5)
  const places = data.topPlaces.slice(0, 5)
  const totalMin = data.byCategory.reduce((a, c) => a + c[1], 0)

  useEffect(() => {
    const c = ref.current
    if (!c) return
    const W = 1000, H = 620, dpr = Math.min(2, window.devicePixelRatio || 1)
    c.width = W * dpr; c.height = H * dpr
    c.style.width = '100%'; c.style.aspectRatio = `${W} / ${H}`
    const g = c.getContext('2d')!
    g.setTransform(dpr, 0, 0, dpr, 0, 0)

    const sky = g.createLinearGradient(0, 0, W, H)
    sky.addColorStop(0, '#eceaff'); sky.addColorStop(0.55, '#f4effc'); sky.addColorStop(1, '#fdf1e9')
    g.fillStyle = sky; g.fillRect(0, 0, W, H)

    g.fillStyle = '#101014'
    g.font = '600 46px "Inter Tight", Inter, system-ui, sans-serif'
    g.fillText('My last 30 days', 56, 96)
    g.fillStyle = '#5b5b66'
    g.font = '400 20px Inter, system-ui, sans-serif'
    g.fillText(`${formatDuration(totalMin * 60000) || '0m'} of work remembered · ${data.memories.toLocaleString()} memories · all on my own Mac`, 56, 132)

    // categories
    g.fillStyle = '#8c8c97'
    g.font = '500 13px "JetBrains Mono", ui-monospace, monospace'
    g.fillText('WHERE THE TIME WENT', 56, 196)
    const max = Math.max(1, ...cats.map((c) => c[1]))
    cats.forEach(([label, mins], i) => {
      const y = 224 + i * 54
      g.fillStyle = '#101014'
      g.font = '500 19px Inter, system-ui, sans-serif'
      g.fillText(label, 56, y + 18)
      g.fillStyle = '#5b5b66'
      g.font = '400 16px Inter, system-ui, sans-serif'
      const t = formatDuration(mins * 60000)
      g.fillText(t, 470 - g.measureText(t).width, y + 18)
      g.fillStyle = 'rgba(16,16,26,.07)'
      g.beginPath(); g.roundRect(56, y + 28, 414, 8, 4); g.fill()
      g.fillStyle = HUES[i % HUES.length]
      g.beginPath(); g.roundRect(56, y + 28, Math.max(8, (mins / max) * 414), 8, 4); g.fill()
    })

    // places
    g.fillStyle = '#8c8c97'
    g.font = '500 13px "JetBrains Mono", ui-monospace, monospace'
    g.fillText('WHERE I ACTUALLY WAS', 560, 196)
    places.forEach(([name, cat, mins], i) => {
      const y = 224 + i * 54
      g.fillStyle = '#ffffff'
      g.beginPath(); g.roundRect(560, y - 4, 384, 46, 12); g.fill()
      g.fillStyle = HUES[i % HUES.length]
      g.beginPath(); g.arc(584, y + 19, 7, 0, 6.2832); g.fill()
      g.fillStyle = '#101014'
      g.font = '500 17px Inter, system-ui, sans-serif'
      g.fillText(name.length > 26 ? `${name.slice(0, 25)}…` : name, 602, y + 17)
      g.fillStyle = '#8c8c97'
      g.font = '400 13px Inter, system-ui, sans-serif'
      g.fillText(cat, 602, y + 34)
      g.fillStyle = '#5b5b66'
      g.font = '400 15px Inter, system-ui, sans-serif'
      const t = formatDuration(mins * 60000)
      g.fillText(t, 926 - g.measureText(t).width, y + 26)
    })

    g.fillStyle = '#5a51e5'
    g.font = '600 20px "Inter Tight", Inter, system-ui, sans-serif'
    g.fillText('Lane', 56, H - 48)
    g.fillStyle = '#8c8c97'
    g.font = '400 16px Inter, system-ui, sans-serif'
    g.fillText('a memory that never leaves your Mac · lane.so', 112, H - 48)
  }, [data, cats, places, totalMin])

  async function save() {
    const c = ref.current
    if (!c) return
    setSaving(true)
    try {
      const path = await api.savePng(`Lane · my last 30 days`, c.toDataURL('image/png'))
      toast.success('Saved to your Desktop', { description: path, action: { label: 'Show', onClick: () => api.revealFile(path) } })
    } catch (e) {
      toast.error(String(e))
    } finally {
      setSaving(false)
    }
  }

  return (
    <section className="rounded-2xl border bg-card p-5">
      <div className="flex items-center gap-2 mb-4">
        <Share2 className="h-4 w-4 text-primary" />
        <h2 className="text-[10.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground">A card you can share</h2>
        <Button size="sm" variant="outline" className="ml-auto" disabled={saving} onClick={save}>
          <Download className="h-3.5 w-3.5 mr-1.5" /> Save image
        </Button>
      </div>
      <canvas ref={ref} className="w-full rounded-xl border" />
      <p className="text-[12px] text-muted-foreground mt-3">
        Drawn here on your Mac and saved to your Desktop. Nothing is uploaded; what you post is your decision.
      </p>
    </section>
  )
}
