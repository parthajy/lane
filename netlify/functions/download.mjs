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
  // The build lives on the repository's releases page: free, fast, and
  // always the newest one without editing anything here.
  const to = process.env.LANE_DMG_URL || 'https://github.com/parthajy/lane/releases/latest/download/Lane.dmg'
  return new Response(null, { status: 302, headers: { location: to, 'cache-control': 'no-store' } })
}
