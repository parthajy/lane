/** Sends someone to the right Dodo checkout, and brings them back to /thanks. */
const PRODUCTS = () => ({
  monthly: process.env.DODO_PRODUCT_MONTHLY || '',
  yearly: process.env.DODO_PRODUCT_YEARLY || '',
  lifetime: process.env.DODO_PRODUCT_LIFETIME || '',
})

export default async (request) => {
  const url = new URL(request.url)
  const plan = (url.pathname.split('/').pop() || '').toLowerCase()
  const product = PRODUCTS()[plan]
  if (!product) return new Response('No such plan.', { status: 404 })

  const base = process.env.DODO_MODE === 'live'
    ? 'https://checkout.dodopayments.com'
    : 'https://test.checkout.dodopayments.com'
  const site = process.env.URL || 'https://lane.so'
  const to = new URL(`${base}/buy/${product}`)
  to.searchParams.set('quantity', '1')
  to.searchParams.set('redirect_url', `${site}/thanks/`)
  return new Response(null, { status: 302, headers: { location: to.toString(), 'cache-control': 'no-store' } })
}
