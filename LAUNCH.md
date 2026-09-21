# Launching Lane

Everything here is done from this Mac. The order matters only where one step
feeds the next; where it does not, do whichever is in front of you.

## 1. Notarise the build (do this first, it takes the longest)

Apple needs credentials stored once, on this Mac. It asks for your Apple ID,
an app-specific password from appleid.apple.com, and the team ID 6AKUD88CVN.

```sh
xcrun notarytool store-credentials lane
```

Then the whole release runs in one command:

```sh
npm run release        # builds, signs, notarises, staples, writes the update manifest
```

Notarisation takes a few minutes and Apple sometimes queues it for longer. If
it fails, `xcrun notarytool log <submission-id> --keychain-profile lane` says
why, and it is almost always an unsigned helper binary.

When it finishes you have `src-tauri/target/release/bundle/dmg/Lane-<version>.dmg`,
stapled, and `dist-updates/<version>/` for the updater.

Check Gatekeeper is happy with it before anyone else sees it:

```sh
spctl --assess --type open --context context:primary-signature -v \
  src-tauri/target/release/bundle/dmg/Lane-<version>.dmg
xcrun stapler validate src-tauri/target/release/bundle/dmg/Lane-<version>.dmg
```

Better still, copy the dmg to another Mac and open it there. A build that
passes on the machine that made it can still fail on a stranger's.

## 2. Put the server up

See `server/README.md`. Short version: copy `server/` to the box, make the
venv, write `/opt/lane/lane.env` with a long `LANE_ADMIN_TOKEN`, enable the
systemd unit, put Caddy in front of it for the certificate.

Then point the site at it: in `site/index.html`, `<meta name="lane-api">`
should read `https://api.lane.so`.

Your dashboard is `https://api.lane.so/admin?token=<your token>`. It shows
downloads by day, the waitlist, seats left, feedback and sales.

## 3. Put the site up

`site/` is a folder of static files. Upload it. The only moving part is the
waitlist form, which posts to the server above; if the server is down the page
still reads correctly and the counter simply does not move.

Upload the dmg too, and point `LANE_DMG_URL` at it. Link the download button
at `https://api.lane.so/download/mac` rather than the file itself, so the
clicks are counted.

## 4. Selling

There is no payment page yet, and nothing in Lane talks to a payment
processor. For the first buyers, take the money however you like and issue the
key by hand:

```sh
scripts/issue-licence.sh buyer@example.com lifetime
```

That prints one line. Paste it into the receipt. The buyer opens Lane,
Settings, Your licence, and pastes it in. Lane checks the signature on their
own machine, so nothing is activated against a server and the app stays
offline. Record the sale so the dashboard knows:

```sh
curl -X POST "https://api.lane.so/api/licences?token=<token>" \
  -d "email=buyer@example.com&plan=lifetime&amount=500"
```

Plans are `monthly`, `yearly` and `lifetime`. A monthly key does not expire on
its own yet, so issue those only when you are ready to track renewals by hand.

## 5. What is still open

- **Screen Recording permission.** Lane cannot capture anything until macOS
  grants it, and granting it needs your password, so it has to be you. System
  Settings, Privacy & Security, Screen Recording, switch Lane on.
- **The signing key.** `~/.tauri/lane.key` signs both updates and licences.
  Lose it and you cannot ship an update to anyone who already installed Lane.
  Copy it somewhere safe that is not this Mac, today.
- **Placeholders on the site.** `hello@lane.so` has to be a real mailbox, and
  the X and GitHub links in the footer still point nowhere.
- **The footer pages.** About thirty links in the footer have no page behind
  them yet. Either write them or cut them before launch.
- **Monthly and yearly keys** need a renewal story. Lifetime is the only plan
  that is honest with the current code.
