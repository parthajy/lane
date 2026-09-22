import { useEffect, useState } from 'react'
import { Cpu, Database, KeyRound, EyeOff, FileText, GraduationCap, Info, Laptop, Lock, Mic, Radio, Shield, Sparkles } from 'lucide-react'
import { toast } from 'sonner'
import { cn } from '@/lib/utils'
import { LicenceCard, useLicence } from '@/components/licence'
import { NotchPositionPicker } from '@/components/notch-position'
import { WhyLine } from '@/components/three-things'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'
import {
  Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
} from '@/components/ui/dialog'
import { AccessibilityPanel, usePermissions } from '@/components/accessibility-panel'
import { NEVER_ASKED, ONLY_WHEN_ON } from '@/components/onboarding'
import { BackupCard } from '@/components/backup-card'
import { UpdateCard } from '@/components/update-card'
import { api, formatBytes, type EngineReport, type FileReport, type LabelsStatus, type PrivacyLists, type PermissionReport, type Settings, type Status } from '@/lib/api'
import { format } from 'date-fns'

const lines = (s: string) => s.split('\n').map((l) => l.trim()).filter(Boolean)

export type SettingsSection = 'labels' | 'about' | 'data'


const SECTIONS: { slug: string; label: string; icon: typeof Shield }[] = [
  { slug: 'licence', label: 'Your licence', icon: KeyRound },
  { slug: 'permissions', label: 'Permissions', icon: Shield },
  { slug: 'capture', label: 'Capture', icon: Radio },
  { slug: 'engine', label: 'Rabbit', icon: Cpu },
  { slug: 'files', label: 'Files', icon: FileText },
  { slug: 'why', label: 'Your why', icon: Sparkles },
  { slug: 'meetings', label: 'Meetings', icon: Mic },
  { slug: 'privacy', label: 'Never capture', icon: EyeOff },
  { slug: 'labels', label: 'Help Rabbit learn', icon: GraduationCap },
  { slug: 'about', label: 'About Lane', icon: Info },
  { slug: 'data', label: 'Your data', icon: Database },
]

/** The rail: every section on this page, and where you are in it. */
function SectionRail({ active, onGo }: { active: string; onGo: (slug: string) => void }) {
  return (
    <nav className="rounded-2xl border bg-card p-2 sticky top-0">
      {SECTIONS.map((s) => (
        <button
          key={s.slug}
          onClick={() => onGo(s.slug)}
          className={cn('w-full flex items-center gap-2.5 rounded-xl px-3 py-2 text-[13.5px] transition-colors',
            active === s.slug ? 'bg-accent/60 font-medium text-foreground' : 'text-muted-foreground hover:bg-secondary hover:text-foreground')}
        >
          <s.icon className={cn('h-4 w-4 shrink-0', active === s.slug ? 'text-primary' : 'text-muted-foreground')} />
          {s.label}
        </button>
      ))}
    </nav>
  )
}

/* The two builds Lane ships. Named here, and only here, because the field
   needs the exact file; nothing else in the app says what is inside Rabbit. */
const RABBIT_S = 'Qwen3-4B-Instruct-2507-Q4_K_M.gguf'
const RABBIT_M = 'Qwen3-8B-Q4_K_M.gguf'

export function SettingsPage({
  status,
  onChanged,
  onRunSetup,
  section,
}: {
  status: Status | null
  onChanged: () => void
  onRunSetup: () => void
  section?: SettingsSection | null
}) {
  useEffect(() => {
    if (!section) return
    const t = setTimeout(() => document.getElementById(`settings-${section}`)?.scrollIntoView({ behavior: 'smooth', block: 'start' }), 80)
    return () => clearTimeout(t)
  }, [section])
  const [settings, setSettings] = useState<Settings | null>(null)
  const [perms, setPerms] = useState<PermissionReport | null>(null)
  useEffect(() => { api.permissions().then(setPerms).catch(() => {}) }, [])
  const [apps, setApps] = useState('')
  const [urls, setUrls] = useState('')
  const [confirmWipe, setConfirmWipe] = useState(false)
  const [forget, setForget] = useState('')
  const [confirmForget, setConfirmForget] = useState(false)
  const [lists, setLists] = useState<PrivacyLists | null>(null)
  const [folders, setFolders] = useState('')
  const [bfolder, setBfolder] = useState('')
  const [labels, setLabels] = useState<LabelsStatus | null>(null)
  const refreshLabels = () => api.labelsStatus().then(setLabels).catch(() => {})
  const [fileReport, setFileReport] = useState<FileReport | null>(null)
  const report = usePermissions(2000)
  const [engine, setEngine] = useState<EngineReport | null>(null)
  useEffect(() => {
    const tick = () => api.engineStatus().then(setEngine).catch(() => {})
    tick()
    const t = setInterval(tick, 3000)
    return () => clearInterval(t)
  }, [])

  useEffect(() => {
    api.getSettings().then((s) => {
      setSettings(s)
      setApps(s.excludedApps.join('\n'))
      setUrls(s.excludedUrlPatterns.join('\n'))
      setFolders(s.indexFolders.join('\n'))
      setBfolder(s.backupFolder)
    })
    api.privacyLists().then(setLists)
    const tick = () => { api.fileStats().then(setFileReport).catch(() => {}); refreshLabels() }
    tick()
    const t = setInterval(tick, 3000)
    return () => clearInterval(t)
  }, [])

  async function save(patch: Partial<Settings>) {
    if (!settings) return
    const next = await api.updateSettings({
      ...settings,
      excludedApps: lines(apps),
      excludedUrlPatterns: lines(urls),
      indexFolders: lines(folders),
      backupFolder: bfolder,
      ...patch,
    })
    setSettings(next)
    setApps(next.excludedApps.join('\n'))
    setUrls(next.excludedUrlPatterns.join('\n'))
    setFolders(next.indexFolders.join('\n'))
    setBfolder(next.backupFolder)
    toast.success('Saved')
  }

  async function wipe() {
    await api.wipeAll()
    setConfirmWipe(false)
    toast.success('All captured memories deleted from this Mac')
    onChanged()
  }

  async function toggleLogin(on: boolean) {
    try {
      await api.setLaunchAtLogin(on)
    } catch (e) {
      toast.error(String(e))
    }
  }

  /* Which card is nearest the top, for the rail. */
  const [lic, setLic] = useLicence()
  const [here, setHere] = useState('licence')
  useEffect(() => {
    const els = SECTIONS.map((x) => document.getElementById(`settings-${x.slug}`)).filter(Boolean) as HTMLElement[]
    if (!els.length) return
    const io = new IntersectionObserver(
      (entries) => {
        const top = entries.filter((e) => e.isIntersecting).sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0]
        if (top) setHere(top.target.id.replace('settings-', ''))
      },
      { rootMargin: '-8% 0px -80% 0px', threshold: 0 },
    )
    els.forEach((el) => io.observe(el))
    return () => io.disconnect()
  }, [settings])

  if (!settings) return null

  return (
    <div className="px-6 py-6 max-w-[1100px]">
      <div className="flex items-end justify-between gap-6 flex-wrap mb-6">
        <div>
          <h1 className="text-[32px] font-semibold tracking-[-0.03em] leading-none">Settings</h1>
          <p className="text-[13.5px] text-muted-foreground mt-2">Control what Lane can see, how it works, and make it truly yours.</p>
        </div>
        <p className="text-[13px] text-muted-foreground italic">“A more thoughtful you, on your terms.”</p>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-[210px_minmax(0,1fr)] gap-5 items-start">
        <SectionRail active={here} onGo={(slug) => { setHere(slug); document.getElementById(`settings-${slug}`)?.scrollIntoView({ behavior: 'smooth', block: 'start' }) }} />

        <div className="space-y-4 min-w-0">
        {lic && (
          <Card id="settings-licence">
            <CardHeader>
              <CardTitle className="text-base">Your licence</CardTitle>
              <CardDescription>Two months free, then $9 a month, $89 a year, or $499 once. Checked on this Mac; no account, nothing sent anywhere.</CardDescription>
            </CardHeader>
            <CardContent>
              <LicenceCard lic={lic} onChanged={setLic} />
            </CardContent>
          </Card>
        )}
        {/* What Lane is doing right now, before any of the switches. */}
        <div className="rounded-2xl border bg-card p-5">
          <div className="flex items-center gap-3 flex-wrap">
            <span className="h-11 w-11 shrink-0 rounded-xl tone-violet grid place-items-center text-primary"><Laptop className="h-5 w-5" /></span>
            <div className="min-w-0">
              <h2 className="text-[17px] font-semibold tracking-tight flex items-center gap-2">
                {status?.paused ? 'Lane is paused' : 'Lane is ready'}
                <span className={cn('h-2 w-2 rounded-full', status?.paused ? 'bg-muted-foreground' : 'bg-emerald-500')} />
              </h2>
              <p className="text-[13px] text-muted-foreground mt-0.5">
                {status?.paused ? 'Nothing is being recorded.' : 'Capturing in the background.'} Everything stays on this Mac.
              </p>
            </div>
            {status && (
              <Button size="sm" variant="outline" className="ml-auto" onClick={() => api.setPaused(!status.paused).then(onChanged)}>
                {status.paused ? 'Resume' : 'Pause'}
              </Button>
            )}
          </div>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-2.5 mt-4">
            <div className="rounded-xl bg-secondary/60 p-3">
              <div className="text-[12px] text-muted-foreground">Memories</div>
              <div className="text-[15px] font-medium mt-0.5 tabular-nums">{status ? status.stats.activities.toLocaleString() : '…'}</div>
            </div>
            <div className="rounded-xl bg-secondary/60 p-3">
              <div className="text-[12px] text-muted-foreground">Files</div>
              <div className="text-[15px] font-medium mt-0.5">{fileReport ? `${fileReport.folders.length} folders` : '…'}</div>
            </div>
            <div className="rounded-xl bg-secondary/60 p-3">
              <div className="text-[12px] text-muted-foreground">Audio</div>
              <div className="text-[15px] font-medium mt-0.5">{settings.autoListenCalls ? 'Calls and meetings' : 'Meetings only'}</div>
            </div>
            <div className="rounded-xl bg-secondary/60 p-3">
              <div className="text-[12px] text-muted-foreground">People</div>
              <div className="text-[15px] font-medium mt-0.5">{settings.contactsEnabled ? 'Contacts on' : 'Contacts off'}</div>
            </div>
          </div>
        </div>

      <Card id="settings-permissions">
        <CardHeader>
          <CardTitle className="text-base">Permissions</CardTitle>
          <CardDescription>What Lane needs from macOS, and what it never asks for.</CardDescription>
        </CardHeader>
        <CardContent className="space-y-5">
          <div>
            <p className="text-sm font-medium mb-2">
              Accessibility <span className="text-xs font-normal text-muted-foreground">· required to read window text</span>
            </p>
            <AccessibilityPanel report={report} />
          </div>
          <div className="flex items-start justify-between gap-6 border-t pt-4">
            <div>
              <Label htmlFor="login">Start Lane when I log in</Label>
              <p className="text-xs text-muted-foreground mt-0.5">
                {report?.bundled
                  ? 'Adds Lane to your login items. You can also manage this in System Settings → General → Login Items.'
                  : 'Available in the installed app, not in development builds.'}
              </p>
            </div>
            <Switch
              id="login"
              checked={!!report?.launchAtLogin}
              disabled={!report?.bundled}
              onCheckedChange={toggleLogin}
            />
          </div>
          <div className="border-t pt-4 text-sm">
            <div className="flex items-center gap-2 font-medium">
              <Lock className="h-3.5 w-3.5" /> Never asked for
            </div>
            <p className="text-xs text-muted-foreground mt-1">
              {NEVER_ASKED.join(', ')}, and internet access. Asked only when you turn the feature on: {ONLY_WHEN_ON.join('; ')}.
            </p>
          </div>
          <Button size="sm" variant="outline" onClick={onRunSetup}>
            Run setup again
          </Button>
        </CardContent>
      </Card>

      <Card id="settings-capture">
        <CardHeader>
          <CardTitle className="text-base">Capture</CardTitle>
          <CardDescription>How often Lane looks at the window in front of you.</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex items-start justify-between gap-6">
            <div>
              <Label htmlFor="notch">Notch strip</Label>
              <p className="text-xs text-muted-foreground mt-0.5">A small tab under the notch. Its dot shows what Lane is doing (green: remembering, red: recording, blue: dictating). Hover for the next event, what you owe, and buttons to record, dictate or ask. Reminders, your brief and live meeting help open it by themselves. Never visible in a screen share.</p>
            </div>
            <Switch id="notch" checked={settings.notchEnabled} onCheckedChange={(v) => save({ notchEnabled: v })} />
          </div>
          {settings.notchEnabled && (
            <NotchPositionPicker value={settings.notchPosition} onChange={(p) => save({ notchPosition: p, notchPositionChosen: true })} className="pl-1" />
          )}
          <div className="flex items-start justify-between gap-6">
            <div>
              <Label htmlFor="tc">Check my thinking while I write</Label>
              <p className="text-xs text-muted-foreground mt-0.5">Off by default. Every few seconds of typing, Lane looks for a sentence you already wrote and for figures that disagree with your own facts, and says so quietly in the notch. Nothing you type is stored.</p>
            </div>
            <Switch id="tc" checked={settings.thoughtCheckEnabled} onCheckedChange={(v) => save({ thoughtCheckEnabled: v })} />
          </div>
          <div className="flex items-start justify-between gap-6">
            <div>
              <Label htmlFor="shots">Keep a picture with each memory</Label>
              <p className="text-xs text-muted-foreground mt-0.5">Off by default. A small screenshot of the window is kept beside the text, so a memory shows what you saw. Needs Screen Recording; pictures live on this Mac and go with the raw text when it is cleared.</p>
              {settings.screenshotsEnabled && perms && !perms.screenRecording && (
                <Button size="sm" variant="outline" className="mt-2" onClick={() => api.requestScreenRecording().then((ok) => toast.message(ok ? 'Screen Recording is on' : 'Allow Lane under Privacy & Security → Screen Recording, then restart Lane'))}>Allow Screen Recording</Button>
              )}
            </div>
            <Switch id="shots" checked={settings.screenshotsEnabled} onCheckedChange={(v) => save({ screenshotsEnabled: v }).then(() => { if (v) api.requestScreenRecording().catch(() => {}) })} />
          </div>
          <p className="text-xs text-muted-foreground">Dictation: ⌥⇧Space to start, again to insert what you said into the field you are in. Also in the menu bar and the notch card. Every dictation is kept as a Voice note memory too.</p>
          <div className="grid grid-cols-2 gap-4 max-w-md">
            <div className="space-y-1.5">
              <Label htmlFor="mbrief">Morning brief at</Label>
              <Input id="mbrief" defaultValue={settings.morningBriefAt} placeholder="07:00 · empty = off" onBlur={(e) => e.target.value !== settings.morningBriefAt && save({ morningBriefAt: e.target.value })} />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="ebrief">Day so far at</Label>
              <Input id="ebrief" defaultValue={settings.eveningBriefAt} placeholder="19:00 · empty = off" onBlur={(e) => e.target.value !== settings.eveningBriefAt && save({ eveningBriefAt: e.target.value })} />
            </div>
            <p className="col-span-2 text-xs text-muted-foreground -mt-2">A notification with the first line; Today has the rest. Times are local, 24-hour.</p>
          </div>

          <div className="flex items-start justify-between gap-6">
            <div>
              <Label htmlFor="clip">Remember what I copy</Label>
              <p className="text-xs text-muted-foreground mt-0.5">
                Text you copy becomes memory too: quotes, figures, addresses, paragraphs. Off by default. Anything that looks like a password or key is never stored, and copies made in excluded apps and sites are skipped.
              </p>
            </div>
            <Switch id="clip" checked={settings.clipboardEnabled} onCheckedChange={(v) => save({ clipboardEnabled: v })} />
          </div>

          <div className="flex items-center justify-between">
            <Label htmlFor="paused">Capturing</Label>
            <Switch
              id="paused"
              checked={!status?.paused}
              onCheckedChange={(on) => api.setPaused(!on).then(onChanged)}
            />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-1.5">
              <Label htmlFor="interval">Check every (seconds)</Label>
              <Input
                id="interval"
                type="number"
                min={2}
                max={60}
                defaultValue={settings.intervalSecs}
                onBlur={(e) => save({ intervalSecs: Number(e.target.value) })}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="idle">Stop after idle (minutes)</Label>
              <Input
                id="idle"
                type="number"
                min={1}
                max={60}
                defaultValue={settings.idleMinutes}
                onBlur={(e) => save({ idleMinutes: Number(e.target.value) })}
              />
            </div>
          </div>
          <div className="flex items-center justify-between gap-4">
            <div>
              <Label htmlFor="web">Read page text in Chrome, Edge, Brave and Arc</Label>
              <p className="text-xs text-muted-foreground mt-0.5">
                Chrome-based browsers only share page text with accessibility tools when asked. Safari and most Mac apps share it anyway.
              </p>
            </div>
            <Switch
              id="web"
              checked={settings.readBrowserText}
              onCheckedChange={(on) => save({ readBrowserText: on })}
            />
          </div>
        </CardContent>
      </Card>

      <Card id="settings-engine">
        <CardHeader>
          <CardTitle className="text-base">Rabbit</CardTitle>
          <CardDescription>
            Our own small language model. It reads your day, writes every memory and answers your questions, here on
            this Mac. It ships with Lane and fetches its weights once (about 2.5 GB); after that nothing is sent anywhere.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="text-sm">
            <span className={engine?.available ? 'text-primary' : engine?.downloadPercent != null ? 'text-amber-600' : 'text-destructive'}>●</span>{' '}
            {engine
              ? engine.downloadPercent != null
                ? `${engine.detail} · ${engine.downloadPercent}%`
                : engine.available
                  ? engine.busy
                    ? engine.detail
                    : 'Ready'
                  : engine.detail
              : 'Checking…'}
            {engine && (
              <span className="text-muted-foreground">
                {' '}· {engine.counts.memories} memories · {engine.counts.pending} waiting · {engine.processed} made since launch
                {engine.tokensPerSecond > 0 && ` · ${engine.tokensPerSecond.toFixed(0)} tokens/s`}
                {engine.dropped > 0 && ` · ${engine.dropped} invented strings dropped`}
                {engine.backend === 'ollama' && ' · development fallback (Ollama)'}
              </span>
            )}
            {engine?.downloadPercent != null && (
              <div className="mt-2 h-1.5 w-full rounded-full bg-muted overflow-hidden">
                <div className="h-full bg-primary transition-all" style={{ width: `${engine.downloadPercent}%` }} />
              </div>
            )}
            {engine?.lastError && <p className="text-xs text-destructive mt-1">{engine.lastError}</p>}
          </div>
          <div className="grid grid-cols-2 gap-4 items-end">
            <div className="space-y-1.5">
              <Label htmlFor="model">Build</Label>
              <div className="flex gap-2">
                <Button size="sm" variant={settings.model === RABBIT_S ? 'default' : 'outline'} onClick={() => settings.model !== RABBIT_S && save({ model: RABBIT_S })}>Rabbit S</Button>
                <Button size="sm" variant={settings.model === RABBIT_M ? 'default' : 'outline'} onClick={() => settings.model !== RABBIT_M && save({ model: RABBIT_M })}>Rabbit M</Button>
              </div>
              <p className="text-xs text-muted-foreground">
                S suits an 8 GB Mac, M a 16 GB one and up. Lane picks for you on first launch; switching re-fetches the
                weights once. The advanced field below takes any local build.
              </p>
              <Input id="model" defaultValue={settings.model} onBlur={(e) => e.target.value !== settings.model && save({ model: e.target.value })} className="font-mono text-xs" />
            </div>
            <div className="flex flex-col gap-2">
              <Button size="sm" variant="outline" onClick={() => api.processNow()}>
                Process waiting activities now
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => api.exportLabels().then((p) => toast.success(`Labels written to ${p}`)).catch((e) => toast.error(String(e)))}
              >
                Export my thumbs as training labels
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card id="settings-files">
        <CardHeader>
          <CardTitle className="text-base">Files</CardTitle>
          <CardDescription>
            Documents in these folders are indexed by their contents so you can find and ask about them. Reading happens on this Mac;
            macOS asks once per folder.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="flex items-start justify-between gap-6">
            <div>
              <Label htmlFor="idx">Index documents</Label>
              {fileReport && (
                <p className="text-xs text-muted-foreground mt-0.5">
                  {fileReport.files.toLocaleString()} documents · {fileReport.chunks.toLocaleString()} sections · {fileReport.detail}
                </p>
              )}
            </div>
            <Switch id="idx" checked={settings.indexFiles} onCheckedChange={(v) => save({ indexFiles: v })} />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="folders">Folders (one per line; empty = Desktop, Documents, Downloads)</Label>
            <Textarea id="folders" rows={4} value={folders} onChange={(e) => setFolders(e.target.value)} placeholder={fileReport?.folders.join('\n')} />
            <div className="flex gap-2">
              <Button size="sm" onClick={() => save({})}>Save folders</Button>
              <Button size="sm" variant="outline" onClick={() => api.reindexFiles()}>Rescan now</Button>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card id="settings-why">
        <CardHeader>
          <CardTitle className="text-base">Your why</CardTitle>
          <CardDescription>What you are building and your principles. Lane ranks each day's three things against it, scores every day for closeness, and the thought check flags writing that crosses a principle.</CardDescription>
        </CardHeader>
        <CardContent>
          <WhyLine settings={settings} onSaved={setSettings} />
        </CardContent>
      </Card>

      <Card id="settings-meetings">
        <CardHeader>
          <CardTitle className="text-base">Meetings</CardTitle>
          <CardDescription>Recording is manual. Transcription runs on this Mac with a downloaded speech model (about 490 MB).</CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="flex items-start justify-between gap-6">
            <div>
              <Label htmlFor="keepaudio">Keep audio files after transcription</Label>
              <p className="text-xs text-muted-foreground mt-0.5">Off: audio is deleted once the transcript is saved.</p>
            </div>
            <Switch id="keepaudio" checked={settings.keepAudio} onCheckedChange={(v) => save({ keepAudio: v })} />
          </div>
          <div className="flex items-start justify-between gap-6">
            <div>
              <Label htmlFor="autocall">Listen during calls by itself</Label>
              <p className="text-xs text-muted-foreground mt-0.5">Off by default. When Zoom, Teams, Meet, Webex, FaceTime or a Slack huddle is in front, recording starts on its own and stops two minutes after the call window is gone. Live help shows in the notch; notes and a memory follow. Off: the notch only asks "In a call?" so one click starts it.</p>
            </div>
            <Switch id="autocall" checked={settings.autoListenCalls} onCheckedChange={(v) => save({ autoListenCalls: v })} />
          </div>
          <div className="flex items-start justify-between gap-6">
            <div>
              <Label htmlFor="diar">Tell the other voices apart</Label>
              <p className="text-xs text-muted-foreground mt-0.5">Off by default. The other side of a call is split by voice into Speaker 1, Speaker 2… which you can name on the Meetings page. Downloads a speaker model once (about 60 MB); runs on this Mac.</p>
            </div>
            <Switch id="diar" checked={settings.diarizeEnabled} onCheckedChange={(v) => save({ diarizeEnabled: v }).then(() => { if (v) { toast.message('Fetching the speaker model…'); api.speakerToolkit(true).then((t) => toast.success(t.detail)).catch((e) => toast.error(String(e))) } })} />
          </div>
          <div className="space-y-1.5 max-w-md">
            <Label htmlFor="mdfolder">Also write briefings, meeting notes and captures as Markdown to</Label>
            <Input id="mdfolder" defaultValue={settings.markdownFolder} placeholder="e.g. your Obsidian vault folder" onBlur={(e) => e.target.value !== settings.markdownFolder && save({ markdownFolder: e.target.value })} />
            <p className="text-xs text-muted-foreground">Creates Lane/Daily, Lane/Weekly, Lane/Meetings and Lane/Notes there. Add the same folder above to search it too. Leave empty to turn off.</p>
          </div>
          <div className="space-y-1.5 max-w-xs">
            <Label htmlFor="lang">Spoken language</Label>
            <Input id="lang" defaultValue={settings.meetingLanguage} placeholder="auto" onBlur={(e) => e.target.value !== settings.meetingLanguage && save({ meetingLanguage: e.target.value || 'auto' })} />
            <p className="text-xs text-muted-foreground">auto, or a code such as en, hi, as, bn. Auto works for mixed English and Hindi.</p>
          </div>
        </CardContent>
      </Card>


      <Card id="settings-privacy">
        <CardHeader>
          <CardTitle className="text-base">Never capture</CardTitle>
          <CardDescription>Matching windows are skipped before anything is saved.</CardDescription>
        </CardHeader>
        <CardContent className="space-y-1 pb-2">
          <div className="flex items-start justify-between gap-6 py-2">
            <div>
              <Label htmlFor="msg">Private messaging</Label>
              <p className="text-xs text-muted-foreground mt-0.5">{lists?.messagingApps.join(', ')} and their web versions</p>
            </div>
            <Switch id="msg" checked={settings.excludeMessaging} onCheckedChange={(v) => save({ excludeMessaging: v })} />
          </div>
          <div className="flex items-start justify-between gap-6 py-2">
            <div>
              <Label htmlFor="mail">Email</Label>
              <p className="text-xs text-muted-foreground mt-0.5">{lists?.emailApps.join(', ')}, Gmail and Outlook on the web</p>
            </div>
            <Switch id="mail" checked={settings.excludeEmail} onCheckedChange={(v) => save({ excludeEmail: v })} />
          </div>
          <div className="flex items-start justify-between gap-6 py-2">
            <div>
              <Label htmlFor="contacts">Hide email addresses and phone numbers</Label>
              <p className="text-xs text-muted-foreground mt-0.5">Replaced with [email] and [phone] before anything is saved. Card numbers are always hidden.</p>
            </div>
            <Switch id="contacts" checked={settings.redactContacts} onCheckedChange={(v) => save({ redactContacts: v })} />
          </div>
          <details className="text-xs text-muted-foreground py-2">
            <summary className="cursor-pointer">Always skipped (can't be changed)</summary>
            <p className="mt-1.5 leading-relaxed">{lists?.alwaysSkipped.join(', ')}</p>
          </details>
        </CardContent>
        <CardContent className="grid grid-cols-2 gap-4 border-t pt-4">
          <div className="space-y-1.5">
            <Label htmlFor="apps">Also skip these apps (exact name, one per line)</Label>
            <Textarea id="apps" rows={8} value={apps} onChange={(e) => setApps(e.target.value)} />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="urls">…and web addresses containing</Label>
            <Textarea
              id="urls"
              rows={8}
              value={urls}
              placeholder={'mybank.com\nmail.google.com'}
              onChange={(e) => setUrls(e.target.value)}
            />
          </div>
          <div className="col-span-2">
            <Button size="sm" onClick={() => save({})}>
              Save exclusions
            </Button>
          </div>
        </CardContent>
      </Card>

      <Card id="settings-labels">
        <CardHeader>
          <CardTitle className="text-base">Help Rabbit learn</CardTitle>
          <CardDescription>
            Your thumbs teach Rabbit what is worth remembering. When this is on, Lane sends each memory you've thumbed — your verdict,
            what Rabbit said, and the screen text it judged — to Lane, about every 6 hours. This is the only thing Lane ever sends
            besides your own backups. Off by default. You can review and remove items below before they go.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="flex items-start justify-between gap-6">
            <div>
              <Label htmlFor="contrib">Contribute my thumbs</Label>
              <p className="text-xs text-muted-foreground mt-0.5">
                {labels ? `${labels.unsent.length} waiting · last sent ${labels.lastSentAt ? format(labels.lastSentAt, 'd MMM HH:mm') : 'never'}${labels.detail ? ` · ${labels.detail}` : ''}` : ''}
              </p>
            </div>
            <Switch id="contrib" checked={settings.contributeLabels} onCheckedChange={(v) => save({ contributeLabels: v })} />
          </div>
          <div className="grid grid-cols-2 gap-4 items-end max-w-md">
            <div className="space-y-1.5">
              <Label htmlFor="tcode">Tester code</Label>
              <Input id="tcode" defaultValue={settings.testerToken} placeholder="from your invite" onBlur={(e) => e.target.value !== settings.testerToken && save({ testerToken: e.target.value })} />
            </div>
            <Button size="sm" variant="outline" disabled={!settings.contributeLabels} onClick={() => api.sendLabelsNow().then((n) => { toast.success(n ? `Sent ${n} labels. Thank you.` : 'Nothing new to send'); refreshLabels() }).catch((e) => toast.error(String(e)))}>
              Send now
            </Button>
          </div>
          {labels && labels.unsent.length > 0 && (
            <details className="text-xs">
              <summary className="cursor-pointer text-muted-foreground">Review what would be sent ({labels.unsent.length})</summary>
              <ul className="mt-2 space-y-1 max-h-48 overflow-y-auto">
                {labels.unsent.map(([id, title, fb]) => (
                  <li key={id} className="flex items-center gap-2">
                    <span className={fb === 'keep' ? 'text-primary' : 'text-destructive'}>{fb === 'keep' ? '👍' : '👎'}</span>
                    <span className="truncate flex-1">{title}</span>
                    <button className="text-muted-foreground hover:text-destructive" onClick={() => api.excludeLabel(id).then(refreshLabels)}>never send</button>
                  </li>
                ))}
              </ul>
            </details>
          )}
        </CardContent>
      </Card>

      <BackupCard folder={bfolder} onFolderChange={setBfolder} onSaveFolder={() => save({})} />

      <Card id="settings-about">
        <CardHeader>
          <CardTitle className="text-base">About Lane</CardTitle>
          <CardDescription>Lane is powered by Rabbit, our own memory model, running entirely on this Mac.</CardDescription>
        </CardHeader>
        <CardContent className="text-xs text-muted-foreground space-y-2">
          <UpdateCard />
          <div className="flex items-center gap-3 pt-1">
            <Button size="sm" variant="outline" onClick={() => api.diagnosticsBundle().then((p) => toast.success(`Saved ${p.split('/').pop()} to your Desktop. Attach it to your message; it holds settings and logs, no screen text or memories.`)).catch((e) => toast.error(String(e)))}>
              Report a problem
            </Button>
            <span className="text-xs text-muted-foreground">Writes a diagnostics file to your Desktop. Nothing is sent.</span>
          </div>
          <p>Version 0.1.0 · lane.so</p>
          <details>
            <summary className="cursor-pointer">Open-source licences</summary>
            <p className="mt-2 leading-relaxed">
              Lane includes software under the MIT and Apache 2.0 licences, including work by the ggml authors, Alibaba Cloud (Qwen),
              Nomic AI, OpenAI (Whisper), the Tauri, Radix, React and Rust communities. Full licence texts are included in the app
              bundle under Contents/Resources.
            </p>
          </details>
        </CardContent>
      </Card>

      <Card id="settings-data">
        <CardHeader>
          <CardTitle className="text-base">Your data</CardTitle>
          <CardDescription>Stored only on this Mac, encrypted. Nothing is sent anywhere.</CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="space-y-1.5 max-w-md">
            <Label htmlFor="retention">Keep raw screen text for</Label>
            <div className="flex items-center gap-2">
              <Input id="retention" type="number" min={0} max={3650} className="w-24" defaultValue={settings.rawRetentionDays} onBlur={(e) => { const v = Math.max(0, Number(e.target.value) || 0); if (v !== settings.rawRetentionDays) save({ rawRetentionDays: v }) }} />
              <span className="text-sm text-muted-foreground">days · 0 keeps it forever</span>
            </div>
            <p className="text-xs text-muted-foreground">
              This is the verbatim text Lane read off your screen, kept so you can look back at the exact wording. After
              this many days that raw text is deleted and the space comes back.
            </p>
            <p className="text-xs text-muted-foreground">
              Your memory itself is never trimmed: the memories, facts, tasks, people, dates and links made from that
              text are kept for as long as you keep Lane, as are meeting transcripts, notes and anything from a
              connected source. Three years from now Lane will still know what happened today — it just will not be able
              to quote the screen back to you word for word. Set 0 to keep the raw text too.
            </p>
          </div>
          <div className="space-y-1.5 max-w-md border-t pt-3">
            <Label htmlFor="forget">Forget a name or word everywhere</Label>
            <div className="flex gap-2">
              <Input id="forget" value={forget} placeholder="e.g. a person, a company, a project" onChange={(e) => setForget(e.target.value)} />
              <Button size="sm" variant="destructive" disabled={forget.trim().length < 2} onClick={() => setConfirmForget(true)}>Forget</Button>
            </div>
            <p className="text-xs text-muted-foreground">Removes it from people and organisations, memory titles and summaries, facts, tasks, raw text, briefings and indexed documents. Cannot be undone.</p>
          </div>

          {status && (
            <p className="text-sm">
              {status.stats.activities.toLocaleString()} activities · {status.stats.snapshots.toLocaleString()} text
              snapshots · {formatBytes(status.stats.dbSizeBytes)}
            </p>
          )}
          <p className="text-xs text-muted-foreground break-all select-text">{status?.dbPath}</p>
          <Button size="sm" variant="destructive" onClick={() => setConfirmWipe(true)}>
            Delete everything
          </Button>
        </CardContent>
      </Card>

      <Dialog open={confirmForget} onOpenChange={setConfirmForget}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Forget “{forget.trim()}” everywhere?</DialogTitle>
            <DialogDescription>Every mention is removed or blanked. Lane will not know this name afterwards. This cannot be undone.</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setConfirmForget(false)}>Cancel</Button>
            <Button variant="destructive" onClick={() => api.forgetTerm(forget).then((r) => { toast.success(`Forgotten: ${r.entities} people or organisations, ${r.memories} memories, ${r.facts} facts, ${r.tasks} tasks, ${r.snapshots} screen texts, ${r.files} document sections`); setForget(''); setConfirmForget(false); onChanged() }).catch((e) => toast.error(String(e)))}>
              Forget
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={confirmWipe} onOpenChange={setConfirmWipe}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete all captured memories?</DialogTitle>
            <DialogDescription>This removes every activity and snapshot from this Mac. It can't be undone.</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setConfirmWipe(false)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={wipe}>
              Delete everything
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
        </div>
      </div>
    </div>
  )
}
