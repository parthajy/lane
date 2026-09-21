import { useCallback, useEffect, useRef, useState } from 'react'
import { ArrowRight, BarChart3, Brain, MoreHorizontal, PanelLeftClose, PanelLeftOpen, ChevronDown, Circle, FileSearch, HelpCircle, Info, LogOut, MessageSquare, Mic, Network, Pause, PenLine, Play, Plug, Search, Settings as SettingsIcon, ShieldAlert, Sunrise, Users } from 'lucide-react'
import { Toaster } from 'sonner'
import { cn } from '@/lib/utils'
import { api, type EngineReport, type RecordingReport, type Status } from '@/lib/api'
import { SettingsPage, type SettingsSection } from '@/pages/settings'
import { MemoriesPage, type MemoriesView } from '@/pages/memories'
import { AskPage } from '@/pages/ask'
import { BoardView } from '@/pages/landscape/board-view'
import { PeoplePage } from '@/pages/people'
import { MeetingsPage } from '@/pages/meetings'
import { IntegrationsPage } from '@/pages/integrations'
import { FilesPage } from '@/pages/files'
import { TodayPage } from '@/pages/today'
import { CapturePage } from '@/pages/capture'
import { ExplorePage } from '@/pages/explore'
import { Onboarding } from '@/components/onboarding'
import { FeedbackDialog } from '@/components/feedback'
import { LicenceWall, useLicence } from '@/components/licence'
import { UpdateCard } from '@/components/update-card'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'

type Page = 'today' | 'ask' | 'memories' | 'meetings' | 'files' | 'people' | 'board' | 'integrations' | 'explore' | 'settings'

const NAV: { id: Page; label: string; icon: typeof Brain; group?: string }[] = [
  { id: 'today', label: 'Today', icon: Sunrise },
  { id: 'ask', label: 'Ask', icon: MessageSquare },
  { id: 'memories', label: 'Memories', icon: Brain, group: 'Your memory' },
  { id: 'meetings', label: 'Meetings', icon: Mic, group: 'Your memory' },
  { id: 'files', label: 'Files', icon: FileSearch, group: 'Your memory' },
  { id: 'people', label: 'People', icon: Users, group: 'Your memory' },
  { id: 'board', label: 'Board', icon: Network, group: 'Your memory' },
  { id: 'integrations', label: 'Integrations', icon: Plug, group: 'Tools' },
  { id: 'settings', label: 'Settings', icon: SettingsIcon, group: 'Tools' },
]

/** Pages the old sidebar had; the tray and deep links still name them. */
const LEGACY: Record<string, { page: Page; view?: MemoriesView; section?: SettingsSection }> = {
  tasks: { page: 'memories', view: 'tasks' },
  timeline: { page: 'memories', view: 'timeline' },
  search: { page: 'memories', view: 'everything' },
  capture: { page: 'today' },
}

function StatusDot({ status, engine }: { status: Status | null; engine: EngineReport | null }) {
  if (!status) return null
  const c = status.capture
  let dot = 'bg-emerald-500'
  let head = 'Capturing'
  let sub = c.currentApp ? `Reading ${c.currentApp}` : 'Listening in the background'
  if (status.paused) {
    dot = 'bg-muted-foreground'; head = 'Paused'; sub = 'Nothing is being recorded'
  } else if (c.excluded) {
    dot = 'bg-amber-500'; head = 'Skipping'; sub = 'This app is on your excluded list'
  } else if (c.idle) {
    dot = 'bg-muted-foreground'; head = 'Idle'; sub = 'Waiting for something to happen'
  }
  if (engine) {
    if (!engine.available && engine.downloadPercent != null) sub = `Downloading the model… ${engine.downloadPercent}%`
    else if (engine.busy && engine.counts.pending > 0) sub = `Catching up on ${engine.counts.pending} memories`
  }
  return (
    <div className="hidden lg:flex items-center gap-2.5 rounded-xl border bg-card px-3 py-1.5" title={`${head} · ${sub}`}>
      <span className={cn('h-2 w-2 rounded-full shrink-0', dot, !status.paused && !c.idle && 'animate-pulse-soft')} />
      <div className="leading-tight min-w-0">
        <div className="text-[12.5px] font-medium">{head}</div>
        <div className="text-[11px] text-muted-foreground truncate max-w-[180px]">{sub}</div>
      </div>
    </div>
  )
}

export default function App() {
  const [page, setPage] = useState<Page>('today')
  /* The rail can be folded down to icons; the board takes the whole window. */
  const [collapsed, setCollapsed] = useState<boolean>(() => {
    try { return localStorage.getItem('lane.sidebar') === 'collapsed' } catch { return false }
  })
  useEffect(() => { try { localStorage.setItem('lane.sidebar', collapsed ? 'collapsed' : 'open') } catch { /* private window */ } }, [collapsed])
  const [memView, setMemView] = useState<MemoriesView>('memories')
  const [memQuery, setMemQuery] = useState('')
  const [settingsSection, setSettingsSection] = useState<SettingsSection | null>(null)
  const [handoff, setHandoff] = useState<string | null>(null)
  const [engine, setEngine] = useState<EngineReport | null>(null)
  const [rec, setRec] = useState<RecordingReport | null>(null)
  const [status, setStatus] = useState<Status | null>(null)
  const [capture, setCapture] = useState(false)
  const [search, setSearch] = useState('')
  const searchRef = useRef<HTMLInputElement>(null)
  // null = still loading; a number = show setup at that step
  const [setupStep, setSetupStep] = useState<number | null | undefined>(undefined)

  useEffect(() => {
    api.getSettings().then((s) => setSetupStep(s.onboardingDone ? null : 0))
  }, [])

  const refreshStatus = useCallback(() => {
    api.status().then(setStatus).catch(() => {})
    api.engineStatus().then(setEngine).catch(() => {})
    api.recordingStatus().then(setRec).catch(() => {})
  }, [])

  useEffect(() => {
    refreshStatus()
    const t = setInterval(refreshStatus, 3000)
    return () => clearInterval(t)
  }, [refreshStatus])

  const go = useCallback((target: string) => {
    const legacy = LEGACY[target]
    if (legacy) {
      setPage(legacy.page)
      if (legacy.view) setMemView(legacy.view)
      if (legacy.section) setSettingsSection(legacy.section)
      return
    }
    if ((NAV as { id: string }[]).some((n) => n.id === target) || target === 'settings' || target === 'explore') setPage(target as Page)
  }, [])

  useEffect(() => {
    const un = api.onNavigate(({ page }) => {
      if (page) go(page)
    })
    return () => {
      un.then((f) => f())
    }
  }, [go])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && e.key === 'k') {
        e.preventDefault()
        searchRef.current?.focus()
      }
      if (e.metaKey && e.key === 'n') {
        e.preventDefault()
        setCapture(true)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const ask = (q: string) => {
    setHandoff(q)
    setPage('ask')
  }

  const openSettings = (section: SettingsSection | null = null) => {
    setSettingsSection(section)
    setPage('settings')
  }

  function submitSearch(e: React.FormEvent) {
    e.preventDefault()
    const q = search.trim()
    if (!q) return
    setMemQuery(q)
    setMemView('memories')
    setPage('memories')
  }


  const [lic, setLic] = useLicence()
  const [feedback, setFeedback] = useState(false)
  const full = page === 'board'

  return (
    <div className={cn('enterprise-shell h-screen flex text-foreground select-none', full ? 'p-0 gap-0 bg-background' : 'gap-2.5 p-2.5 app-ground')}>
      {!full && (
      <aside className={cn('panel shrink-0 flex flex-col p-3 gap-3 overflow-hidden transition-[width] duration-200', collapsed ? 'w-[68px] items-center' : 'w-[252px]')}>
        <div className={cn('flex items-center gap-2.5 pt-1', collapsed ? 'flex-col' : 'px-1')}>
          <img src="/dark.png" alt="" className="h-5 w-5 object-contain dark:invert" />
          {!collapsed && (
            <div className="leading-tight min-w-0">
              <div className="font-semibold tracking-tight text-[17px]">Lane</div>
              <div className="text-[11px] text-muted-foreground truncate">Your memory, always with you</div>
            </div>
          )}
          <button
            onClick={() => setCollapsed((v) => !v)}
            title={collapsed ? 'Show the sidebar' : 'Collapse the sidebar'}
            aria-label={collapsed ? 'Show the sidebar' : 'Collapse the sidebar'}
            className={cn('rounded-lg p-1.5 text-muted-foreground hover:bg-secondary hover:text-foreground', !collapsed && 'ml-auto')}
          >
            {collapsed ? <PanelLeftOpen className="h-4 w-4" /> : <PanelLeftClose className="h-4 w-4" />}
          </button>
        </div>

        <div className="flex-1 min-h-0 overflow-y-auto -mx-1 px-1 flex flex-col gap-3">
        <nav className="flex flex-col gap-0.5">
          {NAV.map(({ id, label, icon: Icon, group }, i) => (
            <div key={id} className="contents">
              {group && NAV[i - 1]?.group !== group && (
                <>
                  {group === 'Tools' && <div className={cn('border-t mt-3', collapsed ? 'mx-2' : 'mx-3')} />}
                  {!collapsed && <div className="px-3 pt-3 pb-1.5 text-[10.5px] font-medium uppercase tracking-[0.14em] text-muted-foreground/70">{group}</div>}
                </>
              )}
              <button
                onClick={() => (id === 'settings' ? openSettings(null) : setPage(id))}
                /* Ask is the one thing you come here to *do*, so it reads as a
                   button rather than another row in the list. */
                title={collapsed ? label : undefined}
                className={cn(
                  id === 'ask'
                    ? cn('nav-ask', collapsed ? 'justify-center px-0' : 'px-3.5', page === 'ask' && 'is-on')
                    : cn('nav-item', collapsed && 'justify-center px-0'),
                  id !== 'ask' && page === id && 'nav-item-on',
                )}
              >
                <Icon className={cn('h-[17px] w-[17px] shrink-0', id === 'ask' ? 'text-white' : page === id ? 'text-primary' : 'text-muted-foreground')} />
                {!collapsed && <span className="truncate">{label}</span>}
                {id === 'today' && status && !collapsed && (
                  <span className={cn('ml-auto rounded-full px-2 py-0.5 text-[11px] tabular-nums', page === 'today' ? 'bg-primary/12 text-primary font-medium' : 'text-muted-foreground')}>
                    {status.stats.activitiesToday}
                  </span>
                )}
              </button>
            </div>
          ))}
        </nav>

        {status && !status.capture.trusted && (
          <button onClick={() => setSetupStep(1)} className="rounded-2xl border border-destructive/30 bg-destructive/5 p-3 text-left text-xs">
            <div className="flex items-center gap-1.5 font-medium text-destructive">
              <ShieldAlert className="h-3.5 w-3.5" /> Limited capture
            </div>
            <p className="text-muted-foreground mt-1">Only app names are being recorded. Click to turn on Accessibility.</p>
          </button>
        )}
        </div>

        {collapsed ? (
          <button onClick={() => openSettings('labels')} title={status ? `Rabbit is running · ${status.stats.activities.toLocaleString()} memories on this Mac` : 'Rabbit is running'} className="shrink-0 mt-auto h-11 w-11 rounded-2xl bg-accent grid place-items-center text-accent-foreground hover:brightness-95">
            <svg viewBox="0 0 40 40" className="h-6 w-6 fill-current" aria-hidden="true">
              <ellipse cx="15.2" cy="11.5" rx="3.1" ry="8.4" transform="rotate(-16 15.2 11.5)" />
              <ellipse cx="24.8" cy="11.5" rx="3.1" ry="8.4" transform="rotate(16 24.8 11.5)" />
              <ellipse cx="20" cy="27" rx="9.6" ry="8.2" />
              <circle cx="16.6" cy="26" r="1.15" className="fill-background" />
              <circle cx="23.4" cy="26" r="1.15" className="fill-background" />
            </svg>
          </button>
        ) : (
        <div className="shrink-0 rounded-2xl bg-accent p-3.5 text-accent-foreground">
          <svg viewBox="0 0 40 40" className="h-7 w-7 fill-current" aria-hidden="true">
            <ellipse cx="15.2" cy="11.5" rx="3.1" ry="8.4" transform="rotate(-16 15.2 11.5)" />
            <ellipse cx="24.8" cy="11.5" rx="3.1" ry="8.4" transform="rotate(16 24.8 11.5)" />
            <ellipse cx="20" cy="27" rx="9.6" ry="8.2" />
            <circle cx="16.6" cy="26" r="1.15" className="fill-background" />
            <circle cx="23.4" cy="26" r="1.15" className="fill-background" />
          </svg>
          <p className="mt-2 text-[14.5px] font-semibold leading-tight flex items-center gap-2">Rabbit is running <span className={cn('h-2 w-2 rounded-full', status?.paused ? 'bg-muted-foreground' : 'bg-emerald-500 animate-pulse-soft')} /></p>
          <p className="mt-1 text-[11.5px] leading-snug opacity-80">
            {status ? `${status.stats.activities.toLocaleString()} memories, all on this Mac.` : 'Reading, writing and answering on this Mac.'} Nothing leaves it.
          </p>
          <button onClick={() => setFeedback(true)} className="mt-2.5 w-full h-8.5 py-2 rounded-xl bg-foreground text-background text-[12.5px] font-medium flex items-center justify-between px-3 hover:opacity-90">
            Help &amp; support <ArrowRight className="h-3.5 w-3.5" />
          </button>
        </div>
        )}

      </aside>
      )}

      <div className={cn('flex-1 flex flex-col min-w-0 overflow-hidden', !full && 'panel')}>
        {!full && (
        <header className="h-16 shrink-0 flex items-center gap-3 px-4">
          <form onSubmit={submitSearch} className="relative flex-1 max-w-md">
            <Search className="h-4 w-4 absolute left-3.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
            <input
              ref={searchRef}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search memories, documents, people…"
              className="w-full h-10 rounded-full bg-secondary pl-10 pr-12 text-sm outline-none placeholder:text-muted-foreground focus:ring-2 focus:ring-ring focus:bg-background"
            />
            <kbd className="absolute right-3.5 top-1/2 -translate-y-1/2 text-[10px] text-muted-foreground">⌘K</kbd>
          </form>

          <div className="ml-auto flex items-center gap-2">
            {rec?.recording && (
              <button onClick={() => go('meetings')} className="flex items-center gap-1.5 h-9 rounded-full border border-destructive/40 bg-destructive/5 px-3 text-xs text-destructive" title="Recording a meeting">
                <Circle className="h-2.5 w-2.5 fill-destructive text-destructive animate-pulse-soft" /> Recording
              </button>
            )}
            <StatusDot status={status} engine={engine} />
            <button onClick={() => setCapture(true)} className="flex items-center gap-1.5 h-10 rounded-full bg-primary text-primary-foreground px-4 text-[13.5px] font-medium hover:brightness-110 shadow-sm" title="Note, voice note or meeting (⌘N)">
              <PenLine className="h-3.5 w-3.5" /> Capture
            </button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button className="h-10 w-10 rounded-full bg-secondary grid place-items-center hover:bg-accent" title="You and your settings">
                  <SettingsIcon className="h-4 w-4 text-muted-foreground" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-52">
                <DropdownMenuItem onClick={() => openSettings(null)}><SettingsIcon className="h-4 w-4 mr-2" /> Settings</DropdownMenuItem>
                <DropdownMenuItem onClick={() => setPage('explore')}><BarChart3 className="h-4 w-4 mr-2" /> Explore</DropdownMenuItem>
                <DropdownMenuItem onClick={() => openSettings('labels')}><HelpCircle className="h-4 w-4 mr-2" /> Help Rabbit learn</DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={() => openSettings('about')}><Info className="h-4 w-4 mr-2" /> About Lane</DropdownMenuItem>
                <DropdownMenuItem onClick={() => api.quitApp()}><LogOut className="h-4 w-4 mr-2" /> Quit Lane</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </header>
        )}

        <main className={cn('flex-1 min-h-0', full ? 'overflow-hidden' : 'overflow-y-auto select-text')}>
          {!full && <div className="px-6 pt-1 empty:hidden"><UpdateCard auto /></div>}
          {page === 'today' && <TodayPage onAsk={ask} onOpenTasks={() => go('tasks')} />}
          {page === 'explore' && <ExplorePage onAsk={ask} onOpenPeople={() => setPage('people')} />}
          {page === 'ask' && <AskPage initialQuestion={handoff} onConsumed={() => setHandoff(null)} />}
          {page === 'memories' && <MemoriesPage view={memView} onViewChange={setMemView} initialQuery={memQuery} onQueryConsumed={() => setMemQuery('')} />}
          {page === 'people' && <PeoplePage onAsk={ask} />}
          {page === 'board' && <BoardView onAsk={ask} onOpenMemory={() => { setMemView('memories'); setPage('memories') }} onExit={() => setPage('today')} />}
          {page === 'meetings' && <MeetingsPage />}
          {page === 'integrations' && <IntegrationsPage />}
          {page === 'files' && <FilesPage onOpenSettings={() => openSettings('data')} />}
          {page === 'settings' && <SettingsPage status={status} onChanged={refreshStatus} onRunSetup={() => setSetupStep(0)} section={settingsSection} />}
        </main>
      </div>

      <Dialog open={capture} onOpenChange={setCapture}>
        <DialogContent className="max-w-2xl p-0 overflow-hidden">
          <DialogHeader className="px-6 pt-5">
            <DialogTitle>Capture</DialogTitle>
          </DialogHeader>
          <div className="max-h-[75vh] overflow-y-auto">
            <CapturePage embedded />
          </div>
        </DialogContent>
      </Dialog>

      {setupStep != null && (
        <Onboarding
          initialStep={setupStep}
          onDone={() => {
            setSetupStep(null)
            setPage('today')
            refreshStatus()
          }}
        />
      )}
      <FeedbackDialog open={feedback} onOpenChange={setFeedback} />
      {lic?.blocked && <LicenceWall lic={lic} onApplied={setLic} />}
      <Toaster position="bottom-right" />
    </div>
  )
}
