import { useState } from 'react'
import { Play, X } from 'lucide-react'
import { api } from '@/lib/api'

/**
 * What Lane is, in two videos.
 *
 * The videos are on YouTube and they are watched there. Nothing here embeds a
 * player and nothing loads a thumbnail: both would be a request to Google
 * from a window whose whole promise is that nothing leaves this Mac. A click
 * hands the link to the browser, where the user already decides such things
 * for themselves.
 */
const FILMS = [
  {
    id: 'tA4UnSGg3j4',
    title: 'What Lane is, and why it exists',
    body: 'The idea: a memory of your working day that is yours, kept on your own machine.',
    length: 'Partha, the founder · 5 min',
  },
  {
    id: 'PXPtb4Z2GQA',
    title: 'A walkthrough of the app',
    body: 'Capture, the notch, asking a question, meetings and dictation — what each one is for and when to reach for it.',
    length: 'Screen recording · 17 min',
  },
]

const SEEN = 'lane.intro-videos.hidden'

export function IntroVideos() {
  const [hidden, setHidden] = useState(() => {
    try { return localStorage.getItem(SEEN) === '1' } catch { return false }
  })
  if (hidden) return null

  function dismiss() {
    try { localStorage.setItem(SEEN, '1') } catch { /* a private window; the card simply returns */ }
    setHidden(true)
  }

  return (
    <div className="relative rounded-2xl border border-primary/25 bg-primary/[0.04] p-4 mb-4">
      <button
        onClick={dismiss}
        aria-label="Hide this"
        className="absolute right-3 top-3 rounded-md p-1 text-muted-foreground hover:bg-secondary hover:text-foreground"
      >
        <X className="h-3.5 w-3.5" />
      </button>

      <p className="text-[13px] font-medium mb-0.5">New to Lane?</p>
      <p className="text-[13px] text-muted-foreground mb-3 max-w-[70ch]">
        Two short films on what Lane is for and how to drive it. They open in your browser.
      </p>

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
        {FILMS.map((f) => (
          <button
            key={f.id}
            onClick={() => api.openLink(`https://youtu.be/${f.id}`)}
            className="group flex items-start gap-3 rounded-xl border bg-background p-3 text-left hover:border-primary/40 hover:bg-secondary/40 transition-colors"
          >
            <span className="mt-0.5 grid h-8 w-8 shrink-0 place-items-center rounded-full bg-primary/12 text-primary group-hover:bg-primary group-hover:text-primary-foreground transition-colors">
              <Play className="h-3.5 w-3.5 translate-x-[1px]" fill="currentColor" />
            </span>
            <span className="min-w-0">
              <span className="block text-[13.5px] font-medium leading-snug">{f.title}</span>
              <span className="block text-[12.5px] text-muted-foreground leading-snug mt-0.5">{f.body}</span>
              <span className="block text-[11.5px] text-muted-foreground/80 mt-1">{f.length}</span>
            </span>
          </button>
        ))}
      </div>
    </div>
  )
}
