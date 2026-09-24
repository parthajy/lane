import { checkout, ready } from './_pay.mjs'

/** Sends someone to checkout, trying each configured gateway in turn. */
export default async (request) => {
  const url = new URL(request.url)
  const plan = (url.pathname.split('/').filter(Boolean).pop() || '').toLowerCase()
  if (!['monthly', 'yearly', 'lifetime'].includes(plan)) {
    return new Response('No such plan.', { status: 404 })
  }

  const site = process.env.URL || 'https://lane.so'
  const gateways = ready()
  if (!gateways.length) {
    console.error('no payment gateway is configured')
    return new Response(null, { status: 302, headers: { location: `${site}/#access` } })
  }

  for (const gw of gateways) {
    try {
      const to = await checkout(gw, plan, site)
      if (to) return new Response(null, { status: 302, headers: { location: to, 'cache-control': 'no-store' } })
      console.error(`${gw} could not make a checkout for ${plan}`)
    } catch (e) {
      console.error(`${gw} checkout threw:`, e.message)
    }
  }
  // Every gateway refused: better the waitlist than a broken page.
  return new Response(null, { status: 302, headers: { location: `${site}/#access` } })
}
