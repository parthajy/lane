import { db, json } from './_lib.mjs'

/** How many people get Lane outright. The database holds the same number in
    lane_lifetime_seats(); this is the fallback when it cannot be reached. */
const SEATS = 200

/** Seats gone, by either route: claimed on the site, or given by hand from
    the admin page, which is how the first testers got theirs. The database
    counts the union of the two, distinct by address, so the number cannot
    drift from however a seat was actually handed over. A seat counts the
    moment it is taken, not when the licence is posted — otherwise the counter
    sits still for a day and the two hundredth person is told there is room
    when there is not. */
const taken = () =>
  db('rpc/lane_seats_taken', { method: 'POST', body: '{}' })
    .then((n) => Number(n) || 0)
    .catch(() => 0)

/**
 * The only door into the two hundred.
 *
 * GET says how many are gone, so the page can show the count without
 * anybody signing up to find out. POST takes a seat. Asking twice is not an
 * error and does not take a second seat: the same address is the same
 * person. Allocation happens inside join_waitlist, in one statement, so two
 * people claiming the last seat at once cannot both get it.
 */
export default async (request) => {
  const url = new URL(request.url)

  if (request.method === 'GET') {
    const gone = await taken()
    return json({ claimed: gone, seats: SEATS, left: Math.max(0, SEATS - gone) })
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
  if (!/^[^@\s]+@[^@\s.]+\.[^@\s]{2,}$/.test(email) || email.length > 160) {
    return json({ ok: false, error: 'That does not look like an email address.' }, 400)
  }

  // Was this address already here? The database will not say so itself, and
  // the page needs to tell somebody they are on the list rather than imply
  // they have just taken a second seat.
  const before = await db(`waitlist?select=id&email=eq.${encodeURIComponent(email)}&limit=1`).catch(() => null)

  let out
  try {
    out = await db('rpc/join_waitlist', {
      method: 'POST',
      body: JSON.stringify({
        p_email: email,
        p_name: name.slice(0, 80),
        p_source: (url.searchParams.get('source') || 'site').slice(0, 40),
      }),
    })
  } catch (err) {
    if (String(err).includes('email address')) {
      return json({ ok: false, error: 'That does not look like an email address.' }, 400)
    }
    return json({ ok: false, error: 'We could not reach the list. Write to pb@lane.so and we will add you by hand.' }, 502)
  }

  const seats = out?.lifetimeSeats ?? SEATS
  const left = out?.lifetimeLeft ?? Math.max(0, seats - (await taken()))
  return json({
    ok: true,
    again: Boolean(before?.length),
    lifetime: Boolean(out?.lifetime),
    claimed: Math.max(0, seats - left),
    seats,
  })
}
