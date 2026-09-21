// POST /v1/labels  Authorization: Bearer <tester code>
// Body: {"app":"lane","version":"0.1.0","labels":[...]}  → stored as
// labels/<tester>/<timestamp>.jsonl. Nothing else is accepted or kept.
export default {
  async fetch(request, env) {
    const url = new URL(request.url)
    if (request.method !== 'POST' || url.pathname !== '/v1/labels') {
      return new Response('not found', { status: 404 })
    }
    const auth = request.headers.get('Authorization') || ''
    const code = auth.startsWith('Bearer ') ? auth.slice(7).trim() : ''
    const tester = code && (await env.TESTERS.get(code))
    if (!tester) return new Response('unknown tester code', { status: 401 })
    if ((request.headers.get('Content-Length') || 0) > 25_000_000) return new Response('too large', { status: 413 })
    let body
    try {
      body = await request.json()
    } catch {
      return new Response('bad json', { status: 400 })
    }
    const labels = Array.isArray(body?.labels) ? body.labels : []
    if (labels.length === 0) return new Response('no labels', { status: 400 })
    const key = `labels/${tester}/${new Date().toISOString().replace(/[:.]/g, '-')}.jsonl`
    const lines = labels.map((l) => JSON.stringify({ ...l, tester, app_version: body.version || '' })).join('\n') + '\n'
    await env.LABELS.put(key, lines, { httpMetadata: { contentType: 'application/x-ndjson' } })
    return Response.json({ stored: labels.length, key })
  },
}
