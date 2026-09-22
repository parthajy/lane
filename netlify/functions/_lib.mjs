/** The bits every function needs: Supabase over plain fetch, and the admin gate. */

// The project URL is public: it ships in the page. Only the key is secret.
const PROJECT = 'https://fuqrvmprgzqjmfqszxoe.supabase.co'
const URL_BASE = () => (process.env.SUPABASE_URL || PROJECT).replace(/\/+$/, '')
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

/** The credential can arrive three ways: a cookie set once, a header for
    scripts, or the query string for a one-off. */
export function tokenFrom(request) {
  const url = new URL(request.url)
  const cookie = (request.headers.get('cookie') || '')
    .split(';').map((c) => c.trim()).find((c) => c.startsWith('lane_admin='))
  return url.searchParams.get('token')
    || request.headers.get('x-admin-token')
    || (cookie ? decodeURIComponent(cookie.slice('lane_admin='.length)) : '')
}

export function tokenIsGood(given) {
  const token = process.env.LANE_ADMIN_TOKEN || ''
  if (!token || !given) return false
  if (given.length !== token.length) return false
  let diff = 0
  for (let i = 0; i < token.length; i++) diff |= token.charCodeAt(i) ^ given.charCodeAt(i)
  return diff === 0
}

export function adminOk(request) {
  const token = process.env.LANE_ADMIN_TOKEN || ''
  if (!token) return false
  const given = tokenFrom(request)
  return tokenIsGood(given)
}

export const json = (body, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } })

export const html = (body, status = 200) =>
  new Response(body, { status, headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' } })

export const escape = (s) =>
  String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]))
