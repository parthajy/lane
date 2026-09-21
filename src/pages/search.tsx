import { useEffect, useRef, useState } from 'react'
import { Search as SearchIcon } from 'lucide-react'
import { toast } from 'sonner'
import { Input } from '@/components/ui/input'
import { EmptyState } from '@/components/ui/empty-state'
import { ActivityRow } from '@/components/activity-row'
import { ActivitySheet } from '@/components/activity-sheet'
import { api, HL_END, HL_START, type SearchHit } from '@/lib/api'

function Highlighted({ text }: { text: string }) {
  const parts: React.ReactNode[] = []
  let rest = text
  let key = 0
  while (rest.length) {
    const start = rest.indexOf(HL_START)
    if (start === -1) {
      parts.push(rest)
      break
    }
    const end = rest.indexOf(HL_END, start)
    parts.push(rest.slice(0, start))
    const hit = end === -1 ? rest.slice(start + 1) : rest.slice(start + 1, end)
    parts.push(
      <mark key={key++} className="bg-primary/15 text-foreground rounded px-0.5">
        {hit}
      </mark>,
    )
    rest = end === -1 ? '' : rest.slice(end + 1)
  }
  return <>{parts}</>
}

export function SearchPage() {
  const [query, setQuery] = useState('')
  const [hits, setHits] = useState<SearchHit[] | null>(null)
  const [openId, setOpenId] = useState<number | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => inputRef.current?.focus(), [])

  useEffect(() => {
    const q = query.trim()
    if (!q) {
      setHits(null)
      return
    }
    const t = setTimeout(() => {
      api.search(q).then(setHits).catch((e) => toast.error(String(e)))
    }, 180)
    return () => clearTimeout(t)
  }, [query])

  return (
    <div className="px-6 py-6 max-w-3xl">
      <h1 className="text-[26px] font-semibold tracking-tight leading-none mb-4">Search</h1>
      <div className="relative mb-5">
        <SearchIcon className="h-4 w-4 absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground" />
        <Input
          ref={inputRef}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Find something you saw: a page, a doc, a name, a number…"
          className="pl-9 h-11"
        />
      </div>

      {hits && hits.length === 0 && (
        <EmptyState icon={SearchIcon} title="No matches" description="Try fewer or different words." tone="muted" />
      )}

      <div className="space-y-0.5">
        {hits?.map((h) => (
          <ActivityRow key={h.activity.id} activity={h.activity} onOpen={setOpenId}>
            <p className="text-xs text-muted-foreground mt-1 line-clamp-3">
              <Highlighted text={h.snippet} />
            </p>
          </ActivityRow>
        ))}
      </div>

      <ActivitySheet
        id={openId}
        onClose={() => setOpenId(null)}
        onDeleted={() => setHits((prev) => prev?.filter((h) => h.activity.id !== openId) ?? null)}
      />
    </div>
  )
}
