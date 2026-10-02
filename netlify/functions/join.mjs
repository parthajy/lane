import { db, json } from './_lib.mjs'

/** How many people can have Lane outright. Matches COMPS in admin.mjs. */
const SEATS = 200

const isEmail = (e) => /^[^@\s]+@[^@\s.]+\.[^@\s]{2,}$/.test(e)

/** Seats already given: keys actually handed out, not people who asked. */
async function claimed() {
  const res = await db('licence_keys?select=id&plan=eq.comp&claimed_at=not.is.null', {
    headers: { Prefer: 'count=exact', Range: '0-0' },
  }).catch(() => null)
  return res?.length ?? 0
}

/**
 * Put your email in, and you are in the queue for one of the free seats.
 *
 * GET returns the count, so the page can say how many are left without
 * anybody signing up to find out. POST joins. Asking twice is not an error
 * and does not take a second seat: the same address is the same person.
 */
export default async (request) => {
  const url = new URL(request.url)

  if (request.method === 'GET') {
    // Counted properly: PostgREST gives the total in content-range.
    const r = await fetch(
      `${process.env.SUPABASE_URL || 'https://fuqrvmprgzqjmfqszxoe.supabase.co'}/rest/v1/licence_keys?select=id&plan=eq.comp&claimed_at=not.is.null`,
      { headers: { apikey: process.env.SUPABASE_SERVICE_KEY || '', Authorization: `Bearer ${process.env.SUPABASE_SERVICE_KEY || ''}`, Prefer: 'count=exact', Range: '0-0' } },
    ).catch(() => null)
    const range = r?.headers?.get('content-range') || ''
    const given = Number(range.split('/')[1]) || 0
    return json({ claimed: given, seats: SEATS, left: Math.max(0, SEATS - given) })
  }

  if (request.method !== 'POST') return json({ ok: false, error: 'Use POST.' }, 405)

  let email = ''
  let name = ''
  const type = request.headers.get('content-type') || ''
  if (type.includes('application/json')) {
    const body = await request.json().catch(() => ({}))
    email = String(body.email || '')
    name = String(body.name || '')
  } else {
    const form = await request.formData().catch(() => null)
    email = String(form?.get('email') || '')
    name = String(form?.get('name') || '')
  }
  email = email.trim().toLowerCase()
  if (!isEmail(email)) return json({ ok: false, error: 'That does not look like an email address.' }, 400)

  const already = await db(`waitlist?select=id&email=eq.${encodeURIComponent(email)}&limit=1`).catch(() => null)
  if (!already?.length) {
    await db('waitlist', {
      method: 'POST',
      headers: { Prefer: 'return=minimal' },
      body: JSON.stringify({
        email,
        name: name.slice(0, 80),
        source: (url.searchParams.get('source') || 'site').slice(0, 40),
        lifetime: true,
      }),
    })
  }
  const given = await claimed()
  return json({ ok: true, again: Boolean(already?.length), claimed: given, seats: SEATS })
}
