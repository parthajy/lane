import { useEffect, useMemo, useRef, useState } from 'react'
import { ArrowDownUp, Clock, Code2, FileSearch, FileText, FolderOpen, Image as ImageIcon, Lightbulb, Lock, Plus, RefreshCw } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { EmptyState } from '@/components/ui/empty-state'
import { FileRow } from '@/components/file-row'
import { api, type FileHit, type FileReport } from '@/lib/api'
import { cn } from '@/lib/utils'

/** The filter chips, each a family of extensions we actually index. */
const FAMILIES = [
  { id: 'recent', label: 'Recent', icon: Clock, exts: null },
  { id: 'pdf', label: 'PDFs', icon: FileText, exts: ['pdf'] },
  { id: 'docs', label: 'Docs', icon: FileText, exts: ['doc', 'docx', 'pages', 'rtf', 'odt', 'txt', 'md'] },
  { id: 'images', label: 'Images', icon: ImageIcon, exts: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'heic', 'svg', 'tiff'] },
  { id: 'code', label: 'Code', icon: Code2, exts: ['ts', 'tsx', 'js', 'jsx', 'py', 'rs', 'go', 'java', 'c', 'cpp', 'h', 'sh', 'html', 'css', 'json', 'toml', 'yml', 'yaml'] },
] as { id: string; label: string; icon: typeof Clock; exts: string[] | null }[]

const SORTS = [
  { id: 'mtime', label: 'Last modified' },
  { id: 'name', label: 'Name' },
  { id: 'size', label: 'Size' },
] as const

const DOT = ['#5a51e5', '#8b5cf6', '#2b9cf3', '#f2555a', '#f5a524', '#4f7bf5', '#a78bfa', '#8b8ba7']

export function FilesPage({ onOpenSettings }: { onOpenSettings?: () => void }) {
  const [query, setQuery] = useState('')
  const [hits, setHits] = useState<FileHit[] | null>(null)
  const [recent, setRecent] = useState<FileHit[]>([])
  const [report, setReport] = useState<FileReport | null>(null)
  const [family, setFamily] = useState<string>('recent')
  const [sort, setSort] = useState<(typeof SORTS)[number]['id']>('mtime')
  const [selected, setSelected] = useState<number | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    inputRef.current?.focus()
    const tick = () => api.fileStats().then(setReport).catch(() => {})
    tick()
    api.listFiles(120).then(setRecent).catch(() => {})
    const t = setInterval(tick, 3000)
    return () => clearInterval(t)
  }, [])

  useEffect(() => {
    if (report && report.pending === 0) api.listFiles(120).then(setRecent).catch(() => {})
  }, [report?.pending]) // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const q = query.trim()
    if (!q) {
      setHits(null)
      return
    }
    const t = setTimeout(() => api.searchFiles(q, 50).then(setHits).catch((e) => toast.error(String(e))), 200)
    return () => clearTimeout(t)
  }, [query])

  const list = useMemo(() => {
    const base = hits ?? recent
    const exts = FAMILIES.find((f) => f.id === family)?.exts
    const filtered = exts ? base.filter((f) => exts.includes((f.ext || '').toLowerCase())) : base
    const sorted = [...filtered]
    if (sort === 'name') sorted.sort((a, b) => a.name.localeCompare(b.name))
    else if (sort === 'size') sorted.sort((a, b) => b.size - a.size)
    else sorted.sort((a, b) => b.mtime - a.mtime)
    return sorted
  }, [hits, recent, family, sort])

  const types = report?.byExt ?? []
  const typeTotal = types.reduce((a, t) => a + t[1], 0)

  return (
    <div className="grid grid-cols-[minmax(0,1fr)_300px] h-full min-h-0">
      <div className="px-6 py-5 overflow-y-auto min-h-0">
        {/* Masthead */}
        <div className="flex items-start gap-4 mb-5 flex-wrap">
          <span className="h-14 w-14 shrink-0 rounded-2xl tone-violet grid place-items-center text-primary">
            <FolderOpen className="h-7 w-7" />
          </span>
          <div className="min-w-0 flex-1">
            <h1 className="text-[32px] font-semibold tracking-[-0.03em] leading-none">Files</h1>
            <p className="text-[13.5px] text-muted-foreground mt-2 max-w-[52ch]">
              Every document in your folders, searchable by name, contents, folder and type. Ask can quote them.
            </p>
          </div>
          <div className="flex items-start gap-2.5 rounded-2xl bg-emerald-50 dark:bg-emerald-500/10 px-3.5 py-3 max-w-[280px]">
            <span className="h-8 w-8 shrink-0 rounded-lg bg-background grid place-items-center text-emerald-600 dark:text-emerald-400"><Lock className="h-4 w-4" /></span>
            <div className="leading-snug">
              <div className="text-[13px] font-semibold">Private by design</div>
              <div className="text-[12px] text-muted-foreground">Files are read on this Mac and never uploaded.</div>
            </div>
          </div>
        </div>

        {/* Search */}
        <div className="relative mb-4">
          <FileSearch className="h-4 w-4 absolute left-4 top-1/2 -translate-y-1/2 text-muted-foreground" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Find files by name or content…"
            className="w-full h-13 py-3.5 rounded-2xl border bg-card pl-11 pr-4 text-[14px] outline-none placeholder:text-muted-foreground focus:ring-2 focus:ring-ring"
          />
        </div>

        {/* Families and sort */}
        <div className="flex items-center gap-2 mb-4 flex-wrap">
          {FAMILIES.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              onClick={() => setFamily(id)}
              className={cn('flex items-center gap-1.5 rounded-full border px-3.5 py-1.5 text-[13px] transition-colors',
                family === id ? 'bg-accent/70 border-primary/30 text-foreground font-medium' : 'bg-card text-muted-foreground hover:text-foreground hover:bg-secondary')}
            >
              <Icon className={cn('h-3.5 w-3.5', family === id && 'text-primary')} /> {label}
            </button>
          ))}
          <div className="ml-auto flex items-center gap-1.5 rounded-full border bg-card px-3 py-1.5 text-[13px] text-muted-foreground">
            <ArrowDownUp className="h-3.5 w-3.5" />
            <select value={sort} onChange={(e) => setSort(e.target.value as typeof sort)} className="bg-transparent outline-none cursor-pointer text-foreground">
              {SORTS.map((s) => <option key={s.id} value={s.id}>{s.label}</option>)}
            </select>
          </div>
        </div>

        {hits && list.length === 0 && <EmptyState icon={FileSearch} title="No documents match" description="Try other words, or add the folder in Settings." tone="muted" />}
        {!query && report && report.files === 0 && (
          <EmptyState
            icon={FileSearch}
            title="Indexing your documents"
            description={report.pending > 0 ? `${report.pending} to go. macOS may ask permission for each folder the first time.` : 'No documents found in the indexed folders yet.'}
            action={{ label: 'Rescan now', onClick: () => api.reindexFiles() }}
            tone="muted"
          />
        )}
        {!hits && !query && list.length === 0 && report && report.files > 0 && (
          <p className="text-sm text-muted-foreground px-3 py-6">Nothing of that kind among the recently changed files.</p>
        )}

        <div className="space-y-0.5">
          {list.map((f) => (
            <FileRow key={f.fileId} f={f} selected={selected === f.fileId} onSelect={() => setSelected(f.fileId)} />
          ))}
        </div>
      </div>

      <aside className="border-l px-4 py-5 space-y-3 overflow-y-auto min-h-0">
        <div className="grid grid-cols-2 gap-2.5">
          <div className="rounded-2xl border bg-card p-3.5">
            <span className="h-9 w-9 rounded-xl tone-blue grid place-items-center text-primary mb-2.5"><FileText className="h-4 w-4" /></span>
            <div className="text-[22px] font-semibold tabular-nums leading-none">{report?.files.toLocaleString() ?? '…'}</div>
            <div className="text-[12px] text-muted-foreground mt-1">indexed files</div>
          </div>
          <div className="rounded-2xl border bg-card p-3.5">
            <span className="h-9 w-9 rounded-xl tone-amber grid place-items-center text-amber-600 dark:text-amber-400 mb-2.5"><Clock className="h-4 w-4" /></span>
            <div className="text-[22px] font-semibold tabular-nums leading-none">{report?.pending?.toLocaleString() ?? '0'}</div>
            <div className="text-[12px] text-muted-foreground mt-1">to index</div>
          </div>
        </div>

        <section className="rounded-2xl border bg-card p-4">
          <h2 className="flex items-center gap-2 text-[13.5px] font-semibold mb-3">
            <FolderOpen className="h-4 w-4 text-primary" /> Watched folders
            {onOpenSettings && (
              <button onClick={onOpenSettings} className="ml-auto flex items-center gap-1 rounded-lg px-2 py-1 text-[12px] font-medium text-primary hover:bg-secondary">
                <Plus className="h-3.5 w-3.5" /> Add
              </button>
            )}
          </h2>
          <ul className="space-y-1.5">
            {report?.folders.map((f) => (
              <li key={f} className="flex items-center gap-2 text-[13px] min-w-0" title={f}>
                <FolderOpen className="h-4 w-4 shrink-0 text-muted-foreground" />
                <span className="truncate">{f.replace(/^\/Users\/[^/]+/, '~')}</span>
              </li>
            ))}
          </ul>
          <p className="text-[12px] text-muted-foreground mt-3">{report?.detail || 'These folders are kept in step with Lane.'}</p>
          <Button size="sm" variant="outline" className="mt-3 w-full" onClick={() => { api.reindexFiles(); toast.message('Rescanning your folders') }}>
            <RefreshCw className="h-3.5 w-3.5 mr-1.5" /> Rescan
          </Button>
        </section>

        {types.length > 0 && (
          <section className="rounded-2xl border bg-card p-4">
            <h2 className="text-[13.5px] font-semibold mb-3">Files by type</h2>
            <ul className="space-y-2">
              {types.map(([e, n], i) => (
                <li key={e} className="flex items-center gap-2.5 text-[13px]">
                  <span className="h-2 w-2 rounded-full shrink-0" style={{ background: DOT[i % DOT.length] }} />
                  <span className="uppercase truncate">{e}</span>
                  <span className="ml-auto tabular-nums text-muted-foreground">{n.toLocaleString()}</span>
                </li>
              ))}
            </ul>
            {typeTotal > 0 && report && report.files > typeTotal && (
              <p className="text-[11.5px] text-muted-foreground mt-2.5">Top {types.length} of {report.files.toLocaleString()} files.</p>
            )}
          </section>
        )}

        <section className="rounded-2xl tone-violet p-4">
          <h2 className="flex items-center gap-2 text-[13.5px] font-semibold mb-1.5"><Lightbulb className="h-4 w-4 text-primary" /> Tip</h2>
          <p className="text-[12.5px] text-muted-foreground">Ask can read these files back to you. Try “what did the Vatsalya proposal say about the timeline?”</p>
        </section>
      </aside>
    </div>
  )
}
