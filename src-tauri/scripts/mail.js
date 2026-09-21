// Apple Mail → JSON lines, through Mail's own automation interface (the
// "Lane wants to control Mail" permission), so Lane never needs Full Disk
// Access. Inbox and Sent of every account, newest first, since a time.
//
// usage: osascript -l JavaScript mail.js <since-ms>   (0 = last 30 days)
function run(argv) {
  const since = Number(argv[0] || 0)
  const cutoff = since > 0 ? new Date(since) : new Date(Date.now() - 30 * 86400000)
  const Mail = Application('Mail')
  const out = []
  const seen = {}
  const accounts = Mail.accounts()
  for (const acct of accounts) {
    let boxes = []
    try { boxes = acct.mailboxes() } catch (e) { continue }
    for (const mb of boxes) {
      let name = ''
      try { name = mb.name() } catch (e) { continue }
      if (!/^(inbox|sent|sent messages|sent mail)$/i.test(name)) continue
      let msgs = []
      try { msgs = mb.messages.whose({ dateReceived: { _greaterThan: cutoff } })() } catch (e) { continue }
      for (const m of msgs.slice(0, 400)) {
        try {
          const id = String(m.messageId() || m.id())
          if (seen[id]) continue
          seen[id] = true
          const subject = m.subject() || '(no subject)'
          const sender = m.sender() || ''
          const to = (() => { try { return m.toRecipients().map((r) => r.address()).join(', ') } catch (e) { return '' } })()
          const when = m.dateReceived()
          const content = String(m.content() || '').replace(/\r/g, '').slice(0, 6000)
          out.push(JSON.stringify({
            id: id.replace(/[^A-Za-z0-9._-]/g, '_').slice(0, 100),
            title: `${name.toLowerCase().startsWith('sent') ? 'To ' + to : 'From ' + sender}: ${subject}`,
            body: `Subject: ${subject}\nFrom: ${sender}\nTo: ${to}\nDate: ${when}\nMailbox: ${acct.name()} / ${name}\n\n${content}`,
            time: when.toISOString(),
            url: `message://%3C${encodeURIComponent(String(m.messageId() || ''))}%3E`,
          }))
        } catch (e) { /* a message that cannot be read is skipped */ }
      }
    }
  }
  return out.join('\n')
}
