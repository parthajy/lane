import { db } from './_lib.mjs'

/** Counts the click, then hands over the file. */
export default async (request) => {
  const url = new URL(request.url)
  const source = (url.searchParams.get('source') || '').slice(0, 40)
  try {
    await db('downloads', { method: 'POST', body: JSON.stringify({ source }) })
  } catch (e) {
    // A download must never fail because the counter did.
    console.error('download count:', e.message)
  }
  const to = process.env.LANE_DMG_URL || 'https://lane.so/Lane.dmg'
  return new Response(null, { status: 302, headers: { location: to, 'cache-control': 'no-store' } })
}
