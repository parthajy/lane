import { adminOk, count, db, escape, html, tokenFrom, tokenIsGood } from './_lib.mjs'

/** Sign in once. The token is kept in a cookie the browser will not hand to
    any script, so the dashboard can be a bookmark rather than a secret URL. */
const SIGNIN = (bad) => `<!doctype html><meta charset=utf-8><title>Lane · admin</title>
<meta name=viewport content="width=device-width,initial-scale=1"><meta name=robots content=noindex>
<style>
*{box-sizing:border-box}
body{margin:0;min-height:100vh;display:grid;place-items:center;background:#f4f4f7;color:#101014;
     font:15px/1.5 -apple-system,Inter,system-ui,sans-serif;padding:24px}
form{background:#fff;padding:30px;border-radius:18px;box-shadow:0 1px 2px rgba(16,16,26,.06),0 20px 50px -30px rgba(16,16,26,.4);width:min(360px,100%);display:grid;gap:12px}
h1{font-size:19px;margin:0;letter-spacing:-.02em}
p{margin:0;color:#6b6b7a;font-size:13.5px}
input{font:inherit;padding:11px 13px;border-radius:10px;border:1px solid rgba(16,16,26,.16);width:100%}
input:focus{outline:0;border-color:#5a51e5;box-shadow:0 0 0 4px rgba(90,81,229,.14)}
button{font:inherit;font-weight:500;padding:11px;border:0;border-radius:10px;background:#101014;color:#fff;cursor:pointer}
.bad{color:#b03f28}
</style>
<form method="post" action="/admin">
  <h1>Lane</h1>
  <p>${bad ? '<span class="bad">That is not the token.</span>' : 'Paste the admin token once. This browser will remember it.'}</p>
  <input name="token" type="password" autocomplete="current-password" placeholder="Admin token" autofocus required>
  <button type="submit">Open the dashboard</button>
</form>`

/** How many people can be given Lane outright. */
const COMPS = 200

/** A plain look at an email address. */
const isEmail = (e) => /^[^@\s]+@[^@\s.]+\.[^@\s]{2,}$/.test(e)

/**
 * Hand one of the early-tester keys to somebody. It comes out of the comp
 * pool, which is separate from the two hundred lifetime seats that are for
 * sale, so giving Lane away never eats one of those. Asking twice for the
 * same address returns the same key rather than spending another.
 */
async function giveLifetime(email) {
  const to = String(email || '').trim().toLowerCase()
  if (!isEmail(to)) return { ok: false, error: 'That does not look like an email address.' }
  const used = await count('licence_keys', 'plan=eq.comp&claimed_at=not.is.null')
  const already = await db(`licence_keys?select=licence_key&plan=eq.comp&payment_id=eq.comp:${encodeURIComponent(to)}&limit=1`)
  if (!already?.length && used >= COMPS) {
    return { ok: false, error: `All ${COMPS} early-tester keys are given out.` }
  }
  const out = await db('rpc/claim_licence', {
    method: 'POST',
    body: JSON.stringify({ p_payment_id: `comp:${to}`, p_plan: 'comp', p_email: to, p_amount: 0, p_currency: 'USD' }),
  })
  if (!out?.ok) return { ok: false, error: out?.error || 'No key came back.' }
  // The counter on the site reads the waitlist, so somebody given a seat by
  // hand — the people who were testing before the site asked for addresses —
  // has to appear there too, or the count reads low for ever.
  await db('rpc/join_waitlist', {
    method: 'POST',
    body: JSON.stringify({ p_email: to, p_name: '', p_source: 'admin' }),
  }).catch(() => null)
  return { ok: true, key: out.key, email: to, again: Boolean(out.again) }
}

/** Everything the outside world tells us. The app itself reports nothing. */
export default async (request) => {
  // Signing in: keep the token in a cookie for ninety days.
  if (request.method === 'POST') {
    const form = await request.formData()

    // Already signed in, and this is the give-a-key form.
    if (adminOk(request) && form.get('give')) {
      const got = await giveLifetime(form.get('give'))
      const q = got.ok
        ? `?gave=${encodeURIComponent(got.email)}&key=${encodeURIComponent(got.key)}${got.again ? '&again=1' : ''}`
        : `?trouble=${encodeURIComponent(got.error)}`
      return new Response(null, { status: 303, headers: { location: `/admin${q}` } })
    }

    const given = String(form.get('token') || '')
    if (!tokenIsGood(given)) return html(SIGNIN(true), 401)
    return new Response(null, {
      status: 303,
      headers: {
        location: '/admin',
        'set-cookie': `lane_admin=${encodeURIComponent(given)}; Path=/; Max-Age=7776000; HttpOnly; Secure; SameSite=Lax`,
      },
    })
  }

  if (!adminOk(request)) {
    // A token that is present but wrong should say so; a missing one just asks.
    return html(SIGNIN(Boolean(tokenFrom(request))), 401)
  }

  // Signing out: drop the cookie and ask again.
  if (new URL(request.url).searchParams.get('out')) {
    return new Response(SIGNIN(false), {
      status: 401,
      headers: { 'content-type': 'text/html; charset=utf-8', 'set-cookie': 'lane_admin=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax' },
    })
  }

  const since = new Date(Date.now() - 21 * 86400_000).toISOString()
  const [downloads, waitTotal, lifetimeTaken, keysLeft, sales, recentWait, recentFeedback, recentDownloads, compsGiven, compRows] =
    await Promise.all([
      count('downloads'),
      count('waitlist'),
      count('waitlist', 'lifetime=is.true'),
      db('licence_keys?select=plan&claimed_at=is.null'),
      db('sales?select=plan,amount_cents,currency,email,created_at&order=id.desc&limit=200'),
      db('waitlist?select=email,name,lifetime,created_at&order=id.desc&limit=60'),
      db('feedback?select=kind,body,email,created_at&order=id.desc&limit=25'),
      db(`downloads?select=created_at&created_at=gte.${since}&order=id.desc&limit=5000`),
      count('licence_keys', 'plan=eq.comp&claimed_at=not.is.null'),
      db('licence_keys?select=claimed_by,claimed_at&plan=eq.comp&claimed_at=not.is.null&order=claimed_at.desc&limit=25'),
    ])

  const seats = Math.max(0, 200 - lifetimeTaken)
  const money = (sales || []).reduce((a, s) => a + (s.amount_cents || 0), 0) / 100
  const byPlan = {}
  for (const s of sales || []) byPlan[s.plan || '—'] = (byPlan[s.plan || '—'] || 0) + 1
  const free = {}
  for (const k of keysLeft || []) free[k.plan] = (free[k.plan] || 0) + 1

  const days = {}
  for (const d of recentDownloads || []) {
    const day = String(d.created_at).slice(0, 10)
    days[day] = (days[day] || 0) + 1
  }
  const dayRows = Object.entries(days).sort((a, b) => (a[0] < b[0] ? 1 : -1)).slice(0, 21)
  const max = Math.max(1, ...dayRows.map(([, n]) => n))
  const when = (t) => new Date(t).toLocaleString('en-GB', { day: '2-digit', month: 'short', hour: '2-digit', minute: '2-digit' })

  const bars = dayRows
    .map(([day, n]) => `<li><span class="d">${day.slice(5)}</span><span class="bar"><i style="width:${Math.round((n / max) * 100)}%"></i></span><span class="n">${n}</span></li>`)
    .join('')
  const plans = Object.entries(byPlan).map(([p, n]) => `<li><span>${escape(p)}</span><b>${n}</b></li>`).join('')
  const pool = ['lifetime', 'yearly', 'monthly']
    .map((p) => `<li><span>${p}</span><b>${free[p] || 0}</b><span class="n">keys unclaimed</span></li>`)
    .join('')
  // Everyone who asked, with the button that answers them. The address is
  // put straight into the give form, so issuing a key is one press rather
  // than copying an address from one table into a box above it.
  const issued = new Set((compRows || []).map((r) => (r.claimed_by || '').toLowerCase()))
  const waitRows = (recentWait || [])
    .map((r) => {
      const done = issued.has((r.email || '').toLowerCase())
      const action = done
        ? '<span class="sent">sent</span>'
        : `<form method="post" action="/admin" class="inline"><input type="hidden" name="give" value="${escape(r.email)}"><button type="submit">Give lifetime</button></form>`
      return `<tr><td>${escape(r.email)}</td><td>${escape(r.name)}</td><td class="n">${when(r.created_at)}</td><td class="act">${action}</td></tr>`
    })
    .join('')
  const fbRows = (recentFeedback || [])
    .map((r) => `<tr><td>${escape(r.kind)}</td><td>${escape(String(r.body).slice(0, 300))}</td><td>${escape(r.email)}</td><td class="n">${when(r.created_at)}</td></tr>`)
    .join('')
  const saleRows = (sales || [])
    .slice(0, 25)
    .map((r) => `<tr><td>${escape(r.email)}</td><td>${escape(r.plan)}</td><td>${((r.amount_cents || 0) / 100).toFixed(2)} ${escape(r.currency || '')}</td><td class="n">${when(r.created_at)}</td></tr>`)
    .join('')

  const ask = new URL(request.url).searchParams
  const gave = ask.get('gave')
  const gaveKey = ask.get('key')
  const trouble = ask.get('trouble')
  const compRowsHtml = (compRows || [])
    .map((r) => `<tr><td>${escape(r.claimed_by || '')}</td><td class="n">${when(r.claimed_at)}</td></tr>`)
    .join('')
  const note = trouble
    ? `<p class="bad">${escape(trouble)}</p>`
    : gave
      ? `<div class="gave"><p>${ask.get('again') ? 'Already had one' : 'Given'} — <b>${escape(gave)}</b>. Send them this key:</p>
         <textarea readonly rows="3" onclick="this.select()">${escape(gaveKey || '')}</textarea>
         <p class="n">Click the key to select it. It is a lifetime key; they paste it into Settings → Your licence.</p></div>`
      : ''

  return html(`<!doctype html><meta charset=utf-8><title>Lane · admin</title>
<meta name=viewport content="width=device-width,initial-scale=1">
<style>
:root{--ink:#101014;--muted:#6b6b7a;--line:rgba(16,16,26,.1);--accent:#5a51e5}
*{box-sizing:border-box}
body{margin:0;background:#f4f4f7;color:var(--ink);font:15px/1.5 -apple-system,Inter,system-ui,sans-serif;padding:28px}
h1{font-size:26px;letter-spacing:-.02em;margin:0 0 4px}
p.sub{color:var(--muted);margin:0 0 24px}
.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(160px,1fr));gap:12px;margin-bottom:20px}
.card{background:#fff;border-radius:16px;padding:16px 18px;box-shadow:0 1px 2px rgba(16,16,26,.05)}
.card b{display:block;font-size:28px;letter-spacing:-.02em}
.card span{color:var(--muted);font-size:13px}
section{background:#fff;border-radius:16px;padding:18px;margin-bottom:16px;box-shadow:0 1px 2px rgba(16,16,26,.05)}
h2{font-size:13px;text-transform:uppercase;letter-spacing:.12em;color:var(--muted);margin:0 0 12px}
ul{list-style:none;margin:0;padding:0}
li{display:flex;align-items:center;gap:10px;padding:4px 0;font-size:13px}
li b{min-width:28px;text-align:right}
.d{width:52px;color:var(--muted)}
.bar{flex:1;height:8px;border-radius:4px;background:rgba(16,16,26,.07);overflow:hidden}
.bar i{display:block;height:100%;background:var(--accent)}
.n{color:var(--muted);font-variant-numeric:tabular-nums}
table{width:100%;border-collapse:collapse;font-size:13px;table-layout:fixed}
td{padding:7px 8px;border-top:1px solid var(--line);vertical-align:top;overflow-wrap:anywhere}
td:last-child{width:104px;white-space:nowrap;text-align:right}
td:first-child{width:34%}
section.fb td:first-child{width:74px;color:var(--muted)}
form.give{display:flex;gap:8px;margin-bottom:12px}
form.give input{font:inherit;padding:9px 12px;border-radius:10px;border:1px solid var(--line);flex:1;min-width:0}
form.give input:focus{outline:0;border-color:var(--accent);box-shadow:0 0 0 4px rgba(90,81,229,.14)}
form.give button{font:inherit;font-weight:500;padding:9px 16px;border:0;border-radius:10px;background:var(--ink);color:#fff;cursor:pointer;white-space:nowrap}
form.inline{margin:0}
form.inline button{font:inherit;font-size:12px;font-weight:500;padding:5px 11px;border:0;border-radius:8px;background:var(--accent);color:#fff;cursor:pointer;white-space:nowrap}
td.act{width:112px;text-align:right}
.sent{color:#1a7f4b;font-size:12px}
.gave{background:#f2fbf5;border:1px solid rgba(22,140,80,.22);border-radius:12px;padding:12px;margin-bottom:12px}
.gave p{margin:0 0 8px;font-size:13.5px}
.gave textarea{width:100%;font:12px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace;padding:9px;border-radius:9px;border:1px solid var(--line);resize:vertical;word-break:break-all}
.bad{color:#b03f28;font-size:13.5px;margin:0 0 12px}
</style>
<h1>Lane</h1><p class="sub">Everything the outside world tells us. The app itself reports nothing. <a href="/admin?out=1" style="color:var(--muted)">Sign out</a></p>
<div class="cards">
  <div class="card"><b>${downloads}</b><span>downloads</span></div>
  <div class="card"><b>${waitTotal}</b><span>on the waitlist</span></div>
  <div class="card"><b>${seats}</b><span>lifetime seats left</span></div>
  <div class="card"><b>${(sales || []).length}</b><span>sales</span></div>
  <div class="card"><b>$${money.toFixed(0)}</b><span>collected</span></div>
  <div class="card"><b>${COMPS - compsGiven}</b><span>free seats left</span></div>
</div>
<section>
  <h2>Give Lane away · ${compsGiven} of ${COMPS} used</h2>
  <form method="post" action="/admin" class="give">
    <input name="give" type="email" placeholder="early tester's email" required>
    <button type="submit">Give lifetime</button>
  </form>
  ${note}
  <table>${compRowsHtml || '<tr><td>nobody yet</td></tr>'}</table>
</section>
<section><h2>Downloads by day</h2><ul>${bars || '<li>nothing yet</li>'}</ul></section>
<section><h2>Sales by plan</h2><ul>${plans || '<li>none yet</li>'}</ul></section>
<section><h2>Keys in the pool</h2><ul>${pool}</ul></section>
<section><h2>Newest sales</h2><table>${saleRows || '<tr><td>none yet</td></tr>'}</table></section>
<section><h2>Asked for a free seat · ${compsGiven} of ${COMPS} sent</h2><table>${waitRows || '<tr><td>nobody yet</td></tr>'}</table></section>
<section class="fb"><h2>Feedback</h2><table>${fbRows || '<tr><td>nothing yet</td></tr>'}</table></section>
`)
}
