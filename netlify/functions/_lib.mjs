/** The bits every function needs: Supabase over plain fetch, and the admin gate. */

const URL_BASE = () => (process.env.SUPABASE_URL || '').replace(/\/+$/, '')
const SERVICE = () => process.env.SUPABASE_SERVICE_KEY || ''

/** A call to PostgREST with the service key, which bypasses row level security. */
export async function db(path, init = {}) {
  const res = await fetch(`${URL_BASE()}/rest/v1/${path}`, {
    ...init,
    headers: {
      apikey: SERVICE(),
      Authorization: `Bearer ${SERVICE()}`,
      'Content-Type': 'application/json',
      ...(init.headers || {}),
    },
  })
  const text = await res.text()
  let body = null
  try { body = text ? JSON.parse(text) : null } catch { body = text }
  if (!res.ok) throw new Error(`supabase ${res.status}: ${typeof body === 'string' ? body : JSON.stringify(body)}`)
  return body
}

/** How many rows match, without carrying them across the wire. */
export async function count(table, query = '') {
  const res = await fetch(`${URL_BASE()}/rest/v1/${table}?select=id${query ? `&${query}` : ''}`, {
    headers: { apikey: SERVICE(), Authorization: `Bearer ${SERVICE()}`, Prefer: 'count=exact', Range: '0-0' },
  })
  const range = res.headers.get('content-range') || '*/0'
  return Number(range.split('/')[1] || 0)
}

export function adminOk(request) {
  const token = process.env.LANE_ADMIN_TOKEN || ''
  if (!token) return false
  const url = new URL(request.url)
  const given = url.searchParams.get('token') || request.headers.get('x-admin-token') || ''
  // Constant time enough for a token this long, and the comparison never
  // short-circuits on length alone.
  if (given.length !== token.length) return false
  let diff = 0
  for (let i = 0; i < token.length; i++) diff |= token.charCodeAt(i) ^ given.charCodeAt(i)
  return diff === 0
}

export const json = (body, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } })

export const html = (body, status = 200) =>
  new Response(body, { status, headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' } })

export const escape = (s) =>
  String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]))
