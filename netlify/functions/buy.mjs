/**
 * Sends someone to checkout for the plan they picked.
 *
 * Polar creates the checkout from the server, so the product ids can live in
 * the open while the token that creates it does not. The buyer comes back to
 * /thanks with the checkout id, and claim.mjs verifies it before handing over
 * a licence key.
 */

const PRODUCTS = () => ({
  monthly: process.env.POLAR_PRODUCT_MONTHLY || 'f4626775-4828-4575-af13-f1ca156e3464',
  yearly: process.env.POLAR_PRODUCT_YEARLY || 'd1c24c98-bb29-4f53-9ab0-101f1f7b5e5f',
  lifetime: process.env.POLAR_PRODUCT_LIFETIME || 'eb318663-fdf2-40c4-9dac-14ba07be52cb',
})

export const polarBase = () =>
  process.env.POLAR_MODE === 'sandbox' ? 'https://sandbox-api.polar.sh' : 'https://api.polar.sh'

export default async (request) => {
  const url = new URL(request.url)
  const plan = (url.pathname.split('/').filter(Boolean).pop() || '').toLowerCase()
  const product = PRODUCTS()[plan]
  if (!product) return new Response('No such plan.', { status: 404 })

  const site = process.env.URL || 'https://lane.so'
  const token = process.env.POLAR_ACCESS_TOKEN
  if (!token) {
    console.error('POLAR_ACCESS_TOKEN is not set')
    return Response.redirect(`${site}/#access`, 302)
  }

  const res = await fetch(`${polarBase()}/v1/checkouts/`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
    body: JSON.stringify({
      products: [product],
      success_url: `${site}/thanks/?checkout_id={CHECKOUT_ID}`,
    }),
  })
  if (!res.ok) {
    console.error('polar checkout', res.status, (await res.text()).slice(0, 600))
    return Response.redirect(`${site}/#access`, 302)
  }
  const checkout = await res.json()
  if (!checkout?.url) {
    console.error('polar checkout had no url', JSON.stringify(checkout).slice(0, 600))
    return Response.redirect(`${site}/#access`, 302)
  }
  return new Response(null, { status: 302, headers: { location: checkout.url, 'cache-control': 'no-store' } })
}
