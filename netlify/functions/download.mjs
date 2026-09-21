import { db } from './_lib.mjs'

const REPO = process.env.LANE_REPO || 'parthajy/lane'
const ASSET = 'Lane.dmg'

/**
 * Counts the click, then hands over the newest build.
 *
 * The build lives on the repository's releases page, so publishing a release
 * is all it takes for this to start serving it: nothing here has to change
 * and nothing has to be redeployed. If there is no release yet, people land
 * on the waitlist rather than a 404.
 */
async function newestBuild() {
  if (process.env.LANE_DMG_URL) return process.env.LANE_DMG_URL
  try {
    const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
      headers: { accept: 'application/vnd.github+json', 'user-agent': 'lane.so' },
    })
    if (!res.ok) return ''
    const release = await res.json()
    const asset = (release.assets || []).find((a) => a.name === ASSET)
    return asset?.browser_download_url || ''
  } catch (e) {
    console.error('release lookup:', e.message)
    return ''
  }
}

export default async (request) => {
  const url = new URL(request.url)
  const source = (url.searchParams.get('source') || '').slice(0, 40)
  const to = await newestBuild()

  try {
    await db('downloads', { method: 'POST', body: JSON.stringify({ source: to ? source : `${source || 'direct'}:no-build` }) })
  } catch (e) {
    // A download must never fail because the counter did.
    console.error('download count:', e.message)
  }

  return new Response(null, {
    status: 302,
    headers: { location: to || `${url.origin}/#access`, 'cache-control': 'no-store' },
  })
}
