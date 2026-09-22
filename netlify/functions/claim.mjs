import { db, json } from './_lib.mjs'
import { polarBase } from './buy.mjs'

/**
 * After paying, the buyer lands on /thanks and this hands them their key.
 *
 * The payment is checked against Polar from here, with the secret token, so a
 * made-up checkout id gets nothing. The key itself was signed on Partha's Mac
 * and uploaded: no signing secret is ever on a server.
 */

const PAID = new Set(['succeeded', 'confirmed', 'paid', 'completed', 'active'])

/** Polar has changed field names before, so every shape we know of is tried. */
function readCheckout(c) {
  const status = String(c.status ?? c.payment_status ?? '').toLowerCase()
  const email = c.customer_email ?? c.customer?.email ?? c.customer_billing_address?.email ?? ''
  const product =
    c.product_id ??
    c.product?.id ??
    (Array.isArray(c.products) ? (c.products[0]?.id ?? c.products[0]) : '') ??
    ''
  const amount = Number(c.total_amount ?? c.amount ?? c.subtotal_amount ?? 0) || 0
  const currency = (c.currency ?? 'USD').toUpperCase()
  return { status, email, product, amount, currency }
}

function planOf(productId) {
  const map = {
    [process.env.POLAR_PRODUCT_MONTHLY || 'f4626775-4828-4575-af13-f1ca156e3464']: 'monthly',
    [process.env.POLAR_PRODUCT_YEARLY || 'd1c24c98-bb29-4f53-9ab0-101f1f7b5e5f']: 'yearly',
    [process.env.POLAR_PRODUCT_LIFETIME || 'eb318663-fdf2-40c4-9dac-14ba07be52cb']: 'lifetime',
  }
  return map[productId] || ''
}

export default async (request) => {
  const url = new URL(request.url)
  const id = url.searchParams.get('checkout_id') || url.searchParams.get('id') || ''
  if (!/^[A-Za-z0-9_-]{6,80}$/.test(id)) {
    return json({ ok: false, error: 'No payment to look up.' }, 400)
  }
  const token = process.env.POLAR_ACCESS_TOKEN
  if (!token) return json({ ok: false, error: 'Payments are not configured yet.' }, 500)

  const res = await fetch(`${polarBase()}/v1/checkouts/${id}`, {
    headers: { Authorization: `Bearer ${token}` },
  })
  if (!res.ok) {
    console.error('polar lookup', res.status, (await res.text()).slice(0, 600))
    return json({ ok: false, error: 'We could not find that payment.' }, 404)
  }
  const raw = await res.json()
  // The first real purchase prints its shape here, which is how the field
  // names above get confirmed rather than guessed.
  console.log('polar checkout', JSON.stringify(raw).slice(0, 2000))

  const c = readCheckout(raw)
  if (!PAID.has(c.status)) {
    return json({ ok: false, error: `That payment is ${c.status || 'not complete'}.` }, 402)
  }
  const plan = planOf(c.product)
  if (!plan) {
    console.error('unknown product', c.product)
    return json({ ok: false, error: 'That purchase is not a Lane plan.' }, 400)
  }

  const claim = await db('rpc/claim_licence', {
    method: 'POST',
    body: JSON.stringify({
      p_payment_id: id,
      p_plan: plan,
      p_email: c.email,
      p_amount: c.amount,
      p_currency: c.currency,
    }),
  })
  if (!claim?.ok) {
    console.error('claim failed', claim)
    return json({ ok: false, error: 'We are out of keys for that plan. Write to pb@lane.so and we will send one within the hour.' }, 409)
  }
  return json({ ok: true, key: claim.key, plan, email: c.email })
}
