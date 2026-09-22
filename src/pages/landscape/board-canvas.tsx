'use client'

// The constellation, drawn rather than built.
//
// React Flow puts a DOM element on the page for every node it is given,
// which is right when you are dragging and linking a few hundred memories
// and impossible at a hundred thousand. So the whole graph is painted on one
// canvas underneath, and React Flow is only handed the memories near the
// viewport (see BUDGET in board-view). The two draw the same dot in the same
// place, so the hand-off is invisible: you pan over a painted sky, and the
// dots you could actually reach are real elements on top of it.
//
// Everything here is culled to the visible rectangle, so the cost of a frame
// follows what is on screen rather than what is in the database.

import { memo, useEffect, useRef } from 'react'
import { useStore } from '@xyflow/react'

export type CanvasPoint = { x: number; y: number; r: number; ink: string; dim?: boolean }
export type CanvasLink = { ax: number; ay: number; bx: number; by: number }

// Past this many visible links the web is a wash of light anyway, and the
// frame time matters more than the last few threads.
const MAX_LINKS = 12000

function draw(
  cv: HTMLCanvasElement,
  points: CanvasPoint[],
  links: CanvasLink[],
  tx: number, ty: number, zoom: number,
) {
  const dpr = Math.min(2, window.devicePixelRatio || 1)
  const w = cv.clientWidth, h = cv.clientHeight
  if (cv.width !== Math.round(w * dpr) || cv.height !== Math.round(h * dpr)) {
    cv.width = Math.round(w * dpr)
    cv.height = Math.round(h * dpr)
  }
  const ctx = cv.getContext('2d')
  if (!ctx) return
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  ctx.clearRect(0, 0, w, h)

  // The visible rectangle in the graph's own coordinates, with a margin so
  // nothing pops in at the edge.
  const m = 80 / zoom
  const x0 = (-tx) / zoom - m, y0 = (-ty) / zoom - m
  const x1 = (w - tx) / zoom + m, y1 = (h - ty) / zoom + m

  ctx.save()
  ctx.translate(tx, ty)
  ctx.scale(zoom, zoom)

  // Links first, as one path: a single stroke for thousands of threads.
  if (links.length) {
    let drawn = 0
    ctx.beginPath()
    for (let i = 0; i < links.length && drawn < MAX_LINKS; i++) {
      const l = links[i]
      if ((l.ax < x0 && l.bx < x0) || (l.ax > x1 && l.bx > x1)) continue
      if ((l.ay < y0 && l.by < y0) || (l.ay > y1 && l.by > y1)) continue
      ctx.moveTo(l.ax, l.ay)
      ctx.lineTo(l.bx, l.by)
      drawn++
    }
    ctx.strokeStyle = 'rgba(188, 205, 255, 0.45)'
    ctx.lineWidth = Math.max(0.35, 0.9 / zoom)
    ctx.stroke()
  }

  // Dots, gathered by colour so each one is a single fill rather than a
  // fill per memory. Two passes: a soft halo, then a bright core.
  const byInk = new Map<string, CanvasPoint[]>()
  for (let i = 0; i < points.length; i++) {
    const p = points[i]
    if (p.x < x0 || p.x > x1 || p.y < y0 || p.y > y1) continue
    const b = byInk.get(p.ink)
    if (b) b.push(p); else byInk.set(p.ink, [p])
  }

  for (const [ink, list] of byInk) {
    ctx.globalAlpha = 0.28
    ctx.fillStyle = ink
    ctx.beginPath()
    for (let i = 0; i < list.length; i++) {
      const p = list[i]
      ctx.moveTo(p.x + p.r * 2.1, p.y)
      ctx.arc(p.x, p.y, p.r * 2.1, 0, 6.2832)
    }
    ctx.fill()

    ctx.globalAlpha = 1
    ctx.beginPath()
    for (let i = 0; i < list.length; i++) {
      const p = list[i]
      ctx.moveTo(p.x + p.r, p.y)
      ctx.arc(p.x, p.y, p.r, 0, 6.2832)
    }
    ctx.fill()

    // A white centre, which is what makes a dot read as a star.
    ctx.fillStyle = '#ffffff'
    ctx.globalAlpha = 0.85
    ctx.beginPath()
    for (let i = 0; i < list.length; i++) {
      const p = list[i]
      const r = p.r * 0.42
      ctx.moveTo(p.x + r, p.y)
      ctx.arc(p.x, p.y, r, 0, 6.2832)
    }
    ctx.fill()
  }
  ctx.globalAlpha = 1
  ctx.restore()
}

export const ConstellationCanvas = memo(function ConstellationCanvas({
  points, links,
}: { points: CanvasPoint[]; links: CanvasLink[] }) {
  const ref = useRef<HTMLCanvasElement>(null)
  const frame = useRef<number | null>(null)
  const [tx, ty, zoom] = useStore((s) => s.transform)

  useEffect(() => {
    const cv = ref.current
    if (!cv) return
    // One draw per animation frame however many times the viewport changes.
    if (frame.current) cancelAnimationFrame(frame.current)
    frame.current = requestAnimationFrame(() => draw(cv, points, links, tx, ty, zoom))
    return () => { if (frame.current) cancelAnimationFrame(frame.current) }
  }, [points, links, tx, ty, zoom])

  useEffect(() => {
    const cv = ref.current
    if (!cv || !('ResizeObserver' in window)) return
    const ro = new ResizeObserver(() => draw(cv, points, links, tx, ty, zoom))
    ro.observe(cv)
    return () => ro.disconnect()
  }, [points, links, tx, ty, zoom])

  return <canvas ref={ref} className="rb-constellation" aria-hidden="true" />
})
