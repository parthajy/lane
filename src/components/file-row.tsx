import { format } from 'date-fns'
import { Eye, FileText, Folder, Plug, SquareArrowOutUpRight } from 'lucide-react'
import { api, formatBytes, type FileHit } from '@/lib/api'
import { cn } from '@/lib/utils'

/** A colour per family, so the eye can sort the list before reading it. */
const TINT: Record<string, string> = {
  pdf: 'bg-rose-50 text-rose-600 dark:bg-rose-500/15 dark:text-rose-300',
  doc: 'bg-blue-50 text-blue-600 dark:bg-blue-500/15 dark:text-blue-300',
  docx: 'bg-blue-50 text-blue-600 dark:bg-blue-500/15 dark:text-blue-300',
  md: 'bg-violet-50 text-violet-600 dark:bg-violet-500/15 dark:text-violet-300',
  txt: 'bg-secondary text-muted-foreground',
  png: 'bg-indigo-50 text-indigo-600 dark:bg-indigo-500/15 dark:text-indigo-300',
  jpg: 'bg-indigo-50 text-indigo-600 dark:bg-indigo-500/15 dark:text-indigo-300',
  jpeg: 'bg-indigo-50 text-indigo-600 dark:bg-indigo-500/15 dark:text-indigo-300',
  html: 'bg-amber-50 text-amber-700 dark:bg-amber-500/15 dark:text-amber-300',
  xlsx: 'bg-emerald-50 text-emerald-700 dark:bg-emerald-500/15 dark:text-emerald-300',
}

export function FileRow({ f, compact, selected, onSelect }: { f: FileHit; compact?: boolean; selected?: boolean; onSelect?: () => void }) {
  const notion = f.path.startsWith('notion://')
  const connector = f.path.startsWith('connector://')
  const folder = notion ? 'Notion' : connector ? f.ext : f.path.replace(/\/[^/]+$/, '').replace(/^\/Users\/[^/]+/, '~')
  const open = () => (connector && f.url ? api.openFile(f.url) : api.openFile(f.path))
  const reveal = () => (connector ? (f.url ? api.openFile(f.url) : Promise.resolve()) : api.revealFile(f.path))
  const ext = (f.ext || '').toLowerCase()

  return (
    <div
      onClick={onSelect}
      className={cn('group flex items-center gap-3 px-3 py-2.5 rounded-xl cursor-default', selected ? 'bg-accent/60' : 'hover:bg-secondary')}
    >
      <div className={cn('h-10 w-10 shrink-0 rounded-xl grid place-items-center text-[10px] font-semibold uppercase', TINT[ext] ?? 'bg-secondary text-muted-foreground')}>
        {connector ? <Plug className="h-4 w-4" /> : ext ? ext.slice(0, 4) : <FileText className="h-4 w-4" />}
      </div>

      <div className="min-w-0 flex-1">
        <button onClick={(e) => { e.stopPropagation(); open() }} className="block max-w-full truncate text-left text-[14px] font-medium hover:underline">
          {f.name}
        </button>
        <button onClick={(e) => { e.stopPropagation(); reveal() }} title={notion ? 'Open in Notion' : connector ? 'Open the source' : 'Show in Finder'} className="flex items-center gap-1.5 text-[12px] text-muted-foreground hover:text-foreground max-w-full">
          <Folder className="h-3 w-3 shrink-0" /> <span className="truncate">{folder}</span>
        </button>
      </div>

      {f.snippet && !compact && (
        <p className="hidden lg:block text-[12.5px] text-muted-foreground truncate flex-1 min-w-0">{f.snippet}</p>
      )}

      <div className="flex items-center gap-1 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
        <button onClick={(e) => { e.stopPropagation(); open() }} title="Open" className="rounded-lg p-1.5 text-muted-foreground hover:bg-background hover:text-foreground"><SquareArrowOutUpRight className="h-3.5 w-3.5" /></button>
        <button onClick={(e) => { e.stopPropagation(); reveal() }} title="Show in Finder" className="rounded-lg p-1.5 text-muted-foreground hover:bg-background hover:text-foreground"><Eye className="h-3.5 w-3.5" /></button>
      </div>

      <div className="shrink-0 text-right">
        <div className="text-[12px] text-muted-foreground tabular-nums">{format(f.mtime, 'd MMM yyyy')}</div>
        {!notion && !connector && <div className="text-[11.5px] text-muted-foreground/80 tabular-nums">{formatBytes(f.size)}</div>}
      </div>
    </div>
  )
}
