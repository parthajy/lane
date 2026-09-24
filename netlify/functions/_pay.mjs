/**
 * Two payment gateways, one behind the other.
 *
 * Dodo is the one people are sent to. If creating a checkout with it fails
 * for any reason, the buyer goes to Polar instead rather than seeing an
 * error, and the receipt records which one took the money so the licence can
 * be verified against the right place.
 *
 * Product ids are public: they appear in every checkout link. The API keys
 * are the secrets, and they only ever live in Netlify.
 */

export const GATEWAYS = ['dodo', 'polar']

const DODO_PRODUCTS = () => ({
  monthly: process.env.DODO_PRODUCT_MONTHLY || '',
  yearly: process.env.DODO_PRODUCT_YEARLY || '',
  lifetime: process.env.DODO_PRODUCT_LIFETIME || '',
})

const POLAR_PRODUCTS = () => ({
  monthly: process.env.POLAR_PRODUCT_MONTHLY || 'f4626775-4828-4575-af13-f1ca156e3464',
  yearly: process.env.POLAR_PRODUCT_YEARLY || 'd1c24c98-bb29-4f53-9ab0-101f1f7b5e5f',
  lifetime: process.env.POLAR_PRODUCT_LIFETIME || 'eb318663-fdf2-40c4-9dac-14ba07be52cb',
})

const dodoBase = () =>
  process.env.DODO_MODE === 'test' ? 'https://test.dodopayments.com' : 'https://live.dodopayments.com'
const dodoCheckout = () =>
  process.env.DODO_MODE === 'test' ? 'https://test.checkout.dodopayments.com' : 'https://checkout.dodopayments.com'
const polarBase = () =>
  process.env.POLAR_MODE === 'sandbox' ? 'https://sandbox-api.polar.sh' : 'https://api.polar.sh'

/** Which gateways are actually configured, in the order they are tried. */
export function ready() {
  const order = (process.env.PAY_ORDER || 'dodo,polar').split(',').map((s) => s.trim())
  return order.filter((g) =>
    (g === 'dodo' && process.env.DODO_API_KEY && DODO_PRODUCTS().lifetime) ||
    (g === 'polar' && process.env.POLAR_ACCESS_TOKEN))
}

/** A checkout URL for one plan, or null if this gateway cannot make one. */
export async function checkout(gateway, plan, site) {
  const back = `${site}/thanks/`
  if (gateway === 'dodo') {
    const product = DODO_PRODUCTS()[plan]
    if (!product) return null
    // Dodo's hosted link needs no API call, so there is nothing to fail.
    const u = new URL(`${dodoCheckout()}/buy/${product}`)
    u.searchParams.set('quantity', '1')
    u.searchParams.set('redirect_url', `${back}?gw=dodo`)
    return u.toString()
  }

  if (gateway === 'polar') {
    const product = POLAR_PRODUCTS()[plan]
    const token = process.env.POLAR_ACCESS_TOKEN
    if (!product || !token) return null
    const res = await fetch(`${polarBase()}/v1/checkouts/`, {
      method: 'POST',
      headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
      body: JSON.stringify({ products: [product], success_url: `${back}?gw=polar&checkout_id={CHECKOUT_ID}` }),
    })
    if (!res.ok) {
      console.error('polar checkout', res.status, (await res.text()).slice(0, 400))
      return null
    }
    const c = await res.json()
    return c?.url ?? null
  }
  return null
}

const PAID = new Set(['succeeded', 'confirmed', 'paid', 'completed', 'active', 'processing_succeeded'])

/** Ask a gateway what really happened, so a made-up id gets nothing. */
export async function verify(gateway, id) {
  if (gateway === 'dodo') {
    const key = process.env.DODO_API_KEY
    if (!key) return { ok: false, error: 'Payments are not configured yet.' }
    const res = await fetch(`${dodoBase()}/payments/${id}`, { headers: { Authorization: `Bearer ${key}` } })
    if (!res.ok) {
      console.error('dodo lookup', res.status, (await res.text()).slice(0, 400))
      return { ok: false, error: 'We could not find that payment.' }
    }
    const p = await res.json()
    console.log('dodo payment', JSON.stringify(p).slice(0, 1500))
    const cart = Array.isArray(p.product_cart) ? p.product_cart : []
    return shape({
      status: p.status ?? p.payment_status,
      email: p.customer?.email ?? p.customer_email ?? '',
      product: p.product_id ?? cart[0]?.product_id ?? p.subscription?.product_id ?? '',
      amount: p.total_amount ?? p.amount ?? 0,
      currency: p.currency ?? p.settlement_currency ?? 'USD',
    }, DODO_PRODUCTS())
  }

  if (gateway === 'polar') {
    const token = process.env.POLAR_ACCESS_TOKEN
    if (!token) return { ok: false, error: 'Payments are not configured yet.' }
    const res = await fetch(`${polarBase()}/v1/checkouts/${id}`, { headers: { Authorization: `Bearer ${token}` } })
    if (!res.ok) {
      console.error('polar lookup', res.status, (await res.text()).slice(0, 400))
      return { ok: false, error: 'We could not find that payment.' }
    }
    const c = await res.json()
    console.log('polar checkout', JSON.stringify(c).slice(0, 1500))
    return shape({
      status: c.status ?? c.payment_status,
      email: c.customer_email ?? c.customer?.email ?? '',
      product: c.product_id ?? c.product?.id ?? (Array.isArray(c.products) ? (c.products[0]?.id ?? c.products[0]) : ''),
      amount: c.total_amount ?? c.amount ?? 0,
      currency: c.currency ?? 'USD',
    }, POLAR_PRODUCTS())
  }
  return { ok: false, error: 'Unknown payment gateway.' }
}

function shape(raw, products) {
  const status = String(raw.status ?? '').toLowerCase()
  if (!PAID.has(status)) return { ok: false, error: `That payment is ${status || 'not complete'}.` }
  const plan = Object.keys(products).find((k) => products[k] && products[k] === raw.product) || ''
  if (!plan) {
    console.error('unknown product', raw.product, products)
    return { ok: false, error: 'That purchase is not a Lane plan.' }
  }
  return {
    ok: true,
    plan,
    email: raw.email || '',
    amount: Number(raw.amount) || 0,
    currency: String(raw.currency || 'USD').toUpperCase(),
  }
}
