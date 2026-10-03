import { createHash } from 'node:crypto'
import { db } from './_lib.mjs'
import FEEDS from './_feed.json' with { type: 'json' }

/**
 * Serves the update manifest, and counts the install that asked for it.
 *
 * This is the one number Lane can honestly have. Every running copy already
 * fetches this file to see whether there is a newer version, the security
 * page has always said so, and nothing new leaves anybody's Mac because of
 * this function: it counts what was already arriving.
 *
 * What is stored is a day, a platform, and a fingerprint that is the hash of
 * the caller's address and browser string together with the date and a
 * secret. It is salted with the date on purpose, so the same Mac hashes
 * differently tomorrow. That buys the question "how many installs checked in
 * today" and deliberately gives up "is this the same one as last week" —
 * which is the difference between counting people and following them.
 *
 * The count is best effort in the strongest sense: an update must never fail
 * because the counting did, so every path here ends in the manifest.
 */
const DAY = () => new Date().toISOString().slice(0, 10)

function fingerprint(request) {
  const ip =
    request.headers.get('x-nf-client-connection-ip') ||
    (request.headers.get('x-forwarded-for') || '').split(',')[0].trim() ||
    ''
  const ua = request.headers.get('user-agent') || ''
  // A guessable salt would let anyone who got hold of the table walk the
  // whole IPv4 space against a day's hashes and turn it back into addresses.
  // The service key is already secret and already in this function's
  // environment, so it does the job without a new variable to go missing.
  const salt = process.env.LANE_CHECK_SALT || process.env.SUPABASE_SERVICE_KEY || 'lane'
  return createHash('sha256').update(`${DAY()}|${salt}|${ip}|${ua}`).digest('hex').slice(0, 32)
}

const json = (body) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json', 'cache-control': 'no-store' },
  })

export default async (request) => {
  const name = new URL(request.url).pathname.split('/').pop().replace(/\.json$/, '')
  const feed = FEEDS[name]

  // No manifest for this platform means no update, which is a 204 to Tauri
  // and not an error. Counting a caller we cannot serve is still worth doing.
  const answer = feed ? json(feed) : new Response(null, { status: 204 })

  try {
    await db('rpc/note_update_check', {
      method: 'POST',
      body: JSON.stringify({ p_target: name, p_fingerprint: fingerprint(request) }),
    })
  } catch (e) {
    console.error('update check count:', e.message)
  }

  return answer
}
