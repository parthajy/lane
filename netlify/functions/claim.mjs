import { db, json } from './_lib.mjs'

/**
 * After paying, the buyer lands on /thanks and this hands them their key.
 *
 * The payment is checked against Dodo from here, with the secret key, so a
 * made-up payment id gets nothing. The key itself was signed on Partha's Mac
 * and uploaded: no signing secret is ever on a server.
 */

const dodoBase = () =>
  process.env.DODO_MODE === 'live' ? 'https://api.dodopayments.com' : 'https://test.dodopayments.com'

const PAID = new Set(['succeeded', 'paid', 'completed', 'active', 'processing_succeeded'])

/** Dodo has changed field names before, so every shape we have seen is tried. */
function readPayment(p) {
  const status = String(p.status ?? p.payment_status ?? '').toLowerCase()
  const email = p.customer?.email ?? p.customer_email ?? p.billing?.email ?? ''
  const cart = Array.isArray(p.product_cart) ? p.product_cart : []
  const product =
    p.product_id ??
    cart[0]?.product_id ??
    p.subscription?.product_id ??
    p.items?.[0]?.product_id ??
    ''
  const amount = Number(p.total_amount ?? p.amount ?? p.settlement_amount ?? 0) || 0
  const currency = p.currency ?? p.settlement_currency ?? 'USD'
  return { status, email, product, amount, currency }
}

const TEST = {
  monthly: 'pdt_0No5etdA408odWxLdLHjI',
  yearly: 'pdt_0No5ethe62xSynYBoWPnO',
  lifetime: 'pdt_0No5eqhNCUK6AbLdVMdL3',
}

function planOf(productId) {
  const map = {
    [process.env.DODO_PRODUCT_MONTHLY || TEST.monthly]: 'monthly',
    [process.env.DODO_PRODUCT_YEARLY || TEST.yearly]: 'yearly',
    [process.env.DODO_PRODUCT_LIFETIME || TEST.lifetime]: 'lifetime',
  }
  return map[productId] || ''
}

export default async (request) => {
  const url = new URL(request.url)
  const paymentId =
    url.searchParams.get('payment_id') ||
    url.searchParams.get('paymentId') ||
    url.searchParams.get('subscription_id') ||
    ''
  if (!/^[A-Za-z0-9_-]{6,80}$/.test(paymentId)) {
    return json({ ok: false, error: 'No payment to look up.' }, 400)
  }
  const key = process.env.DODO_API_KEY
  if (!key) return json({ ok: false, error: 'Payments are not configured yet.' }, 500)

  const isSub = url.searchParams.has('subscription_id')
  const res = await fetch(`${dodoBase()}/${isSub ? 'subscriptions' : 'payments'}/${paymentId}`, {
    headers: { Authorization: `Bearer ${key}` },
  })
  if (!res.ok) {
    console.error('dodo lookup', res.status, await res.text())
    return json({ ok: false, error: 'We could not find that payment.' }, 404)
  }
  const raw = await res.json()
  // The first real purchase prints its shape here, which is how the field
  // names above get confirmed rather than guessed.
  console.log('dodo payment', JSON.stringify(raw).slice(0, 2000))

  const p = readPayment(raw)
  if (!PAID.has(p.status)) {
    return json({ ok: false, error: `That payment is ${p.status || 'not complete'}.` }, 402)
  }
  const plan = planOf(p.product)
  if (!plan) {
    console.error('unknown product', p.product)
    return json({ ok: false, error: 'That purchase is not a Lane plan.' }, 400)
  }

  const claim = await db('rpc/claim_licence', {
    method: 'POST',
    body: JSON.stringify({
      p_payment_id: paymentId,
      p_plan: plan,
      p_email: p.email,
      p_amount: p.amount,
      p_currency: p.currency,
    }),
  })
  if (!claim?.ok) {
    console.error('claim failed', claim)
    return json({ ok: false, error: 'We are out of keys for that plan. Write to pb@lane.so and we will send one within the hour.' }, 409)
  }
  return json({ ok: true, key: claim.key, plan, email: p.email })
}
