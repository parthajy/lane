# Launching Lane

Four places hold the launch: this Mac builds the app, GitHub holds the code,
Supabase holds the database, Netlify serves the site and the few functions,
and Dodo takes the money. The app itself talks to none of them.

## 1. Supabase (five minutes, do it first)

Open the SQL editor for the project at
`https://fuqrvmprgzqjmfqszxoe.supabase.co`, paste the whole of
`supabase/schema.sql`, and run it. It makes the tables, locks them so the
website can read nothing directly, and adds the three functions the site is
allowed to call.

Then upload the licence keys. They were minted on this Mac and live in
`~/.tauri/lane-keys/` as three CSV files, 250 keys each. In the table editor,
open `licence_keys`, choose Import data from CSV, and load all three. Nothing
else ever has to know the signing secret.

You will need two things from Project settings → API:

- the **publishable key**, already in `site/index.html`, safe in public
- the **service role key**, which is a secret and goes only into Netlify

## 2. Netlify

Connect the GitHub repository `parthajy/lane`. The build settings come from
`netlify.toml`, so there is nothing to type: the site is `site/` and the
functions are `netlify/functions/`.

Set these environment variables in Site configuration → Environment variables:

| Name | Value |
| --- | --- |
| `SUPABASE_URL` | `https://fuqrvmprgzqjmfqszxoe.supabase.co` |
| `SUPABASE_SERVICE_KEY` | the service role key, from Supabase |
| `LANE_ADMIN_TOKEN` | a long random string you invent |
| `LANE_DMG_URL` | where the dmg is served from |
| `DODO_API_KEY` | the Dodo secret key |
| `DODO_MODE` | `test` while you are testing, `live` when you are not |
| `DODO_PRODUCT_LIFETIME` | `pdt_0No5eqhNCUK6AbLdVMdL3` |
| `DODO_PRODUCT_MONTHLY` | `pdt_0No5etdA408odWxLdLHjI` |
| `DODO_PRODUCT_YEARLY` | `pdt_0No5ethe62xSynYBoWPnO` |

Your dashboard is then `https://lane.so/admin?token=<your token>`: downloads by
day, the waitlist, seats left, sales, keys still in the pool, and feedback.

## 3. Dodo

The three products already exist in test mode, with the ids above. When you are
ready to take real money, make the same three in live mode, put the live ids and
the live key into Netlify, and set `DODO_MODE=live`.

The flow needs nothing else: `/buy/lifetime` sends the buyer to Dodo, Dodo sends
them back to `/thanks/`, and that page asks Netlify for their key. The key is
claimed from the pool once per payment, so a reloaded page shows the same key
rather than burning another.

**Test it before you announce anything.** Buy the lifetime plan in test mode with
Dodo's test card, check the key appears on `/thanks/`, paste it into Lane under
Settings → Your licence, and confirm the sale shows on `/admin`.

## 4. Notarise and ship the app

Apple needs credentials stored once, on this Mac. It asks for your Apple ID, an
app-specific password from appleid.apple.com, and the team ID 6AKUD88CVN.

```sh
xcrun notarytool store-credentials lane
npm run release        # builds, signs, notarises, staples, writes the update manifest
```

Then check Gatekeeper is happy before anyone else sees it:

```sh
spctl --assess --type open --context context:primary-signature -v \
  src-tauri/target/release/bundle/dmg/Lane-<version>.dmg
xcrun stapler validate src-tauri/target/release/bundle/dmg/Lane-<version>.dmg
```

Better still, copy the dmg to another Mac and open it there. A build that
passes on the machine that made it can still fail on a stranger's.

Upload the dmg wherever `LANE_DMG_URL` points, and always link people at
`/download/mac` so the clicks are counted.

## 5. Issuing a key by hand

For a refund, a replacement, or someone who paid you another way:

```sh
node scripts/licence.mjs sign someone@example.com lifetime
```

That prints one line to paste into their receipt. To refill the pool:

```sh
node scripts/licence.mjs mint lifetime 250 > lifetime.csv   # then import to Supabase
```

## What is still open

- **Screen Recording.** Lane captures nothing until macOS grants it, and that
  needs your password, so it has to be you. System Settings → Privacy &
  Security → Screen Recording → Lane.
- **The signing keys.** `~/.tauri/lane.key` signs updates and
  `~/.tauri/lane-licence.json` signs licences. Lose the first and you can never
  update anyone; lose the second and you can never mint another key. Copy both
  somewhere that is not this Mac, today.
- **The Windows workflow** could not be pushed: the GitHub token this Mac holds
  has no `workflow` scope. Run `gh auth refresh -s workflow`, then
  `git add .github && git commit -m "CI" && git push`.
- **Placeholders on the site.** `hello@lane.so` has to be a real mailbox, and
  the X and GitHub links in the footer still point nowhere.
- **The footer pages.** About thirty links have no page behind them. Write them
  or cut them before launch.
- **Monthly and yearly keys never expire on their own.** Lifetime is the only
  plan the current code tells the truth about, so sell that one first.
