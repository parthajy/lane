import { useEffect, useState } from 'react'
import { Check, Lock, X } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import { Label } from '@/components/ui/label'
import { AccessibilityPanel, accessState, usePermissions } from '@/components/accessibility-panel'
import { api, type PrivacyLists, type Settings } from '@/lib/api'
import { RestoreRow } from '@/components/backup-card'
import { cn } from '@/lib/utils'
import { NotchPositionPicker } from '@/components/notch-position'

const STEPS = ['Welcome', 'Access', 'Privacy', 'Notch', 'Your name', 'Finish'] as const

export const NEVER_ASKED = ['Screen Recording', 'Camera', 'Full Disk Access', 'Location']
export const ONLY_WHEN_ON = ['Microphone and System Audio (meetings)', 'Calendars (meeting prep)', 'Contacts (names)', 'Mail automation (Apple Mail)']

function Row({ ok, children }: { ok: boolean; children: React.ReactNode }) {
  return (
    <li className="flex gap-2.5 text-sm leading-relaxed">
      {ok ? (
        <Check className="h-4 w-4 text-primary shrink-0 mt-0.5" />
      ) : (
        <X className="h-4 w-4 text-destructive shrink-0 mt-0.5" />
      )}
      <span>{children}</span>
    </li>
  )
}

function ToggleRow({
  id,
  label,
  hint,
  checked,
  disabled,
  onChange,
}: {
  id: string
  label: string
  hint?: string
  checked: boolean
  disabled?: boolean
  onChange: (v: boolean) => void
}) {
  return (
    <div className="flex items-start justify-between gap-6 py-3 border-b last:border-b-0">
      <div>
        <Label htmlFor={id} className="text-sm font-medium">
          {label}
        </Label>
        {hint && <p className="text-xs text-muted-foreground mt-0.5 leading-relaxed">{hint}</p>}
      </div>
      <Switch id={id} checked={checked} disabled={disabled} onCheckedChange={onChange} />
    </div>
  )
}

export function Onboarding({ initialStep = 0, onDone }: { initialStep?: number; onDone: () => void }) {
  const [step, setStep] = useState(initialStep)
  const [settings, setSettings] = useState<Settings | null>(null)
  const [lists, setLists] = useState<PrivacyLists | null>(null)
  const report = usePermissions(step === 1 ? 1000 : 3000)
  const [notchApps, setNotchApps] = useState<string[]>([])
  const [name, setName] = useState('')
  const [osName, setOsName] = useState('')

  useEffect(() => {
    api.status().then((st) => { setOsName(st.userName ?? ''); setName((n) => n || st.userName || '') }).catch(() => {})
  }, [])

  function saveName() {
    const next = settings ? { ...settings, displayName: name.trim() } : null
    if (next) {
      setSettings(next)
      api.updateSettings(next).catch((e) => toast.error(String(e)))
    }
    setStep(5)
  }
  useEffect(() => { api.notchContext().then((c) => setNotchApps(c.notchApps)).catch(() => {}) }, [])
  const access = accessState(report)

  useEffect(() => {
    api.getSettings().then(setSettings)
    api.privacyLists().then(setLists)
  }, [])

  async function finish() {
    if (!settings) return
    await api.updateSettings({ ...settings, onboardingDone: true })
    onDone()
  }

  async function toggleLogin(on: boolean) {
    try {
      await api.setLaunchAtLogin(on)
    } catch (e) {
      toast.error(String(e))
    }
  }

  return (
    <div className="enterprise-shell fixed inset-0 z-50 bg-background overflow-y-auto">
      <div className="max-w-2xl mx-auto px-8 py-10">
        <div className="flex items-center gap-2 mb-8">
          <img src="/dark.png" alt="" className="h-6 w-6 object-contain dark:invert" />
          <span className="font-semibold tracking-tight">Lane</span>
          <div className="ml-auto flex items-center gap-1.5">
            {STEPS.map((label, i) => (
              <button
                key={label}
                onClick={() => i <= step && setStep(i)}
                className={cn(
                  'text-xs px-2.5 py-1 rounded-full transition-colors',
                  i === step ? 'bg-primary text-primary-foreground' : i < step ? 'text-foreground' : 'text-muted-foreground',
                )}
              >
                {i + 1}. {label}
              </button>
            ))}
          </div>
        </div>

        {step === 0 && (
          <section className="space-y-6 animate-fade-in">
            <div>
              <h1 className="text-2xl font-semibold tracking-tight">Your work, remembered. On this Mac only.</h1>
              <p className="text-muted-foreground mt-2 leading-relaxed">
                Lane quietly keeps a searchable memory of what you work on, so you can find that page, document or
                number later. Setup takes about two minutes.
              </p>
            </div>
            <div className="grid grid-cols-2 gap-4">
              <div className="rounded-2xl border bg-card p-4">
                <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground mb-3">What it saves</p>
                <ul className="space-y-2">
                  <Row ok>Which apps and windows you use, and for how long</Row>
                  <Row ok>Text on screen in those windows: pages, documents, notes</Row>
                  <Row ok>Web addresses of pages you visit</Row>
                  <Row ok>What's inside the documents in Desktop, Documents and Downloads</Row>
                </ul>
              </div>
              <div className="rounded-2xl border bg-card p-4">
                <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground mb-3">What it never does</p>
                <ul className="space-y-2">
                  <Row ok={false}>Send anything off this Mac. No account, no cloud. Encrypted on disk</Row>
                  <Row ok={false}>Read password fields or record keystrokes</Row>
                  <Row ok={false}>Take screenshots, or record audio unless you press Record for a meeting</Row>
                  <Row ok={false}>Look at password managers, login or Touch ID prompts</Row>
                </ul>
              </div>
            </div>
            <Button size="lg" onClick={() => setStep(1)}>
              Get started
            </Button>
            <details className="text-sm">
              <summary className="cursor-pointer text-muted-foreground">Moving from another Mac? Restore a backup</summary>
              <div className="mt-3 rounded-2xl border bg-card p-4">
                <RestoreRow compact />
              </div>
            </details>
          </section>
        )}

        {step === 1 && (
          <section className="space-y-6 animate-fade-in">
            <div>
              <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">Required · one time</p>
              <h1 className="text-2xl font-semibold tracking-tight mt-1">Let Lane read on-screen text</h1>
              <p className="text-muted-foreground mt-2 leading-relaxed">
                macOS protects on-screen text with the <span className="text-foreground font-medium">Accessibility</span>{' '}
                permission. No app is allowed to switch it on by itself, so you need to flip one switch. Lane
                opens the right page and notices as soon as it's on.
              </p>
            </div>
            <AccessibilityPanel report={report} />
            <div className="flex items-center gap-3 pt-2">
              <Button disabled={access !== 'on'} onClick={() => setStep(2)}>
                Continue
              </Button>
              {access !== 'on' && (
                <button className="text-sm text-muted-foreground underline" onClick={() => setStep(2)}>
                  Skip for now (only app names will be recorded)
                </button>
              )}
            </div>
          </section>
        )}

        {step === 2 && settings && (
          <section className="space-y-6 animate-fade-in">
            <div>
              <h1 className="text-2xl font-semibold tracking-tight">Choose what Lane skips</h1>
              <p className="text-muted-foreground mt-2">You can add any app or website later in Settings.</p>
            </div>
            <div className="rounded-2xl border bg-card px-4">
              <ToggleRow
                id="always"
                label="Password managers, login and Touch ID prompts, lock screen"
                hint="Always skipped. This can't be turned off."
                checked
                disabled
                onChange={() => {}}
              />
              <ToggleRow
                id="messaging"
                label="Skip private messaging"
                hint={lists ? [...lists.messagingApps, 'web versions'].join(', ') : undefined}
                checked={settings.excludeMessaging}
                onChange={(v) => setSettings({ ...settings, excludeMessaging: v })}
              />
              <ToggleRow
                id="email"
                label="Skip email"
                hint={lists ? [...lists.emailApps, 'Gmail and Outlook on the web'].join(', ') : undefined}
                checked={settings.excludeEmail}
                onChange={(v) => setSettings({ ...settings, excludeEmail: v })}
              />
              <ToggleRow
                id="web"
                label="Read page text in Chrome, Edge, Brave and Arc"
                hint="Chrome-based browsers only share page text with accessibility tools when asked. Safari and most Mac apps share it anyway."
                checked={settings.readBrowserText}
                onChange={(v) => setSettings({ ...settings, readBrowserText: v })}
              />
            </div>
            <Button
              onClick={async () => {
                try {
                  setSettings(await api.updateSettings(settings))
                  setStep(3)
                } catch (e) {
                  toast.error(String(e))
                }
              }}
            >
              Continue
            </Button>
          </section>
        )}

        {step === 3 && settings && (
          <section className="space-y-6 animate-fade-in">
            <div>
              <h1 className="text-2xl font-semibold tracking-tight">Where should Lane's tab live?</h1>
              <p className="text-muted-foreground mt-2 leading-relaxed">A small tab that shows what Lane is doing and opens into a card: your next meeting, what you owe, live help in calls, and what you know about the page in front. Pick a spot; change it any time in Settings → Notch.</p>
            </div>
            <div className="rounded-2xl border bg-card p-4">
              <NotchPositionPicker value={settings.notchPosition} onChange={(p) => { const next = { ...settings, notchPosition: p, notchPositionChosen: true }; setSettings(next); api.updateSettings(next).catch((e) => toast.error(String(e))) }} />
              {notchApps.length > 0 && <p className="text-xs text-amber-700 dark:text-amber-400 mt-3">{notchApps.join(', ')} is running and uses the notch. A side or corner spot keeps them apart.</p>}
            </div>
            <Button size="lg" onClick={() => setStep(4)}>Continue</Button>
          </section>
        )}

        {step === 4 && settings && (
          <section className="space-y-6 animate-fade-in">
            <div>
              <h1 className="text-2xl font-semibold tracking-tight">What should Lane call you?</h1>
              <p className="text-muted-foreground mt-2 leading-relaxed">
                Lane speaks to you by name and, more importantly, knows which name in your memories means you. Your Mac
                calls you {osName || 'nothing in particular'}; change it if you go by something else. It stays on this Mac
                like everything else.
              </p>
            </div>
            <div className="rounded-2xl border bg-card p-4">
              <input
                autoFocus
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder={osName || 'Your name'}
                onKeyDown={(e) => { if (e.key === 'Enter') saveName() }}
                className="w-full h-12 rounded-xl border bg-background px-4 text-[16px] outline-none focus:ring-2 focus:ring-ring"
              />
              <p className="text-xs text-muted-foreground mt-2">First name is plenty. Leave it as it is to keep {osName || 'the account name'}.</p>
            </div>
            <Button size="lg" onClick={saveName}>Continue</Button>
          </section>
        )}

        {step === 5 && (
          <section className="space-y-6 animate-fade-in">
            <div>
              <h1 className="text-2xl font-semibold tracking-tight">You're set{name ? `, ${name.split(' ')[0]}` : ''}</h1>
              <p className="text-muted-foreground mt-2 leading-relaxed">
                Lane lives in your menu bar, at the top right of the screen. Closing this window doesn't stop it.
                To stop, use <span className="text-foreground">Pause</span> or <span className="text-foreground">Quit</span>{' '}
                from the menu bar icon.
              </p>
            </div>

            <div className="rounded-2xl border bg-card px-4">
              <ToggleRow
                id="login"
                label="Start Lane when I log in"
                hint={
                  report?.bundled
                    ? 'macOS may show a “Background item added” notice. That’s expected.'
                    : 'Available in the installed app, not in development builds.'
                }
                checked={!!report?.launchAtLogin}
                disabled={!report?.bundled}
                onChange={toggleLogin}
              />
              <div className="py-3 text-sm">
                <div className="flex items-center gap-2 font-medium">
                  <Lock className="h-3.5 w-3.5" /> Permissions Lane never asks for
                </div>
                <p className="text-xs text-muted-foreground mt-1 leading-relaxed">
                  {NEVER_ASKED.join(', ')}, and internet access. If you ever see a request for one of these, it isn't
                  from Lane. Asked only when you turn the feature on: {ONLY_WHEN_ON.join('; ')}. macOS will ask once each for your Desktop, Documents and Downloads folders so documents can be
                  indexed; that's expected.
                </p>
              </div>
            </div>

            {access !== 'on' && (
              <p className="text-sm text-destructive">
                Accessibility is still off, so only app names will be recorded.{' '}
                <button className="underline" onClick={() => setStep(1)}>
                  Turn it on
                </button>
              </p>
            )}

            <Button size="lg" onClick={finish}>
              Start remembering
            </Button>
          </section>
        )}
      </div>
    </div>
  )
}
