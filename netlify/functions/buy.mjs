/** Sends someone to the right Dodo checkout, and brings them back to /thanks. */
// The test-mode products, which are public: they appear in every checkout
// link. Set the variables in Netlify to point at the live ones.
const TEST = {
  monthly: 'pdt_0No5etdA408odWxLdLHjI',
  yearly: 'pdt_0No5ethe62xSynYBoWPnO',
  lifetime: 'pdt_0No5eqhNCUK6AbLdVMdL3',
}
const PRODUCTS = () => ({
  monthly: process.env.DODO_PRODUCT_MONTHLY || TEST.monthly,
  yearly: process.env.DODO_PRODUCT_YEARLY || TEST.yearly,
  lifetime: process.env.DODO_PRODUCT_LIFETIME || TEST.lifetime,
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
