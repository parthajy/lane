import { useEffect, useState } from 'react'
import { format } from 'date-fns'
import { BookText, Boxes, Database, FileText, Laptop, Plug } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Switch } from '@/components/ui/switch'
import { SourceIcon } from '@/components/source-icon'
import { cn } from '@/lib/utils'
import { api, type ConnectorReport, type McpConfig, type NotionStatus, type Settings } from '@/lib/api'

function Panel({ icon: Icon, title, desc, children, className }: { icon: React.ComponentType<{ className?: string }>; title: string; desc?: React.ReactNode; children?: React.ReactNode; className?: string }) {
  return (
    <section className={cn('rounded-2xl border bg-card p-5', className)}>
      <div className="flex items-start gap-3.5 mb-4">
        <span className="h-11 w-11 shrink-0 rounded-xl tone-violet grid place-items-center text-primary"><Icon className="h-5 w-5" /></span>
        <div className="min-w-0">
          <h2 className="text-[17px] font-semibold tracking-tight">{title}</h2>
          {desc && <p className="text-[13px] text-muted-foreground mt-1 leading-relaxed">{desc}</p>}
        </div>
      </div>
      {children}
    </section>
  )
}

/** Every outside source and tool, each off until turned on, each talking only to the service you chose with your own key. */
export function IntegrationsPage() {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [notion, setNotion] = useState<NotionStatus | null>(null)
  const [notionToken, setNotionToken] = useState('')
  const [connectors, setConnectors] = useState<ConnectorReport[]>([])
  const [mcp, setMcp] = useState<McpConfig | null>(null)
  const [secretFor, setSecretFor] = useState<string | null>(null)
  const [secret, setSecret] = useState('')
  const refreshNotion = () => api.notionStatus().then(setNotion).catch(() => {})
  const refreshConnectors = () => api.listConnectors().then(setConnectors).catch(() => {})

  useEffect(() => {
    api.getSettings().then(setSettings)
    api.mcpConfig().then(setMcp).catch(() => {})
    const tick = () => { refreshNotion(); refreshConnectors() }
    tick()
    const t = setInterval(tick, 4000)
    return () => clearInterval(t)
  }, [])

  async function save(patch: Partial<Settings>) {
    if (!settings) return
    const next = await api.updateSettings({ ...settings, ...patch })
    setSettings(next)
    toast.success('Saved')
  }

  if (!settings) return null

  return (
    <div className="px-6 py-6 max-w-6xl">
      <div className="flex items-start gap-4 mb-6">
        <span className="h-14 w-14 shrink-0 rounded-2xl tone-violet grid place-items-center text-primary"><Plug className="h-7 w-7" /></span>
        <div className="min-w-0">
          <h1 className="text-[32px] font-semibold tracking-[-0.03em] leading-none">Integrations</h1>
          <p className="text-[13.5px] text-muted-foreground mt-2 max-w-[70ch]">Each one talks only to the service you connect, with your own key, from this Mac. Nothing goes through Lane's servers. Off until you turn it on.</p>
        </div>
      </div>
      <div className="grid grid-cols-1 xl:grid-cols-2 gap-4 items-start">

      <Panel icon={Laptop} title="On this Mac" desc="Apple apps, read locally. macOS asks once per source.">
        <div className="space-y-4">
          <div className="flex items-start gap-3">
            <SourceIcon app="Calendar" size={32} className="mt-0.5 rounded-lg" />
            <div className="min-w-0 flex-1">
              <Label htmlFor="cal">Calendar</Label>
              <p className="text-xs text-muted-foreground mt-0.5">Upcoming events, so Today can prepare you for each meeting and recordings are named after them.</p>
            </div>
            <Switch id="cal" checked={settings.calendarEnabled} onCheckedChange={(v) => save({ calendarEnabled: v }).then(() => { if (v) api.upcomingEvents(1).catch((e) => toast.error(String(e))) })} />
          </div>
          {settings.calendarEnabled && (
            <div className="flex items-start justify-between gap-6 pl-[44px]">
              <div>
                <Label htmlFor="mnotif">Remind me 15 minutes before a meeting</Label>
                <p className="text-xs text-muted-foreground mt-0.5">A notification with who and what, and a nudge to open Today → Prepare.</p>
              </div>
              <Switch id="mnotif" checked={settings.meetingNotifications} onCheckedChange={(v) => save({ meetingNotifications: v })} />
            </div>
          )}
          <div className="flex items-start gap-3 border-t pt-4">
            <SourceIcon app="Contacts" size={32} className="mt-0.5 rounded-lg" />
            <div className="min-w-0 flex-1">
              <Label htmlFor="contacts">Contacts</Label>
              <p className="text-xs text-muted-foreground mt-0.5">
                First names and nicknames, so "Sarah" means the right Sarah. Read once a day; nothing is kept beyond the aliases{settings.contactsLastSyncAt ? ` · last read ${format(settings.contactsLastSyncAt, 'd MMM HH:mm')}` : ''}.
              </p>
            </div>
            <Switch id="contacts" checked={settings.contactsEnabled} onCheckedChange={(v) => save({ contactsEnabled: v })} />
          </div>
          <div className="flex items-start gap-3 border-t pt-4">
            <SourceIcon app="Mail" size={32} className="mt-0.5 rounded-lg" />
            <div className="min-w-0 flex-1">
              <Label htmlFor="mail">Apple Mail</Label>
              <p className="text-xs text-muted-foreground mt-0.5">Inbox and Sent of every account, read through Mail itself (macOS asks once to let Lane control Mail). Messages become searchable and citable; no Full Disk Access needed.</p>
            </div>
            <Switch id="mail" checked={settings.mailEnabled} onCheckedChange={(v) => save({ mailEnabled: v })} />
          </div>
          {settings.mailEnabled && (
            <div className="flex items-start justify-between gap-6 pl-[44px]">
              <div>
                <Label htmlFor="mailmem">Also make memories of mail</Label>
                <p className="text-xs text-muted-foreground mt-0.5">Off: mail is only indexed. On: Rabbit extracts facts, tasks and people from each message, which takes time on a busy inbox.</p>
              </div>
              <Switch id="mailmem" checked={settings.mailMemories} onCheckedChange={(v) => save({ mailMemories: v })} />
            </div>
          )}
        </div>
      </Panel>

      <Panel icon={BookText} title="Notion" desc={notion?.connected
        ? `Connected to ${notion.workspace || 'your workspace'} · ${notion.pages} pages indexed${notion.detail ? ` · ${notion.detail}` : ''}${notion.lastSyncAt ? ` · checked ${format(notion.lastSyncAt, 'd MMM HH:mm')}` : ''}`
        : 'Pages you share with your integration are indexed so you can search and ask about them. Meeting notes can be sent back.'}>
        <div className="space-y-3">
          {notion?.connected ? (
            <>
              <div className="flex gap-2">
                <Button size="sm" variant="outline" onClick={() => api.syncNotionNow().then(() => toast.success('Checking Notion for changes')).catch((e) => toast.error(String(e)))}>Sync now</Button>
                <Button size="sm" variant="ghost" onClick={() => api.disconnectNotion().then(() => { toast.success('Notion disconnected; its pages were removed from the index'); refreshNotion() }).catch((e) => toast.error(String(e)))}>Disconnect</Button>
              </div>
              <div className="space-y-1.5 max-w-lg">
                <Label htmlFor="nparent">Page to write meeting notes under</Label>
                <Input id="nparent" defaultValue={settings.notionParentPage} placeholder="paste the page's link" onBlur={(e) => e.target.value !== settings.notionParentPage && save({ notionParentPage: e.target.value })} />
                <p className="text-xs text-muted-foreground">Share that page with the integration too. "Send to Notion" on a meeting creates a sub-page there.</p>
              </div>
            </>
          ) : (
            <>
              <div className="flex gap-2 max-w-lg items-end">
                <div className="flex-1 space-y-1.5">
                  <Label htmlFor="ntoken">Internal integration secret</Label>
                  <Input id="ntoken" type="password" value={notionToken} onChange={(e) => setNotionToken(e.target.value)} placeholder="ntn_… or secret_…" />
                </div>
                <Button size="sm" disabled={!notionToken.trim()} onClick={() => api.setNotionToken(notionToken).then((ws) => { toast.success(`Connected to ${ws}. Importing pages…`); setNotionToken(''); refreshNotion() }).catch((e) => toast.error(String(e)))}>Connect</Button>
              </div>
              <p className="text-xs text-muted-foreground">Create one at notion.so/my-integrations (Internal, read content + insert content), then on each page you want Lane to see choose ··· → Connections → your integration. The secret is kept in your Keychain.</p>
            </>
          )}
        </div>
      </Panel>

      <Panel icon={Database} title="Connectors" desc={'Anything with an API, a script, or an MCP server: your website CRM, analytics, a database, a feed. One JSON file per source; Lane pulls it on a schedule with your key, indexes it, and with "memories": true makes memories of what changed.'}>
        <div className="space-y-3">
          <Button size="sm" variant="outline" onClick={() => api.openConnectorsFolder().then(() => toast.success('Copy an example, drop the .txt, edit, save')).catch((e) => toast.error(String(e)))}>Open the connectors folder</Button>
          {connectors.length > 0 && (
            <ul className="space-y-1">
              {connectors.map((c) => (
                <li key={c.id} className="text-sm rounded-md border px-3 py-2">
                  <div className="flex items-center gap-2">
                    <span className="font-medium truncate">{c.name}</span>
                    <span className="text-xs text-muted-foreground">{c.kind}{c.kind !== 'invalid' && ` · every ${c.everyMinutes} min${c.memories ? ' · memories' : ' · index only'}`}</span>
                    <span className="ml-auto text-xs text-muted-foreground tabular-nums shrink-0">{c.lastRun ? `${c.items} items · ${format(c.lastRun, 'd MMM HH:mm')}` : 'not run yet'}</span>
                    {c.kind !== 'invalid' && (
                      <>
                        <Button size="sm" variant="ghost" onClick={() => { api.runConnector(c.id); toast.success(`Running ${c.name}`); setTimeout(refreshConnectors, 3000) }}>Run now</Button>
                        <Button size="sm" variant="ghost" onClick={() => { setSecretFor(secretFor === c.id ? null : c.id); setSecret('') }}>Secret</Button>
                      </>
                    )}
                  </div>
                  {c.error && <p className="text-xs text-destructive mt-1">{c.error}</p>}
                  {secretFor === c.id && (
                    <div className="flex gap-2 mt-2 max-w-md">
                      <Input type="password" value={secret} placeholder={`value for {{secret}} in ${c.id}.json`} onChange={(e) => setSecret(e.target.value)} />
                      <Button size="sm" onClick={() => api.setConnectorSecret(c.id, secret).then(() => { toast.success('Kept in your Keychain'); setSecretFor(null) }).catch((e) => toast.error(String(e)))}>Save</Button>
                    </div>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>
      </Panel>

      <Panel icon={Boxes} title="Lane inside other tools" desc="Claude Desktop, Cursor and any MCP client can search your memories, facts, tasks and documents through Lane. Read-only, on this Mac, with your Keychain.">
        <div className="space-y-2">
          <p className="text-xs text-muted-foreground">Paste this into the client's MCP settings{mcp ? ` (Claude Desktop: ${mcp.claudeDesktopFile})` : ''}:</p>
          {mcp && (
            <div className="flex gap-2 items-start">
              <pre className="text-[11px] bg-secondary/70 rounded-xl p-3 overflow-x-auto flex-1 font-mono leading-relaxed">{mcp.config}</pre>
              <Button size="sm" variant="outline" onClick={() => api.copyText(mcp.config).then(() => toast.success('Copied'))}>Copy</Button>
            </div>
          )}
          {mcp && !mcp.present && <p className="text-xs text-destructive">The MCP helper is missing from this build.</p>}
        </div>
      </Panel>

      <Panel icon={FileText} title="Obsidian and other Markdown apps" desc="Set the Markdown folder under Settings → Meetings to your vault: briefings, meeting notes and captures appear there as plain files. Add the vault to the indexed folders and your notes become searchable and askable too." />
      </div>
    </div>
  )
}
