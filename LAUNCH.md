# Launching Lane

Four places hold the launch: this Mac builds the app, GitHub holds the code,
Supabase holds the database, Netlify serves the site and the few functions,
and Polar takes the money. The app itself talks to none of them.

## 1. Supabase (five minutes, do it first)

One command does all of it, with a personal access token from
https://supabase.com/dashboard/account/tokens:

```sh
SUPABASE_ACCESS_TOKEN=sbp_… python3 scripts/setup-supabase.py
```

That makes the tables, locks them so the website can read nothing directly,
adds the three functions the site is allowed to call, and loads the 750
licence keys from `~/.tauri/lane-keys/`. Running it twice is safe. Add
`--dry-run` to see what it would send without sending it.

By hand instead: paste `supabase/schema.sql` into the SQL editor, then import
the three CSVs into `licence_keys` from the table editor.

You will need two things from Project settings → API:

- the **publishable key**, already in `site/index.html`, safe in public
- the **service role key**, which is a secret and goes only into Netlify

## 2. Netlify

Import the repository `parthajy/lane` and take the settings it offers, because
`netlify.toml` already says the site is `site/`, the functions are
`netlify/functions/`, and there is no build step.

Everything public is in the code already: the Supabase project URL, the Polar
product ids, the download link. Only four variables need setting, in Site
configuration → Environment variables, and three of them are secrets that must
never be committed to this repository:

| Name | What it is |
| --- | --- |
| `SUPABASE_SERVICE_KEY` | the project's secret key, which bypasses row level security |
| `POLAR_ACCESS_TOKEN` | the Polar organization access token |
| `LANE_ADMIN_TOKEN` | a long random string you invent, the only thing guarding the dashboard |
| `LANE_DMG_URL` | only if the build is not on the releases page |

Two more are worth setting when you go live: `DODO_MODE=live`, and the three
`DODO_PRODUCT_*` ids for the live products.

Your dashboard is then `https://lane.so/admin?token=<your token>`: downloads by
day, the waitlist, seats left, sales, keys still in the pool, and feedback.

## 3. Payments: Dodo first, Polar behind it

`/buy/lifetime` tries each configured gateway in order and sends the buyer to
the first that works. If Dodo is down or refuses, they go to Polar instead of
seeing an error. The receipt carries which one took the money, so the licence
is verified against the right place.

Set in Netlify:

| Name | Value |
| --- | --- |
| `DODO_API_KEY` | the live Dodo secret key |
| `DODO_PRODUCT_LIFETIME` | the live product id for $499 |
| `DODO_PRODUCT_MONTHLY` | the live product id for $9 |
| `DODO_PRODUCT_YEARLY` | the live product id for $89 |
| `POLAR_ACCESS_TOKEN` | the Polar organization access token |
| `PAY_ORDER` | optional; `dodo,polar` is the default |
| `DODO_MODE` | optional; set to `test` to use Dodo's test environment |
| `POLAR_MODE` | optional; set to `sandbox` for Polar's |

Polar's three product ids are already in the code. Dodo's are not, because
the live ones do not exist until you make them.

**Test before you announce anything.** Buy the lifetime plan, check the key
appears on `/thanks/`, paste it into Lane under Settings and Your licence, and
confirm the sale shows on `/admin`.

## 4. Notarise and ship the app

Apple needs credentials stored once, on this Mac. It asks for your Apple ID, an
app-specific password from appleid.apple.com, and the team ID 6AKUD88CVN.

```sh
xcrun notarytool store-credentials lane
npm run release              # build, sign, notarise, write the update manifest
scripts/release-github.sh    # upload the dmg and the update artefact
git add site/updates && git commit -m "Update manifest" && git push
```

Check Gatekeeper is happy before anyone else sees it:

```sh
spctl --assess --type open --context context:primary-signature -v \
  src-tauri/target/release/bundle/dmg/Lane-<version>.dmg
xcrun stapler validate src-tauri/target/release/bundle/dmg/Lane-<version>.dmg
```

Better still, copy the dmg to another Mac and open it there. A build that
passes on the machine that made it can still fail on a stranger's.

### Where the dmg lives

On the repository's releases page. It is free, it has no bandwidth limit for a
public repository, and it is fast everywhere. `scripts/release-github.sh`
uploads the build twice: once named for its version, and once as `Lane.dmg`, so
this link is always the current build and never has to be edited:

```
https://github.com/parthajy/lane/releases/latest/download/Lane.dmg
```

Always send people to `/download/mac` rather than that link, so the clicks are
counted. The updater reads `https://lane.so/updates/darwin-aarch64.json`, which
Netlify serves from `site/updates/`, and which points back at the release.

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
- **Placeholders on the site.** `pb@lane.so` has to be a real mailbox, and
  the X and GitHub links in the footer still point nowhere.
- **The footer pages.** About thirty links have no page behind them. Write them
  or cut them before launch.
- **Monthly and yearly keys never expire on their own.** Lifetime is the only
  plan the current code tells the truth about, so sell that one first.
