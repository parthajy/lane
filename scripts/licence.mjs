#!/usr/bin/env node
// Lane licence keys.
//
//   node scripts/licence.mjs keygen                 make the signing key (once)
//   node scripts/licence.mjs sign <email> <plan>    one key, printed
//   node scripts/licence.mjs mint <plan> <count>    a batch, as CSV for Supabase
//
// A key is `lane1|<email>|<plan>|<issued ms>::<signature>`, where the
// signature is a minisign signature file in base64 so the whole thing is one
// pasteable line. Lane checks it on the buyer's own Mac; nothing is activated
// against a server. Node has Ed25519 and BLAKE2b built in, so this needs no
// dependencies and can run anywhere.
import crypto from 'node:crypto'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

const KEY_FILE = process.env.LANE_LICENCE_KEY || path.join(os.homedir(), '.tauri', 'lane-licence.json')
const PKCS8_ED25519 = Buffer.from('302e020100300506032b657004220420', 'hex')

/** The minisign public key line the app embeds: alg || key id || key. */
function publicKeyLine(keyId, pub) {
  return Buffer.concat([Buffer.from([0x45, 0x64]), keyId, pub]).toString('base64')
}

export function keygen() {
  const { publicKey, privateKey } = crypto.generateKeyPairSync('ed25519')
  const pub = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32)
  const seed = privateKey.export({ type: 'pkcs8', format: 'der' }).subarray(-32)
  const keyId = crypto.randomBytes(8)
  return {
    keyId: keyId.toString('hex'),
    seed: seed.toString('hex'),
    publicKey: publicKeyLine(keyId, pub),
    // What goes into the app: the two-line minisign public key file, base64'd.
    publicKeyFile: Buffer.from(
      `untrusted comment: minisign public key: ${Buffer.from(keyId).reverse().toString('hex').toUpperCase()}\n` +
      `${publicKeyLine(keyId, pub)}\n`,
    ).toString('base64'),
  }
}

function loadKey() {
  if (process.env.LANE_LICENCE_SEED && process.env.LANE_LICENCE_KEYID) {
    return { seed: process.env.LANE_LICENCE_SEED, keyId: process.env.LANE_LICENCE_KEYID }
  }
  if (!fs.existsSync(KEY_FILE)) {
    throw new Error(`no signing key at ${KEY_FILE}. Run: node scripts/licence.mjs keygen`)
  }
  return JSON.parse(fs.readFileSync(KEY_FILE, 'utf8'))
}

/** Sign one payload. Returns the signature file, base64, as one line. */
export function signPayload(payload, key = loadKey()) {
  const seed = Buffer.from(key.seed, 'hex')
  const keyId = Buffer.from(key.keyId, 'hex')
  const priv = crypto.createPrivateKey({
    key: Buffer.concat([PKCS8_ED25519, seed]),
    format: 'der',
    type: 'pkcs8',
  })
  // Prehashed ("ED"): the signature is over BLAKE2b-512 of the payload.
  const digest = crypto.createHash('blake2b512').update(Buffer.from(payload, 'utf8')).digest()
  const sig = crypto.sign(null, digest, priv)
  const line1 = Buffer.concat([Buffer.from([0x45, 0x44]), keyId, sig])
  const trusted = `timestamp:${Math.floor(Date.now() / 1000)}\tfile:lane-licence`
  const global = crypto.sign(null, Buffer.concat([sig, Buffer.from(trusted, 'utf8')]), priv)
  const file =
    `untrusted comment: Lane licence\n${line1.toString('base64')}\n` +
    `trusted comment: ${trusted}\n${global.toString('base64')}\n`
  return Buffer.from(file, 'utf8').toString('base64')
}

export function makeKey(email, plan, key) {
  // The nonce is what makes every key different. Without it a batch minted
  // inside the same millisecond signs the same payload, and every buyer in
  // that batch would be handed the same key. Lane reads the first three
  // fields and ignores the rest, so this is safe to append.
  const nonce = crypto.randomBytes(6).toString('base64url')
  const payload = `lane1|${email}|${plan}|${Date.now()}|${nonce}`
  return `${payload}::${signPayload(payload, key)}`
}

const PLANS = new Set(['monthly', 'yearly', 'lifetime'])

function main() {
  const [cmd, a, b] = process.argv.slice(2)
  if (cmd === 'keygen') {
    if (fs.existsSync(KEY_FILE) && !process.env.LANE_FORCE) {
      console.error(`${KEY_FILE} already exists. Signing with a new key would strand every licence already issued.`)
      console.error('Set LANE_FORCE=1 only if you are sure.')
      process.exit(1)
    }
    const k = keygen()
    fs.mkdirSync(path.dirname(KEY_FILE), { recursive: true })
    fs.writeFileSync(KEY_FILE, JSON.stringify({ keyId: k.keyId, seed: k.seed }, null, 2), { mode: 0o600 })
    console.log(`secret written to ${KEY_FILE} — back it up, it cannot be recovered`)
    console.log('\npublic key for src-tauri/src/licence_pubkey.txt:\n')
    console.log(k.publicKeyFile)
    return
  }
  if (cmd === 'sign') {
    if (!a || !PLANS.has(b)) {
      console.error('usage: node scripts/licence.mjs sign <email> <monthly|yearly|lifetime>')
      process.exit(2)
    }
    console.log(makeKey(a, b))
    return
  }
  if (cmd === 'mint') {
    const plan = a
    const n = Number(b || 0)
    if (!PLANS.has(plan) || !Number.isInteger(n) || n < 1 || n > 5000) {
      console.error('usage: node scripts/licence.mjs mint <monthly|yearly|lifetime> <1..5000>')
      process.exit(2)
    }
    const key = loadKey()
    const seen = new Set()
    const rows = []
    while (rows.length < n) {
      const k = makeKey('a Lane customer', plan, key)
      if (seen.has(k)) continue
      seen.add(k)
      rows.push(k)
    }
    console.log('plan,licence_key')
    for (const k of rows) console.log(`${plan},"${k}"`)
    return
  }
  console.error('usage: node scripts/licence.mjs <keygen|sign|mint> …')
  process.exit(2)
}

if (import.meta.url === `file://${process.argv[1]}`) main()
