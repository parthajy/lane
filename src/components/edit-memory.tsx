import { useEffect, useState } from 'react'
import { format } from 'date-fns'
import { AlertTriangle, Check, Plus, Trash2, User } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { api, type Fact, type MemoryCard } from '@/lib/api'

const split = (s: string) => s.split(/[,\n]/).map((x) => x.trim()).filter(Boolean)

/** Edit a memory in your own words. Your version replaces the model's and is never remade. */
export function EditMemoryDialog({ card, onClose, onSaved }: { card: MemoryCard | null; onClose: () => void; onSaved: (c: MemoryCard) => void }) {
  const [title, setTitle] = useState('')
  const [summary, setSummary] = useState('')
  const [people, setPeople] = useState('')
  const [orgs, setOrgs] = useState('')
  const [projects, setProjects] = useState('')
  const [decisions, setDecisions] = useState('')
  const [facts, setFacts] = useState<Fact[]>([])
  const [newFact, setNewFact] = useState({ subject: '', attribute: '', value: '' })
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    if (!card) return
    setTitle(card.title)
    setSummary(card.summary)
    setPeople(card.people.join(', '))
    setOrgs(card.organizations.join(', '))
    setProjects(card.projects.join(', '))
    setDecisions(card.decisions.join('\n'))
    setFacts(card.facts)
    setNewFact({ subject: '', attribute: '', value: '' })
  }, [card])

  const reloadFacts = () => card && api.memoryFacts(card.ids).then(setFacts).catch(() => {})

  async function save() {
    if (!card) return
    setSaving(true)
    try {
      const saved = await api.updateMemory(card.id, {
        title, summary, people: split(people), organizations: split(orgs), projects: split(projects), decisions: split(decisions),
      })
      toast.success('Saved. This memory is yours now; Rabbit will not rewrite it.')
      onSaved(saved)
      onClose()
    } catch (e) {
      toast.error(String(e))
    } finally {
      setSaving(false)
    }
  }

  async function addFact() {
    if (!card) return
    try {
      await api.addFact(card.id, newFact.subject, newFact.attribute, newFact.value)
      setNewFact({ subject: '', attribute: '', value: '' })
      reloadFacts()
    } catch (e) {
      toast.error(String(e))
    }
  }

  return (
    <Dialog open={card != null} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-2xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Edit memory</DialogTitle>
          <DialogDescription>Your words replace Rabbit's. Facts you confirm or correct are used first when Lane answers.</DialogDescription>
        </DialogHeader>
        <div className="space-y-3">
          <div className="space-y-1.5">
            <Label htmlFor="em-title">Title</Label>
            <Input id="em-title" value={title} onChange={(e) => setTitle(e.target.value)} />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="em-summary">Summary</Label>
            <Textarea id="em-summary" rows={3} value={summary} onChange={(e) => setSummary(e.target.value)} />
          </div>
          <div className="grid grid-cols-3 gap-3">
            <div className="space-y-1.5">
              <Label htmlFor="em-people">People</Label>
              <Input id="em-people" value={people} onChange={(e) => setPeople(e.target.value)} placeholder="comma separated" />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="em-orgs">Organisations</Label>
              <Input id="em-orgs" value={orgs} onChange={(e) => setOrgs(e.target.value)} placeholder="comma separated" />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="em-projects">Projects</Label>
              <Input id="em-projects" value={projects} onChange={(e) => setProjects(e.target.value)} placeholder="comma separated" />
            </div>
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="em-decisions">Decisions (one per line)</Label>
            <Textarea id="em-decisions" rows={2} value={decisions} onChange={(e) => setDecisions(e.target.value)} />
          </div>

          <div className="space-y-1.5 border-t pt-3">
            <Label>Facts</Label>
            <p className="text-xs text-muted-foreground">Exact values from this memory. Correct a value or remove a wrong one; add what Rabbit missed.</p>
            <div className="space-y-1">
              {facts.map((f) => (
                <FactRow key={f.id} f={f} onChanged={reloadFacts} />
              ))}
              {facts.length === 0 && <p className="text-xs text-muted-foreground">No facts yet.</p>}
            </div>
            <div className="grid grid-cols-[1fr_1fr_1fr_auto] gap-2 items-end pt-1">
              <Input placeholder="subject, e.g. Vatsalya proposal" value={newFact.subject} onChange={(e) => setNewFact({ ...newFact, subject: e.target.value })} />
              <Input placeholder="attribute, e.g. budget" value={newFact.attribute} onChange={(e) => setNewFact({ ...newFact, attribute: e.target.value })} />
              <Input placeholder="value, e.g. ₹35 lakh" value={newFact.value} onChange={(e) => setNewFact({ ...newFact, value: e.target.value })} onKeyDown={(e) => e.key === 'Enter' && addFact()} />
              <Button size="sm" variant="outline" onClick={addFact} disabled={!newFact.subject.trim() || !newFact.attribute.trim() || !newFact.value.trim()}>
                <Plus className="h-3.5 w-3.5" />
              </Button>
            </div>
          </div>
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>Cancel</Button>
          <Button onClick={save} disabled={saving || !title.trim()}>Save</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function FactRow({ f, onChanged }: { f: Fact; onChanged: () => void }) {
  const [value, setValue] = useState(f.value)
  useEffect(() => setValue(f.value), [f.value])
  const dirty = value.trim() !== f.value
  return (
    <div className="text-sm">
      <div className="flex items-center gap-2">
        <span className="truncate min-w-0 flex-1">
          <span className="font-medium">{f.subject}</span> <span className="text-muted-foreground">· {f.attribute}:</span>
        </span>
        <Input className="h-7 w-44 text-sm" value={value} onChange={(e) => setValue(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && dirty && api.correctFact(f.id, value).then(onChanged).catch((e) => toast.error(String(e)))} />
        {dirty ? (
          <Button size="icon-sm" variant="ghost" title="Save correction" onClick={() => api.correctFact(f.id, value).then(onChanged).catch((e) => toast.error(String(e)))}>
            <Check className="h-3.5 w-3.5" />
          </Button>
        ) : (
          <Button size="icon-sm" variant="ghost" title="Wrong, remove" onClick={() => api.retractFact(f.id).then(onChanged).catch((e) => toast.error(String(e)))}>
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        )}
        {f.origin === 'user' && <User className="h-3 w-3 text-primary shrink-0" aria-label="Confirmed by you" />}
      </div>
      {f.conflicts.length > 0 && (
        <div className="text-xs text-amber-700 dark:text-amber-400 flex items-start gap-1 pl-1 mt-0.5">
          <AlertTriangle className="h-3 w-3 mt-0.5 shrink-0" />
          <span>
            Elsewhere: {f.conflicts.map((c) => `${c.value} (${format(c.asOf, 'd MMM')}, ${c.title})`).join('; ')}
          </span>
        </div>
      )}
    </div>
  )
}
