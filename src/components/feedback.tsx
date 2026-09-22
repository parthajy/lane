import { useState } from 'react'
import { MessageSquareHeart, Send } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { api } from '@/lib/api'

const TO = 'pb@lane.so'

/**
 * Feedback without a server: Lane cannot reach the network, so it writes the
 * note for you and hands it to your mail app, or to the clipboard. Nothing is
 * attached and nothing is read from your memories.
 */
export function FeedbackDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (v: boolean) => void }) {
  const [kind, setKind] = useState<'idea' | 'problem' | 'praise'>('idea')
  const [text, setText] = useState('')
  const body = `${text}\n\n—\nLane ${import.meta.env.VITE_APP_VERSION ?? ''} · ${navigator.platform}`.trim()

  function send() {
    const subject = kind === 'problem' ? 'Lane · something is wrong' : kind === 'praise' ? 'Lane · this worked' : 'Lane · an idea'
    api.openFile(`mailto:${TO}?subject=${encodeURIComponent(subject)}&body=${encodeURIComponent(body)}`)
      .then(() => onOpenChange(false))
      .catch(() => toast.error('No mail app. Use Copy instead.'))
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2"><MessageSquareHeart className="h-4 w-4 text-primary" /> Tell us</DialogTitle>
        </DialogHeader>
        <div className="space-y-3">
          <div className="flex gap-2">
            {([['idea', 'An idea'], ['problem', 'Something is wrong'], ['praise', 'This worked']] as const).map(([id, label]) => (
              <button key={id} onClick={() => setKind(id)} className={`rounded-full border px-3 py-1.5 text-[13px] ${kind === id ? 'bg-accent/60 border-primary/35' : 'hover:bg-secondary'}`}>
                {label}
              </button>
            ))}
          </div>
          <textarea
            autoFocus
            value={text}
            onChange={(e) => setText(e.target.value)}
            rows={6}
            placeholder={kind === 'problem' ? 'What happened, and what did you expect instead?' : 'What would make Lane better for you?'}
            className="w-full rounded-xl border bg-background p-3 text-[14px] outline-none focus:ring-2 focus:ring-ring"
          />
          <p className="text-[12px] text-muted-foreground">
            This opens your mail app with the note written. Nothing is sent from Lane itself, and none of your memories
            are attached — say only what you want us to read.
          </p>
          <div className="flex gap-2">
            <Button disabled={!text.trim()} onClick={send}><Send className="h-3.5 w-3.5 mr-1.5" /> Open in Mail</Button>
            <Button variant="outline" disabled={!text.trim()} onClick={() => api.copyText(body).then(() => toast.success('Copied. Paste it wherever suits you.'))}>
              Copy instead
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
