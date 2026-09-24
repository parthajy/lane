import { db, json } from './_lib.mjs'
import { verify, GATEWAYS } from './_pay.mjs'

/**
 * After paying, the buyer lands on /thanks and this hands them their key.
 *
 * The payment is checked against the gateway that took it, from here, with
 * the secret key, so a made-up id gets nothing. The key itself was signed on
 * Partha's Mac and uploaded: no signing secret is ever on a server.
 */
export default async (request) => {
  const url = new URL(request.url)
  const id =
    url.searchParams.get('payment_id') ||
    url.searchParams.get('checkout_id') ||
    url.searchParams.get('subscription_id') ||
    url.searchParams.get('id') || ''
  if (!/^[A-Za-z0-9_-]{6,100}$/.test(id)) {
    return json({ ok: false, error: 'No payment to look up.' }, 400)
  }

  // The receipt says which gateway took the money; if it does not, ask both.
  const said = (url.searchParams.get('gw') || '').toLowerCase()
  const order = GATEWAYS.includes(said) ? [said] : GATEWAYS

  let last = { ok: false, error: 'We could not find that payment.' }
  for (const gw of order) {
    const seen = await verify(gw, id)
    if (seen.ok) {
      const claim = await db('rpc/claim_licence', {
        method: 'POST',
        body: JSON.stringify({
          p_payment_id: `${gw}:${id}`,
          p_plan: seen.plan,
          p_email: seen.email,
          p_amount: seen.amount,
          p_currency: seen.currency,
        }),
      })
      if (!claim?.ok) {
        console.error('claim failed', claim)
        return json({ ok: false, error: 'We are out of keys for that plan. Write to pb@lane.so and we will send one within the hour.' }, 409)
      }
      return json({ ok: true, key: claim.key, plan: seen.plan, email: seen.email })
    }
    last = seen
  }
  return json(last, 402)
}
