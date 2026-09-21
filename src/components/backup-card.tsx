import { useEffect, useState } from 'react'
import { format } from 'date-fns'
import { Lock, ShieldCheck } from 'lucide-react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { api, type BackupStatus } from '@/lib/api'

export function BackupCard({ folder, onFolderChange, onSaveFolder }: { folder: string; onFolderChange: (v: string) => void; onSaveFolder: () => void }) {
  const [st, setSt] = useState<BackupStatus | null>(null)
  const [pass, setPass] = useState('')
  const [pass2, setPass2] = useState('')
  const [busy, setBusy] = useState(false)

  const refresh = () => api.backupStatus().then(setSt).catch(() => {})
  useEffect(() => {
    refresh()
    const t = setInterval(refresh, 5000)
    return () => clearInterval(t)
  }, [])

  async function enable() {
    if (pass !== pass2) return toast.error('The two passphrases differ.')
    setBusy(true)
    try {
      const p = await api.setBackupPassphrase(pass)
      toast.success(`Backups on. First backup written to ${p.split('/').pop()}`)
      setPass('')
      setPass2('')
      refresh()
    } catch (e) {
      toast.error(String(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base flex items-center gap-2">
          <Lock className="h-4 w-4" /> Encryption and backup
        </CardTitle>
        <CardDescription>
          Your memory is encrypted on this Mac with a key kept in your login Keychain. Backups add a second layer: one file, sealed with a
          passphrase only you know, in a folder you choose. Put that folder in iCloud Drive or Google Drive and a new Mac can restore
          everything; the cloud only ever sees a sealed file.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <p className="text-sm flex items-center gap-2">
          <ShieldCheck className="h-4 w-4 text-primary" /> Database encrypted at rest.
        </p>
        {st?.enabled ? (
          <div className="space-y-2 text-sm">
            <p>
              Backups <span className="text-primary">on</span> · daily · last {st.lastBackupAt ? format(st.lastBackupAt, 'EEE d MMM, HH:mm') : 'never'}
              {st.detail && ` · ${st.detail}`}
            </p>
            <p className="text-xs text-muted-foreground break-all">{st.folder}</p>
            <div className="flex gap-2">
              <Button size="sm" variant="outline" onClick={() => api.backupNow().then((p) => toast.success(`Backup written: ${p.split('/').pop()}`)).catch((e) => toast.error(String(e)))}>
                Back up now
              </Button>
              <Button size="sm" variant="ghost" onClick={() => api.disableBackups().then(refresh)}>
                Turn off
              </Button>
            </div>
          </div>
        ) : (
          <div className="space-y-2">
            <Label>Set a recovery passphrase to turn backups on</Label>
            <div className="grid grid-cols-2 gap-2 max-w-md">
              <Input type="password" placeholder="Passphrase (8+ characters)" value={pass} onChange={(e) => setPass(e.target.value)} />
              <Input type="password" placeholder="Repeat" value={pass2} onChange={(e) => setPass2(e.target.value)} />
            </div>
            <p className="text-xs text-muted-foreground">
              Write it down. Nobody, including us, can open a backup without it.
            </p>
            <Button size="sm" onClick={enable} disabled={busy || pass.length < 8}>
              Turn on backups
            </Button>
          </div>
        )}
        <div className="space-y-1.5 max-w-md">
          <Label htmlFor="bfolder">Backup folder (empty = {st?.folder.replace(/^\/Users\/[^/]+/, '~')})</Label>
          <div className="flex gap-2">
            <Input id="bfolder" value={folder} onChange={(e) => onFolderChange(e.target.value)} placeholder="~/Library/Mobile Documents/com~apple~CloudDocs/Lane" />
            <Button size="sm" variant="outline" onClick={onSaveFolder}>Save</Button>
          </div>
        </div>
        <RestoreRow />
      </CardContent>
    </Card>
  )
}

export function RestoreRow({ compact }: { compact?: boolean }) {
  const [file, setFile] = useState<string | null>(null)
  const [pass, setPass] = useState('')
  return (
    <div className={compact ? 'space-y-2' : 'border-t pt-3 space-y-2'}>
      <Label>Restore from a backup{compact ? '' : ' (replaces everything on this Mac, then restarts)'}</Label>
      <div className="flex flex-wrap gap-2 items-center">
        <Button size="sm" variant="outline" onClick={() => api.chooseBackupFile().then(setFile)}>
          {file ? file.split('/').pop() : 'Choose .rvault file'}
        </Button>
        <Input type="password" placeholder="Passphrase" value={pass} onChange={(e) => setPass(e.target.value)} className="max-w-[200px]" />
        <Button size="sm" variant="destructive" disabled={!file || !pass} onClick={() => file && api.restoreBackup(file, pass).catch((e) => toast.error(String(e)))}>
          Restore
        </Button>
      </div>
    </div>
  )
}
