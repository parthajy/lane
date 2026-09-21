# Lane

**Lane** is a desktop app (Mac now, Windows next) that remembers what you do on your computer: the apps, windows, pages and text in front of you, your meetings, and the documents on your disk. Everything is stored encrypted and processed on your own machine. Nothing leaves it.

Lane is **powered by Rabbit**, our own memory model. Externally there is Lane and Rabbit; the components inside Rabbit are never named in the product. lane.so · bundle id `so.lane.app` (the app adopts a Reattend-era data folder and Keychain key on first launch).

Plan: Lane for individuals (invite-only testers first, free forever for the first 100) → the first Rabbit S fine-tune from their labels → Windows → hosted end-to-end-encrypted sync and share links → org server, which is where Lane and Reattend meet.

## The landing page (`site/`)

A static, dependency-free site: `index.html`, `styles.css`, `app.js`, `arch.js`, `og.html` → `og.png`, `logo.png`, `favicon.svg`, `robots.txt`, `sitemap.xml` (34 URLs). Open `site/index.html` in a browser; deploy by copying the folder to any static host.

Light, with colour carried by whole sections rather than accents alone. The hero is a painted sky (lilac into peach into cream, with a soft grain overlay) and no animation at all; the navbar sits on that sky and scrolls away with the page rather than sticking. Below it: white product sections, three tinted figure plates (peach, lilac, sky), a tinted bento, one deliberately dark privacy band, and a peach closing block with the notch card beside it. Tokens: ink `#101014`, indigo `#5a51e5`, plus blue, violet, coral and amber. Inter Tight for display, Inter for text, JetBrains Mono for labels. The wave mark from `public/dark.png` is the logo throughout, and the isometric figure plates come from the generator kept in `site/figs.json`.

The dark band works by redefining the colour tokens inside `.sec.dark` rather than restyling its components, so everything in it inverts on its own. Watch for class-name collisions when doing that: an arch chip named `.vault` once landed on the privacy section's `.wrap.vault` grid and painted it white-on-white inside the dark band, which is why the chip is now `.vault-chip`.

`arch.js` animates the architecture section (`#rabbit`), the page's centrepiece: eight named sources (Google Chrome, Gmail, Microsoft Word, files, Zoom, Calendar, Slack, your voice) stream coloured particles in waves from both sides into a Rabbit card that pulses as each one lands; a thicker indigo stream runs down into the encrypted vault, and three streams fan back out to the notch tab, ⌥Space recall and the app. Node positions come from the live DOM (`[data-node]`), so the curves re-route at any width and when the columns stack on a phone. Streams are seeded on build, and each shares one moving phase so its dots read as a single travelling wave.

`app.js` runs the four small performances, each gated on an IntersectionObserver so nothing animates off screen: Ask types three real questions in turn and builds each answer word by word with its citations; the meetings panel reveals the call line by line, lighting the speaker's pill as it goes and finishing on the action item; the recall panel opens its hits in sequence; and the dictation panel types what was said while a ribbon of speech scrolls behind a live waveform. The notch section is a drawn Mac desktop (`.desk`): a wallpaper of stacked gradients, a translucent menu bar, the dark Lane panel hanging from the notch, and a dock. Bento tiles are solid colour blocks on a six-column grid with uneven spans (`big`, `tall`, `wide`, `full`).

The page covers the whole product: passive intake, Rabbit on the machine, the notch tab, Ask with citations, three things, meetings with named voices, recall and dictation, then a bento for commitments, briefs, why and drift, people, files, board, pictures and Windows. SEO is built in: canonical, Open Graph and Twitter cards, JSON-LD for Organization, SoftwareApplication and a ten-question FAQPage. The footer carries five columns of landing-page slots whose pages still need writing.

Copy rule on the site: **Rabbit is named** — it is our own model and the reason nothing leaves the Mac, so the page says so plainly ("no external model, no API key"). What is never named is anything inside Rabbit, and the words "AI", "smart" and "powered by" stay out.

## What Lane costs

Seven weeks free with everything on and no card, then $9 a month, $90 a year, or $500 once for a lifetime licence. There is no free tier: a memory app funded by anything other than the person using it is a contradiction. The site, its FAQ and its structured data all carry these numbers.

## Licence, trial and feedback

The trial is seven weeks (`licence::TRIAL_DAYS = 49`), started the first time Lane runs and kept in the Keychain under `so.lane.app.trial`, so deleting the app and its data does not hand out another seven weeks. A licence is a signed note, not an account: `lane1|<email>|<plan>|<issued ms>::<signature>`, verified offline in `licence.rs` against the public half of the updater key (`src/licence_pubkey.txt`). The signature travels as base64 of the whole minisign signature file, so a key is one pasteable line, and `PublicKey::from_base64` is what parses the key (`decode` takes bytes and silently fails).

Issue one with `scripts/issue-licence.sh <email> <monthly|yearly|lifetime>`; it signs with `~/.tauri/lane.key`, whose password is empty. `cargo test --lib licence` checks that an issued key verifies and an edited one does not, and `-- --ignored` also runs the Keychain round trip.

When the trial runs out, `AppState.blocked` stands the capture and engine loops down, the notch says so, and `LicenceWall` covers the window. Nothing is deleted, and pasting a key brings everything back.

Feedback (`components/feedback.tsx`, from Help & support) writes the note and hands it to the mail app or the clipboard. Lane itself sends nothing, which is why there is no in-app "submit".

## The launch server (`server/`)

One small FastAPI service, SQLite behind it, that the app never touches: the waitlist and its first-200 lifetime seats, feedback from the site, download counts, recorded sales, and an admin page at `/admin?token=…`. See `server/README.md` to put it on a box, and `LAUNCH.md` for the order of the whole release.

## Categories and the share card

`clean::place_of` turns an activity into where it happened — the app, or the site when the app is a browser — and `clean::category_of` sorts that into nine coarse buckets (assistants, social, watching, talking to people, building, documents and planning, money and admin, shopping, reading, browsing, everything else). `Explore` carries `by_category` and `top_places` for the last 30 days, Explore shows both, and `components/share-card.tsx` draws them onto a canvas the person can save to their Desktop as a picture. Nothing is uploaded; posting it is their decision.

## The app's look

The window is two floating panels on a soft, faintly lilac ground (`.app-ground`, `.panel` in `src/styles/globals.css`), in the register of the LoadLogic and TruConversion dashboards Partha chose as references. The sidebar carries the wave mark, a "Your memory · Local · this Mac" card with the pause control, one dark pill for Today with the day's count, a 2×2 grid of nav tiles for Ask, Memories, Meetings, Files, People and Board, a quiet Integrations row, and a Rabbit card at the bottom that reports how many memories live on this Mac and links to the labelling screen. The main panel holds a pill search, the Capture button as the one solid dark action, and the page.

The sidebar navigation is one grouped list, not a grid: Today and Ask at the top, then **Your memory** (Memories, Meetings, Files, People, Board) and **Settings** (Integrations, Settings). The page you are on is lifted onto its own white card with a hairline and an indigo icon (`.nav-item`, `.nav-item-on`); the list scrolls on its own while the Rabbit card stays pinned to the bottom of the panel.

One trap removed while doing this: `.enterprise-shell`, the app's root class, carried a whole Reattend-era palette of its own (warm cream surfaces, brand-green primary) that beat `:root` for everything inside the window, so the buttons and charts stayed green no matter what the root tokens said. Those two blocks are gone and `:root` is now the only place either theme is defined. The chart palette in `components/charts.tsx` follows the same hues as lane.so.

Today itself follows the mockup Partha drew: a small date line over a large title, three tinted headline cards (memories, open commitments, captured) each with an icon chip, and a sparkline on the one figure that has a real series behind it. The why sits in a violet card with a Write button, the three things in a white card with numbered chips, a priority pill, a progress ring and a footer strip that asks why those three. The right column carries time by app with a letter chip and bar per app, a donut with the 30-day total in the middle, a 14-day bar chart with its average, and a week-on-week note. The page closes on a privacy line.

Explore is the report: the circle at the top, then time captured per day and memories per day as charts with a scale and dated ticks, three lists for people, organisations and projects, and a kinds donut with a chip per kind carrying its count and share. It closes on a line of totals.

In Settings the engine section is called **Rabbit** and describes it as our own small language model; the two builds are chosen by name (Rabbit S for an 8 GB Mac, Rabbit M for 16 GB and up) with the file itself behind an advanced field. The file names appear in exactly one place in the app, `RABBIT_S`/`RABBIT_M` in `settings.tsx`, because the field needs an exact value; the licence notice in About keeps its attributions, as the licences require. The retention control now says plainly that it prunes only the verbatim screen text and never the memories, facts, tasks, transcripts or notes made from it.

macOS gives an accessory app no full-screen behaviour by default, which is why the green button only zoomed; `setup` now adds `FullScreenPrimary` to the main window's collection behaviour.

Integrations and Settings follow the same card language. Integrations is a two-column grid of panels, each with its own icon tile, and the Apple rows carry the real Calendar, Contacts and Mail icons through `SourceIcon`. Settings gained a masthead, a sticky section rail built from the cards actually on the page (each `<Card>` carries a `settings-<slug>` id and an observer tracks which one you are in), and a status card at the top reporting whether Lane is capturing plus four figures: memories, watched folders, what audio it listens to and whether Contacts is on.

The sidebar carries identity at the foot, as a row with the account's name from macOS, the pause control and a menu, while the Rabbit card sits above it.

A walk down memory lane: the play button on the board picks up to 28 memories spread evenly across the whole history, preferring the best-connected one in each slice, then flies the camera from one to the next in the order they happened. The window goes properly full screen, the chrome steps out, each stop lights with its neighbours while the rest dims, and a caption names the kind, the title and the day. Space pauses, Escape leaves. The button beside it does the same walk and saves it: the system recorder captures the screen for the length of the run and drops a `.mov` in `<data>/walks/`, which needs Screen Recording the first time and, like everything else, never leaves the Mac.

The Board is the constellation: the one dark room in a light app. Memories are glowing bodies on a drifting star field, sized by how connected they are and tinted by kind, joined by faint threads, with a census-style label chip that appears on the brightest few and on whatever you touch or search for (`labelMode` widens as you zoom in). Everything the Miro-style canvas already did still works — select, add a memory, connect, blast, undo and redo, tidy, search, the type legend, zoom, the minimap and the drawer — and dragging a body still saves your own layout. It takes the whole window: on this page the shell drops its rail and header, and a back chevron in the brand block returns you to the app. The skin is the last block of `board.css`, scoped to `.rb-root`, so it overrides the light palette in both themes without leaking.

The sidebar folds. The toggle sits beside the wordmark, the choice is remembered in `localStorage` under `lane.sidebar`, and folded it becomes a 68px rail of icons with the rabbit reduced to its mark.

Files opens on a masthead with the folder mark, the promise that files are read here and never uploaded, and a wide search over names and contents. Under it a row of family chips (Recent, PDFs, Docs, Images, Code) and a sort by modified date, name or size; each row carries a colour-coded type badge, the name, the folder it lives in, a snippet, the date and the size, with open and reveal appearing on hover. The rail holds the indexed and pending counts, the watched folders with rescan, files by type, and a tip. The per-type counts are real: `FileStats` now carries `by_ext`, a grouped count over the whole index rather than a tally of whatever happened to be on screen.

Memories is three columns: the filter rail (views with icons, Show, and Kind with a count per kind), the list, and the memory in full. Each card leads with its kind, the source's own icon and name, the time and how long it took, then a confidence pill and a chevron that opens the rest — the remaining entities, dates, numbers, facts and the keep/ignore, pin and edit controls. Selecting a card rings it and opens the right panel: the same header, the title with its confidence, the full summary, Edit / Pin / Add to Board, entities and topics as chips, and what was on screen.

Source icons come from this Mac only (`src-tauri/src/icons.rs`, command `source_icon`). First it looks the page's domain up in the browser's own favicon cache (Chrome, Brave, Edge and Arc are checked; the file is copied before reading because the browser holds a lock), which is why a ChatGPT or Claude memory shows that site's mark rather than Chrome's. Failing that it finds the application bundle by name, reads `CFBundleIconFile` with `plutil` and renders the `.icns` down to a 64px PNG with `sips`; newer system apps keep their icon in an asset catalog with no `.icns` to convert, so those fall through to a Quick Look thumbnail of the bundle. Both are cached under `<data>/icons/`. Nothing is fetched over the network, so an unvisited site simply falls back to the app icon and then to a coloured letter (`components/source-icon.tsx`).

Ask follows the same mockup: a chat column with a New chat button, a Recent list where each conversation carries an icon chip, its title and how many questions it holds, and beside it the conversation pane with a large title, an Answer / Write a draft pill, an empty state that offers four real questions as chips, and a tall rounded composer with the send key as the one accent control. In the sidebar Ask is deliberately a button rather than another row, because it is the thing you come to the app to do.

Every figure on Today is derived, never invented: the week-on-week delta compares the last seven days of `memoriesPerDay` with the seven before and is hidden when there is not enough history, the average comes from the same series, and the High/Medium/Low chip is read off the reason string the ranking already wrote, so the pill and the line beneath it always agree.

Tokens moved with it: `--app` is the ground behind the panels, `--primary` is the same indigo as lane.so (`#5a51e5`), `--radius` is 12px with panels at 18px, and `--ok` / `--warn` are the semantic dots. Dark mode keeps the same structure on a near-black ground. Inner blocks (stat tiles, panes, the three things, the briefing) sit on `bg-secondary/60` rather than bordered white, so cards read as surfaces inside a panel instead of boxes on a page. The default window is 1280×840, minimum 900×600, because the sidebar takes 252px.

## Principles

- **Local only.** No network access anywhere in the app. The Tauri capabilities grant core IPC only: no HTTP, shell or filesystem plugins.
- **Text, not pixels.** Capture reads text apps already expose to assistive technology. No screenshots, no keystrokes, and never password fields.
- **The user owns their memory.** Pause any time, exclude apps and sites, delete one memory or everything.
- **Cheap enough to forget it's running.** Target under 3% average CPU. The first smoke test measured ~0 at steady state.

## Install and run

**Use the installed app day to day** (so macOS lists it as "Lane"):

```bash
npm install
# one-time: put the llama.cpp macOS arm64 release (llama-server + dylibs) in src-tauri/llama/
#           and a static whisper-cli in src-tauri/whisper/ (see "Packaging" below)
npm run build:dmg      # builds the Swift helpers, the app, signs everything, writes Lane-<version>.dmg
cp -R src-tauri/target/release/bundle/macos/Lane.app /Applications/
open /Applications/Lane.app
```

**Packaging.** Everything a user needs is inside `Lane.app` (about 28 MB): the language-model runtime (`Resources/llama/`), the speech engine (`Resources/whisper/whisper-cli`, built from whisper.cpp with `-DBUILD_SHARED_LIBS=OFF -DGGML_METAL_EMBED_LIBRARY=ON` so it is one static binary), and the Swift helpers for audio (`lane-audio`) and calendar (`lane-calendar`). Only the models download on first use, from fixed URLs: the memory model by RAM tier (2.5 GB on 8 GB Macs), the search index model (~140 MB), and the speech model (`ggml-small` 487 MB; `large-v3-turbo` q5 574 MB on 16 GB+). `scripts/sign-runtime.sh` signs the helpers and the app (ad-hoc until the Developer ID step), `scripts/make-dmg.sh` wraps it in a drag-to-Applications image. Windows build and Developer ID signing/notarization are the last steps before release.

**Development:** `npm run app` (Vite on :1420 + native app with hot reload). Tests: `cd src-tauri && cargo test`.

Only one copy runs at a time. Opening a second copy (for example the dev build while the installed app runs) focuses the first one instead.

## Permissions

On first launch a setup screen walks through everything. It can be reopened from Settings → Permissions → Run setup again.

| Permission | Needed? | Why |
|---|---|---|
| **Accessibility** | Required for text | Reads window titles and on-screen text that apps expose to assistive tools. Without it, only app names are recorded. |
| Login Items | Optional | "Start Reattend when I log in" (installed app only). |
| Screen Recording, Microphone, Camera, Full Disk Access, Contacts, Location, network | **Never** | Reattend does not use them. |

macOS does not let any app switch Accessibility on by itself. Setup therefore:
1. Adds Reattend to the Accessibility list (the macOS pop-up) and opens **System Settings → Privacy & Security → Accessibility**.
2. Tells the user exactly which name to switch on.
3. Detects the switch within a second, including the "trusted but needs restart" state, and offers **Restart Reattend**.
4. Covers the failure cases: not in the list (+ button), switch on but not detected (toggle, or remove with −; the installed app can remove its stale entry itself via `tccutil`), and managed Macs.

**Development builds** inherit the permission of the app that launched them (VS Code, Terminal, iTerm). Setup detects this and names that app instead of "Reattend".

**Always skipped:** any Reattend window, password managers, login/Touch ID/password prompts, lock screen, screenshot overlay, Dock, Spotlight, Control Center. Private messaging is skipped by default and email is optional. Users can add apps and web addresses.

## How it works

```
every N seconds (default 5)
  ├─ paused or idle (no input for 5 min)?  → close current activity
  ├─ frontmost app            AX focused app, else window server (no permission needed)
  ├─ cheap look               window title + URL only (no tree walk)
  ├─ privacy                  excluded apps / URL patterns dropped
  ├─ sessionizer
  │    same app + window + URL → extend the activity (a missing URL is "unknown", not "different")
  │    back within 90 s        → resume the earlier activity
  │    otherwise               → start a new one; <3 s with no text is discarded
  └─ only when a snapshot is due (new activity, or 30 s since the last one):
       text read              Accessibility tree walk, bounded (4k nodes, 400 ms, 30k chars)
                              web pages: only inside the page area, skipping nav/banner/footer landmarks
                              tables: one line per row · title-only apps (Finder, Settings, editors, terminals)
       cleanup                icon glyphs, fragments and counters dropped; short lines seen ≥2× before
                              from the same site/app dropped as chrome; card numbers redacted
       store                  clean text (searched, shown) + raw text (kept for re-cleaning)
```

**Memory engine** (`src-tauri/src/engine.rs`): once an activity has been closed for 2 minutes, its cleaned text (≤4,000 chars) goes to the on-device model with a fixed JSON schema → keep/ignore, kind, title, summary, people, organizations, dates, numbers, confidence. **Every extracted string must occur literally in the source or it is dropped** (and counted). One memory per activity; reprocessing keeps the user's thumbs verdict.

**Bundled runtime** (`src-tauri/src/runtime.rs`, `src-tauri/llama/`): a llama.cpp `llama-server` (MIT, build b11026, ~24 MB with its Metal libraries) ships inside the app under `Contents/Resources/llama/` and is started on demand on a free `127.0.0.1` port. The model file is chosen by RAM and downloaded once into the app data folder, resumable:

| Tier | File | Size | RAM |
|---|---|---|---|
| Rabbit S | Qwen3-4B-Instruct-2507 Q4_K_M | 2.5 GB | 8 GB |
| Rabbit M | Qwen3-8B Q4_K_M (thinking off) | 5.0 GB | 16 GB+ |

Users install nothing else. **Network use, in full:** that one model download from a fixed URL, and localhost. In development, if `src-tauri/llama/` is absent, an Ollama tag in Settings (e.g. `qwen3:4b`) uses a local Ollama instead. `scripts/sign-runtime.sh` signs the runtime after `tauri build` (the runtime is a separate process signed without the hardened runtime so it can load its own dylibs); `npm run build:app` does both.

**Search and Ask:** every memory gets a vector from nomic-embed-text-v1.5 (Q8, 146 MB, downloaded once, served by a second bundled `llama-server` on the CPU). Search fuses keyword (FTS5) and vector ranks with reciprocal rank fusion, then groups sessions. Ask retrieves the top 8 memory groups, hands the model their titles, summaries, entities and a 700-character excerpt each, and streams a plain-text answer that must cite `[n]`; citations outside the range are removed and only cited memories are shown as sources. The memory engine yields the model while a question is being answered.

**Recall overlay:** ⌥ Space (or ⌃ ⌥ Space if taken) opens a floating panel on any Space: typing searches memories instantly, Enter asks. The window is content-protected, so it is invisible in screen shares and recordings, and Reattend never captures its own windows. Esc or clicking away hides it. Ask keeps the last three turns of a conversation and, for short follow-ups, retrieves with the previous question added; the prompt carries the current time, each memory's relative time, and the Mac account's name so "you" means the user.

**Entities, tasks, board:** people, organisations and projects are nodes with identity (`entities`, case- and punctuation-insensitive), linked to the memories that mention them. Commitments found in text become `tasks` (open / done / dismissed), only from memories the model kept and the user hasn't thumbed down; they survive reprocessing with their status. The Board draws the entity graph (edge = appear in the same memory), sized by mentions, with a threshold slider; clicking a node lists its memories. When the model's output schema changes, `MEMORY_SCHEMA_VERSION` bumps and existing memories are remade in the background.

**File index** (`src-tauri/src/files.rs`): every document in Desktop, Documents and Downloads (folders configurable) is read with the Mac's own Spotlight importers (`mdimport -t`: PDF, Word, PowerPoint, Excel, Pages, Keynote, Numbers…) with `textutil` and plain reads as fallbacks, split into ~1,200-character sections, indexed for keyword and vector search, and watched for changes (FSEvents) with a rescan every 30 minutes. Files: 25 MB max, first 60K characters. The Files tab searches by name or contents and opens the file or reveals it in Finder; the overlay shows document hits; Ask cites documents as sources alongside memories. Reading a folder makes macOS ask once for that folder (Files & Folders permission), which setup explains.

**Meetings** (`src-tauri/src/meetings.rs`, `src-tauri/audio/lane-audio.swift`): a small Swift helper inside the app records two streams while the user has pressed Record: the Mac's audio output through a Core Audio process tap (macOS 14.2+, needs only *System Audio Recording*, not Screen Recording) and the microphone (AVAudioEngine), each as 16 kHz mono WAV. On Stop, whisper.cpp transcribes both on this Mac (`ggml-small`, 487 MB, downloaded on first use; Metal), the streams are merged by time as **You** / **Others**, silence fillers whisper invents are dropped, the transcript becomes snapshots of a "Meeting" activity, and the memory engine makes the card, tasks and people from it. Audio is deleted after transcription unless *Keep audio* is on. No bot joins, nothing appears on screen. `whisper-cli` is bundled (`src-tauri/whisper/`, static build, verified transcribing inside the app).

**Encryption and backup** (`src-tauri/src/vault.rs`, `backup.rs`): the database is a SQLCipher vault; its 256-bit key lives in the login Keychain (through the `security` tool, readable only by it). A pre-vault plaintext database is encrypted in place on first launch, kept until the encrypted copy passes `integrity_check` plus FTS checks, then removed. Backups: `lane-<stamp>.rvault` = XChaCha20-Poly1305 over a plaintext export, key = Argon2id(passphrase, salt); daily once a passphrase is set; last 7 kept; folder defaults to iCloud Drive/Reattend when iCloud Drive exists, else ~/Documents/Reattend Backups. Restore (Settings, or the setup screen's "Moving from another Mac?") decrypts, re-encrypts with this Mac's key, swaps the database and restarts. The cloud only ever holds a sealed file. Debugging and the training collector use `cargo test live_export_plain` to write an unencrypted copy.

**Today** (`engine.rs: recap`): one briefing per local day, written from that day's kept memories, open commitments and time by app, cached in `recaps`; generated automatically for yesterday after 06:00 and on demand for any day; cites memories; "Redo" regenerates. Quick questions hand off to Ask.

**Integrations** (`src-tauri/src/integrations.rs`): local-first, each off until turned on, each talking only to the service the user chose with their own key. No relay server (a hosted OAuth broker such as Nango would put a Lane server between the user and their data, which is the one thing Lane promises not to do; Reattend web keeps Nango for that reason, Lane does not).
- *Calendar*: `lane-calendar` reads the Mac's own Calendar (EventKit, one permission prompt). Today shows *Coming up* with a **Prepare** button that asks Lane for everything it knows about the people and topic; a recording started during an event is named after it.
- *Notion*: a personal internal-integration secret, stored in the Keychain. Pages shared with the integration are imported into the file index as `notion://<id>` (searchable, cited by Ask, opened in Notion on click), re-checked every 30 minutes or on *Sync now*; disconnecting removes them. *Send to Notion* on a meeting creates a sub-page under a page the user picks.
- *Obsidian and other Markdown apps*: the Markdown folder (Daily, Weekly, Meetings, Notes) plus the file index; no code.
- *Entity profiles*: opening a person, organisation or project on the Board compiles "What Lane knows" from its memories (cached until the entity is mentioned again).

**Memories you own** (`store.rs: update_memory, set_pinned`): every card can be edited in the user's own words (title, summary, people, organisations, projects, decisions) or pinned. Edited and pinned memories are never remade by a schema change or reprocessing, edits reach the search index through an UPDATE trigger, the vector is re-embedded, and pinned memories rank as if they topped a result list.

**Facts** (`facts` table, `engine.rs: verify_facts`): the model also extracts up to six subject / attribute / value triples per activity ("Vatsalya proposal · budget: ₹35 lakh"). A fact is kept only when its value occurs literally in the source and its subject is mostly from it. Facts carry an as-of date (when they were seen) and are matched by their own FTS index. Two active facts with the same subject and attribute but different values are a **conflict**: shown on the card (amber), and given to Ask as "disagrees with X (as of date)", with the instruction to prefer the newest or the one the user confirmed. Users can correct a value (the model's row is retracted, a user row replaces it and survives reprocessing), remove a wrong one, or add one Rabbit missed. `conflicting_facts` lists every disagreement.

**Connectors** (`src-tauri/src/connectors.rs`): anything becomes memory. One JSON file per source in `~/Library/Application Support/so.lane.app/connectors/` (Settings → Integrations → Open folder writes a README and three examples). Kinds: `http` (fetch a JSON API with `{{secret}}` from the Keychain, pick items with a small path like `$.data[*]`, map fields with `{{templates}}`), `script` (run any program; JSON lines or an array on stdout; `LANE_SINCE`/`LANE_SECRET` in the environment), `mcp` (call one tool on an MCP server over stdio). Each runs on its own schedule between memories, items land in the file index as `connector://<id>/<item>` (searchable, cited, opened at their URL), and with `"memories": true` every new or changed item becomes an activity that Rabbit turns into a memory with facts and tasks, so yesterday's analytics or a CRM change appears in the briefing without the user opening anything. Verified end to end: a script connector's item was indexed and became the memory "Website traffic on 17 September" with its numbers, retrievable within a minute.

**MCP server** (`src-tauri/src/mcp.rs`, crate `src-tauri/lane-mcp`, bundled at `Resources/mcp/lane-mcp`): Claude Desktop, Cursor and any MCP client can use Lane as a tool: `search_memories`, `facts`, `open_tasks`, `search_documents`, `who_is`, `recent`. Read-only, stdio, opens the vault with the key from the user's Keychain. Settings shows the config to paste. Verified against the real vault: facts and memories come back with dates and conflicts.

**Checkably right** (`engine.rs: unverified_figures`): every number, amount and date in an answer is checked against the context the model was given; the Ask page shows "✓ Every figure and date checked against the sources" or lists the ones not found. **Fact ownership and stance**: each fact carries `owner` (mine / theirs / unknown) and `stance` (stated / proposed / agreed / asked), extracted with the fact and shown to Ask as "yours", "someone else's", "proposed", so "my domains" excludes listings merely browsed and a proposal is not reported as agreed. Schema v6 remakes older memories to add these.

**Insights** (`store.rs: insights`): deterministic observations from the whole memory, woven into the morning briefing and listed on Explore: project time this week versus last, facts that changed this week (old → new), deadlines within three days, people who went quiet.

**Speed**: the answer is delivered before follow-up suggestions are generated (a separate `ask-followups` event), a follow-up reuses the previous turn's memories, and short activities get a smaller extraction budget.

**Retrieval at 100K memories** (`store.rs: QuantIndex`): memory and document vectors live in memory as 8-bit values with a per-vector scale (100K × 768 dims is 77 MB), built on first use, appended on every new vector, rebuilt after deletes, and every hit is checked against the table. A query scans it in tens of milliseconds; the keyword side is FTS5. Nothing is ever "read all memories": candidates come from the two indexes, then the model reads **many compact lines and a few excerpts**, tiered by intent (`engine.rs: answer_with`): gathering questions (list, timeline, synthesis, actions) get three times the candidates, one line each with their facts, and full excerpts for the top three; pinpoint questions get fewer candidates with excerpts for the top five. `--cache-reuse` keeps the system prompt's KV cache between requests.

**Conversations are kept** (`conversations`, `messages`): every question in Ask lands in a chat with a title, the answer and its sources are saved when it completes, the Ask page has a Recent pane to reopen, continue or delete chats, and follow-ups use the saved turns. The overlay and Today do not persist.

**Ask, intent-aware** (`engine.rs: classify, date_range, hop_query`): each question is classified instantly (factual, temporal, list, person, timeline, synthesis, actions) and gets an answer-style instruction to match; time phrases ("yesterday", "last week", "in August", "on Tuesday", "last 3 days") become a window that retrieval respects, and a windowed question with few keyword hits falls back to that window's memories, so "what did I do yesterday" works. A second retrieval hop follows the names the first hop surfaced. First names expand to the one known full name ("Sarah" → "Sarah Khan"), and the Board lets the user add aliases ("Also known as").

**Today additions**: *Dates in your memories* (dates written in memories and facts that fall in the next 30 days, parsed as written), *Worth a note* (people dealt with three or more times this week with no task, decision or fact recorded; calendar meetings that left no memory), and Day / Week / Month spans with a monthly roll-up generated automatically from the 3rd of the month.

**Explore**: memories and captured time per day for 30 days, kinds, top people / organisations / projects (click to ask), facts and disagreements, tasks, documents, meetings, pinned and edited counts.

**Images**: screenshots, photos and scans in indexed folders are read with the Mac's own Vision OCR (`lane-ocr` helper), so a screenshot of a quote or a whiteboard is searchable and citable.

**Dropped bin**: Memories → Dropped shows what Rabbit judged not worth keeping; a thumbs-up rescues it (and becomes a training label).

**Robust extraction**: the JSON grammar caps every array and string (no runaway lists), the output budget is 1,200 tokens, and a reply that still gets cut short is repaired (open strings, arrays and objects closed; the unfinished field dropped) instead of failing the activity. Server startup is serialised so a question arriving while the engine starts cannot launch a second runtime; transient server errors restart the runtime and leave the activity pending.

**Two model slots:** `llama-server` runs with `--parallel 2` and an 8-bit KV cache (8192 tokens total, ~0.6 GB), so a question, briefing or profile starts at once (first token 0.3 s measured) while a memory is being made in the background, instead of queueing behind it. Server RSS on the 8 GB tier: ~3.0 GB.

**Ask follow-ups:** after each answer a small structured call proposes three next questions (chips). Context size scales with RAM (6 memories/500-char excerpts on 8 GB; 10/800 on 16 GB+).

**Live meeting notes:** every 30 s while recording, new audio from each stream is cut into a self-contained WAV and transcribed; the merged You/Others transcript shows in the Meetings tab. The final transcript is still made from the whole recording at Stop.

**Help Rabbit learn** (`engine.rs: send_labels`, `infra/labels-worker/`): off by default. When on, thumbed memories (verdict, model output, the screen text judged) are posted every 6 h to `labels.lane.so/v1/labels` with the tester code; a review list allows excluding items ("never send"). The Cloudflare Worker stores each upload as JSONL in R2, keyed by tester. Not deployed yet.

**Ask sources:** memories (including meetings), documents, open tasks, and entity briefs (everything known about a person/organisation/project) each get their own `[n]`.

**Meeting sharing:** *Copy summary* (title, summary, people, figures, action items, "Prepared with Reattend" footer) and *Save with transcript* (Markdown in Downloads). The user sends it; nothing is uploaded.

**Board:** entities plus memory nodes attached to what they mention; time range (today / 7 / 30 days / all); kind filters; "seen ≥ N×"; search highlight; drag pins positions (kept in `board_positions`); right-click or Focus shows a node's neighbourhood; click opens memories.

**Training labels:** thumbs on memory cards are stored as labels, not applied to the model on the device. Settings → *Export my thumbs as training labels* writes `labels.jsonl` (source text, model output, human verdict) for the fine-tuning pipeline in the `rabbit` repo. A new model version reaches users as a file update.

**Measured on 117 real snapshots (Chrome, Gmail, Claude, VS Code, Terminal…):** cleanup made the stored text **50% smaller** (461K → 230K characters) and removed 23 snapshots that had no content. Re-run any time with `RAT_DB=copy.db cargo test live_reclean -- --ignored --nocapture`.

| Path | What |
|---|---|
| `src-tauri/src/capture/macos.rs` | Accessibility + CoreGraphics bindings, tree walk |
| `src-tauri/src/capture/mod.rs` | Capture loop and sessionizer (tested) |
| `src-tauri/src/clean.rs` | Text cleanup rules and title-only app list (tested) |
| `src-tauri/src/store.rs` | SQLite + FTS5 store, boilerplate statistics, re-clean migration (tested) |
| `src-tauri/src/privacy.rs` | Exclusions, presets and redaction (tested) |
| `src-tauri/src/permissions.rs` | Permission report, System Settings deep links, launch at login |
| `src-tauri/src/engine.rs` | Memory engine: backend selection, model call, schema, source verification (tested) |
| `src-tauri/src/runtime.rs` | Bundled llama.cpp: tiers, resumable download, chat + embedding servers, streaming |
| `src-tauri/src/diag.rs` | Log file (`lane.log` in app data) and panic hook |
| `src-tauri/src/commands.rs` | IPC commands used by the UI |
| `src-tauri/src/lib.rs` | App setup, menu-bar tray, window behaviour |
| `src-tauri/src/files.rs` | Document crawl, text extraction (Spotlight importers), chunking, folder watch |
| `src-tauri/src/meetings.rs` + `audio/` | Meeting recording helper, whisper transcription, You/Others merge |
| `src-tauri/src/vault.rs`, `backup.rs` | SQLCipher key in Keychain; passphrase-sealed backups and restore |
| `src/` | React UI: setup, Today, Ask, Memories, Tasks, Board, Files, Meetings, Timeline, Search, Settings; `overlay.tsx` is the recall panel |
| `infra/labels-worker/` | Cloudflare Worker + R2 for opted-in label uploads (to deploy) |

**Data model:** `activities` (one session of attention) → `snapshots` (distinct text states). Both have FTS5 indexes with rowid-correct triggers. Search requires every word to match, falls back to any word, and weights title/URL matches above body text.

**UI:** the primitives in `src/components/ui`, `src/lib/utils.ts`, `src/styles/globals.css` and `tailwind.config.ts` were copied from the enterprise web app, which is untouched. The app shell uses the `.enterprise-shell` palette.

## Layout (UX pass, 2026-09-19)

Sidebar: **Today, Ask, Memories, Meetings, Files, People**, then **Board** and **Integrations** below a divider. The top bar holds the search field (⌘K, searches memories), **Capture** (⌘N: note, voice note, record a meeting, in a dialog), the recording indicator, the capture and memory status dot, Pause, and a profile menu (Settings, Integrations, Explore, Help Rabbit learn, About, Quit). Tasks, Timeline and Everything (raw text search) are views inside Memories; the tray and deep links still name the old pages and are routed there.

Three panes where it pays: **Today** is coming up / dates in memories / worth a note on the left, the briefing in the middle, and the shape of the day on the right (memories and open tasks, time by app, kinds as a donut, memories per day, open commitments with a done checkbox). **Memories** is views and filters (kept / dropped / all, kind counts) on the left, cards in the middle, and the selected memory on the right (summary, facts with disagreements, people, the captured text, edit and pin). **People** is the directory: kind tabs and search on the left, the compiled profile, aliases, quick questions, Add to Board and Forget in the middle, that person's memories on the right. **Integrations** is its own page (Calendar and reminders, Contacts, Apple Mail, Notion, connectors, MCP, Obsidian). Charts are dependency-free SVG in `components/charts.tsx` (Donut, Bars, HBars, Stat).

**Board** (`pages/landscape/`, ported from the Reattend enterprise "landscape" board and rewired to Lane): an Obsidian-style brain of memory dots on React Flow. Dots are coloured by type (decision, meeting, insight, idea, note, context) and sized by how connected they are; a deterministic wedge layout (`brainLayout`) groups colours into lobes. Tools rail: Select (V), Add memory (N or double-click: a sticky composer that saves a note, which Rabbit files into a memory), Connect (C, click two dots or drag from a dot), Undo / Redo (⌘Z / ⌘⇧Z), Tidy up, Blast. Connecting opens the relation picker (11 relations: related to, supports, contradicts, leads to, causes, depends on, blocks, part of, continues, same topic, same people); user links are solid with a label, links Lane infers from shared people or topics are dashed and marked auto, and naming one makes it yours. Click a dot for the drawer: summary, facts, decisions, what was on screen, people, topics, connections, Connect / Ask about it / Open. Search (/) with ↑↓, clickable type legend, minimap, label level-of-detail with zoom. Positions persist per board; links live in `board_links` (label = relation kind). "Add to Board" from Memories and People places a memory on it.

## Clipboard, report a problem, testers

**Clipboard** (Settings → Capture → "Remember what I copy", off by default; `clipboard.rs`): the pasteboard is compared every 3 s; new text of 20+ characters joins a rolling "Clipboard" activity (10-minute sessions) and becomes memory. Anything that looks like a password or key (one run of mixed characters, known token prefixes) is never stored; copies made while an excluded app or site is in front are skipped; contact details are redacted like everything else.

**Report a problem** (Settings → About): writes `~/Desktop/Lane-diagnostics-<stamp>.txt` (version, macOS, RAM, settings without tokens, engine state, counts, permissions, log tails) and shows it in Finder. Nothing is sent; the user attaches it if they want.

**Tester kit** (`infra/labels-worker/`): the label intake worker accepts `Authorization: Bearer <tester code>` and checks the code in the TESTERS KV namespace. To deploy on the Lane Cloudflare account:

```bash
cd infra/labels-worker
npx wrangler login
npx wrangler r2 bucket create lane-labels
npx wrangler kv namespace create TESTERS        # paste the id into wrangler.toml
python3 codes.py 100 --kv <namespace-id> > codes.txt 2> load-codes.sh && sh load-codes.sh
npx wrangler deploy                              # serves labels.lane.so (DNS on lane.so)
```

Each tester gets one line of `codes.txt`; they paste it under Settings → Help Rabbit learn → Tester code and turn on "Contribute my thumbs". Uploads land in R2 as JSONL per tester code, so one tester's data can be dropped later.

## Notch, live meeting help, thought check, dictation

**What gets indexed** (`files.rs`): documents the Mac's own importers read (PDF, Word, PowerPoint, Excel, Pages, Keynote, Numbers, OpenDocument), images through Vision OCR, and now everything that is already text — notes and prose (md, rst, adoc, org, tex, csv), the web (html, css, svg), code in about thirty languages, what a project says about itself (json, yaml, toml, ini, xml, plist, gradle, Dockerfile, Makefile), subtitle files (srt, vtt, sbv, which is as close as Lane gets to remembering a video), and patches. Guards come with it: build and dependency folders are skipped (node_modules, target, Pods, DerivedData, .venv, vendor and the rest), anything whose name smells of a secret or of machine noise is passed over (id_rsa, credentials, *.pem, lock files, minified bundles, source maps), and plain text is capped at 3 MB a file.

**Capture my screen** (`commands::capture_now`, menu bar only): one deliberate press writes down whatever is in front, now. Unlike the background pass it ignores the excluded lists and the noise rules — the person asked, so the activity is created, stretched past the alt-tab floor, given the window's text (or its title and address when there is none) and Rabbit is woken to make the memory at once. Three things make it work where the background pass would not: it looks past Lane's own windows (`observe_front_other`), because pressing a menu bar item makes Lane the front app; it always writes a content-bearing snapshot, since an activity with no snapshot is never turned into a memory and that is how an asked-for capture went missing; and it marks the activity `urgent`, so `next_pending_activity` takes it ahead of the two-minute settling queue. Times stay true — the activity starts and ends when you pressed it.

**Recall overlay** (`src/overlay.tsx`): a pane of glass in Spotlight's register — 44px blur at 200% saturation over a translucent ground, a highlight along the top edge, light pooling faintly in two corners, and everything inside (chips, rows, icon tiles) floating on it with its own inner hairline. Both themes are painted; the dark one swaps the ground rather than inverting — one large line to type in, a row of group chips (everything, memories, people, owed, files) that filter as you click them, and results as rounded rows carrying the source's own icon, a title, a subtitle and a right-hand figure. It is content-protected, so it never appears in a screen share and cannot be screenshotted, even by us.

**Notch tab** (`src/notch.tsx`, window `notch`): open, it is a glass panel — a dark blue-violet wash over the blurred desktop, a hairline of light along the top edge, and colour in the corners. It carries a header with the mark and a way into the app, one line to ask, three actions (record, dictate, open), and three panels: recent memories, today's captured time as a ring with its top apps and a streak, and four quick actions that each open a real screen or ask a real question. Everything in it is live. The window is cleared explicitly on macOS (`setOpaque(false)` and a clear background colour), because a transparent Tauri window still gets an opaque backing and that showed as white corners on a dark desktop. The menu bar carries a **Show the notch** switch that follows the same setting as Settings → Capture → Notch.

**The tab itself**: a 180×14-pt tab under the menu bar, centred on the notch, always on top, on every Space, hidden from screen sharing. Its dot shows what Lane is doing (green remembering, red recording, blue dictating, grey idle/paused). Hover grows it to a 500×250 card with three tiles (Next, Owed, From Lane) and a toolbar; leaving the card collapses it after a short grace so the resize does not flicker (`notch_resize`, re-centred by `place_notch`): current activity and today's count, the latest word from Lane (reminder / brief / meeting help / thought check), the next calendar event, the first open commitments with a tick to close them, and buttons Record/Stop, Dictate/Insert, Ask (⌥Space overlay), Open. Reminders, briefs and help open the card by themselves for 15–25 s; a dictation keeps it open until it ends. Settings → Capture → Notch. The window level is set through AppKit (`lib.rs: raise_window`, objc2-app-kit): the notch at the status level (25) so it sits over the menu bar and above every app, the overlay at the floating level; both get CanJoinAllSpaces + FullScreenAuxiliary + Stationary + IgnoresCycle and are re-ordered (`orderFrontRegardless`), and the notch is pinned with `setFrameOrigin` in AppKit coordinates (every Tauri placement is constrained below the menu bar). Measured with plain AppKit test windows on 2026-09-20 (full screen verified by screenshot): none of this is enough unless the process is an Accessory app **when the window is created**; a window created while the app is Regular never joins other apps' full-screen Spaces, whatever is set or re-ordered afterwards, and switching the policy before showing does not help either. Tauri creates its windows before `setup`, so the policy has to come from the bundle: `src-tauri/Info.plist` sets `LSUIElement` true (it had been false), `setup` also sets Accessory, and `show_main` no longer switches to Regular. Lane is a menu-bar app: no Dock icon; the main window opens from the tray, the notch, ⌥Space or a second launch. `show_main` also activates the app in AppKit (`activateIgnoringOtherApps` + `makeKeyAndOrderFront`): from another app's full-screen Space, Tauri's show + focus alone left the window ordered on the desktop Space with the person still looking at the full-screen app (2026-09-20); with activation macOS switches to the Space holding the window. Even then a Tauri `NSWindow` was never drawn over another app's full-screen Space (verified with an unprotected build and screenshots, while plain AppKit NSWindows with identical settings were), so `raise_window_now` re-classes the notch and the overlay in place as `LanePanel` (`object_setClass`, the tauri-nspanel technique): a runtime-registered NSPanel subclass (`lane_panel_class`, objc2 ClassBuilder) whose `canBecomeKeyWindow` is YES, because a borderless NSPanel cannot take the keyboard and the overlay's field must; `NonactivatingPanel` + floating panel, `becomesKeyOnlyIfNeeded` for the notch only. After that the tab is drawn over full-screen Chrome and ⌥Space types over it. Note System Events reports the *other* app as frontmost even when Lane's main window is in front: accessory apps are never "frontmost" there; judge by screenshot. Test aids: `LANE_UNPROTECTED=1` leaves the notch visible to screenshots and logs a dump of every NSWindow (`windows_dump`) every 20 s; `notch-watch` lines log the notch's state every 20 s always. `window_report` logs level / visible / occluded / activeSpace / behaviour bits whenever the notch is placed or the overlay shown. Centring takes the width the window is about to have (`place_notch_width`) and the monitor's origin, so it no longer drifts when the card opens.

**Meeting notes, attendees, audio** (`engine.rs: meeting_notes`, `meeting_screen_step`): when a recording stops, the transcript is transcribed, then written up on this Mac into notes (summary, key points, decisions, action items with owner and date, open questions) stored in `meetings.notes` and shown on the Meetings page (two panes: recorder + list on the left, the selected meeting on the right; Copy notes / Save with transcript / Notion / Transcript / Play audio / rewrite / delete). While recording, every 45 s the front window is read if it is a meeting app or site (Zoom, Teams, Meet, Webex, FaceTime, Slack huddles, Whereby; the only time excluded meeting apps are read, and only because the user pressed Record): its text goes into the meeting activity as "On screen during the meeting" so the memory names the participants, and lines that look like names become `meetings.attendees` ("seen on screen" chips). Speakers in the transcript are "You" / "Others" unless Speakers apart is on (below). Keep audio (Settings, and a switch on the Meetings page) leaves `mic.wav` / `system.wav` in the recordings folder; Play audio opens your side in the default player.

**Speakers apart** (`diarize.rs`, opt-in Settings → Meetings): the other side's audio is diarised on this Mac with the sherpa-onnx offline speaker-diarization CLI (pyannote segmentation 3.0 + NeMo TitaNet-small embeddings), fetched once into `<data>/diarize/` (≈60 MB, Apple silicon only for now). Each system-side whisper segment takes the voice it overlaps most; voices become "Speaker 1", "Speaker 2"… in the transcript, listed in `meetings.speakers`. On the Meetings page a voice chip opens a name prompt; `rename_speaker` rewrites "] Speaker 2: " to "] Sarah: " in the meeting's snapshots (FTS follows the trigger) and re-embeds. Measured: 57 s of four-speaker audio in 5 s (RTF 0.09). Your own side is always "You" (its own microphone track), so no diarisation is needed there.

**Pictures with memories** (`shots.rs`, `audio/lane-shot.swift`, opt-in Settings → Capture): when a new text snapshot is stored, the helper finds the front app's main window (CGWindowListCopyWindowInfo), captures it with `screencapture -l` (needs Screen Recording; the app is the responsible process for the grant), scales to 1280 px wide and writes a JPEG (~100 KB) to `<data>/shots/<activity>-<ms>.jpg`; `snapshots.image` points at it. Activity views show it above the text (`snapshot_image` returns a data URL, only for files under the shots folder). Deleted with the activity, and pruned with raw text by the retention step. `permissions.screen_recording` reports the grant; `request_screen_recording` asks.

**Prep card** (`meeting_notify_step`): the 5–20 min reminder now carries, per invitee found among your people, "You owe X: …" (open tasks tied to them) and "Last with X (3 days ago): …"; the notification body shows the first two lines and the notch card all of them.

**Morning nudges** (`briefing_notify_step`, `memory_gaps`): the morning brief adds "Past a week, still open: …" (`store.overdue_tasks`) and "Gone quiet: …" (`store.going_quiet`: people with 4+ mentions, nothing in 14–60 days). Today → Worth a note shows the same as gaps of kind `overdue` and `quiet`, each with a Recall question.

**Recall overlay** (⌥Space): hits now include people (`list_entities`), open commitments matching the words, memories and documents; ↑↓ move a cursor and ⌘↩ opens the selected row, Enter still asks.

**Windows**: `.github/workflows/windows.yml` runs `cargo check` and `tauri build --bundles msi` on `windows-latest` (manual or on a `v*` tag). Unix-only calls are gated (`meetings::stop` SIGTERM, the capture platform stub now has `focused_text`/`press_paste`); cross-checking from macOS stops at `ring`'s build script, so the CI job is the check. On Windows the capture layer returns nothing: Ask, Files, notes, Board, connectors and MCP work; capture, meetings, dictation, the notch and Calendar/Mail/Contacts are macOS-only until a Windows platform layer (UIA + WASAPI) exists.

**Commitments, precision over recall** (`engine.rs: looks_like_form_question`, `dismiss_form_questions`): a task is never a question put to the reader (ends with "?", or starts like a form field: "Have you…", "Why…", "Please share…"), nor longer than 140 characters; the extraction prompt says so too, and one sweep at engine start dismisses any such lines already stored. Application forms had been producing dozens of false commitments.

**Self-test drivers** (`scripts/selftest/` is not committed; the session's AppleScript helpers): the app is driven through Accessibility (`entire contents of window "Lane"`, buttons by name, switches by the label's vertical position), the tray through `menu bar 2`, pages through `Lane.app/Contents/MacOS/rat-mac --page <name>` (a second process; the single-instance plugin forwards `--page` and the app emits `navigate`; `open -a` does not pass args to a running app), and floating windows through `window_report` lines in lane.log (level, visible, occluded, activeSpace, behaviour bits) because content-protected windows are invisible to screenshots and to CGWindowList. Verified 2026-09-20: overlay and notch reports, dictation into TextEdit (91 chars), a two-voice synthetic meeting (`say -v Samantha` / `-v Daniel` through the speakers) → 9 turns, Speaker 1 / Speaker 2, notes with summary, key points, decisions and action items. JS `prompt()` / `confirm()` do nothing in the webview: use inline controls.

**Commitments, precision over recall, part two** (`tasks_for_kind`, `kind_carries_commitments`, `first_person_task`): a week of capture had produced 249 open commitments, mostly requests read in articles, posts, product pages and the same email seen twice. Only email, chat, document, meeting, note and voice-note memories may carry tasks; other kinds keep only first-person lines ("I will…", "Remember to…"); five per memory at most; the startup sweep also dismisses repeats (same text, keeps the oldest) and lines from kinds that cannot carry commitments. Today's "open commitments" stat is now a real count (`open_task_count`), not the capped list length.

**Notch closing** (`lib.rs: watch_notch_mouse`, `notch.tsx`): the webview's mouseleave was unreliable once the card had grown under the pointer, so the shell polls `NSEvent.mouseLocation` against the panel frame four times a second and emits `notch-mouse {inside}` on change; the card folds when the pointer leaves. A click on the tab keeps the card open until clicked again; × closes; events (reminder, brief, help) still open it for 15–25 s and a dictation until it ends.

**Calls** (`engine.rs: call_detect_step`, Settings → Meetings → "Listen during calls by itself", off by default): every 20 s the front window is checked against the meeting apps and sites list. With the setting on, recording starts by itself (titled after the calendar event when there is one) and stops two minutes after the call window has gone; live help, notes and the memory follow as for any recording. With it off, the notch shows "In a call?" once per call window (and at most every 15 min) so one click on Record starts it. Nothing is recorded without either the setting or the click.

**Notch position and other notch apps** (`lib.rs: NotchPin`, `running_notch_apps`, `engine.rs: notch_apps_step`, `components/notch-position.tsx`): the tab can sit at the top centre (over the menu bar, where the notch is), top left or top right (below the menu bar, 8 pt in), bottom centre (above the Dock), or the left or right edge (a vertical 14×180 tab, the card opening inward). Placement uses the screen's full frame for the top centre and its visible frame elsewhere. Size and origin are set in the same AppKit step (`raise_window_now`: `setContentSize` then `setFrameOrigin`): Tauri's `set_size` / `set_position` go through `dispatch_async` while the AppKit step goes through the event-loop proxy, so a Tauri call could land after it and drag a side-pinned tab back to the top centre, which is what "rendered sideways at the notch" was. The shell also forces the tab's orientation (180×14 or 14×180) to the spot when no size is given. Chosen in onboarding (step "Notch") and in Settings → Notch with a little screen mock; `notch_position` + `notch_position_chosen`. Once a minute the running apps are checked for names containing "notch" or "Alcove"; if one is found, the position is top centre and the user never chose, the tab moves to the top right, saves that, and a notification says so once. A chosen position is never moved.

**Card contents** (`commands.rs: notch_context`): when the card opens it reads the front window (light observation, same exclusions as capture) and shows in the third tile, in this order of precedence: a streaming answer to the card's own ask line (`api.ask` with persist=false; Esc clears), the latest word from Lane (reminder / brief / help), the live transcript while recording, **the person in front** (a known person's name in the window title → what you owe them and the last thing with them), or **last time here** (the most recent memory about the same page or document from an earlier sitting, matched by the title's first segment and the same app or URL). Otherwise a hint.

**Ask answers what was asked** (`engine.rs: Constraints`, `constraints_of`, `scope_check`, `evals/`): "How much did I spend today?" had been answered with an annual total from a page seen today. Now the question's explicit constraints are spelled out to the model (the time window and its phrase, named people, whether an amount or count is asked) and the system prompt forbids leading with a figure or fact from another period, person or thing. After the streamed answer, for any question with a constraint, a second short JSON pass (`scope_check`, ~300 tokens) judges whether the first sentence respects it given the sources and rewrites the answer if not; the final `ask-done` carries the rewrite (`AskResult.scope_fixed`, shown as "answer re-read against what you asked"). Regression set: `evals/ask_cases.json` (must_contain_any / must_not_lead_with / must_not_contain) run by `evals/run_asks.py`, which sends each question to the running app through `rat-mac --ask "…"`; the app answers off the conversation history and writes `<data>/asks/<ms>.txt`. Add a case for every miss reported. A second cause of short answers was the context window itself: each llama-server slot has 4096 tokens (`-c 8192 --parallel 2`), and wide prompts (16 sources + facts) at 11,000 user chars filled it, so generation stopped after a few words (`stop processing: n_tokens = 4095, truncated = 1` in llama.log). `MAX_PROMPT_CHARS` is 8,500 now and `chat_stream` retries once with a 60 % prompt when the finish reason is `length` after under 200 chars. Run it on a quiet machine: with a release build compiling alongside, generation fell to 2 tokens/s and one answer came back truncated, which looks like a model failure and is not. Retrieval also changed: `retrieval_terms` drops time words from the search query (the window covers them) and adds the words pages use for money questions (spending, paid, invoice, total…), and amount questions rank memories that carry figures first. The checker is conservative: it only acts when the first sentence clearly breaks the window or the person; hedged answers skip it; the person constraint only covers names literally in the question; it is told who is asking so "Partha" is "you". Runs on 2026-09-20: 8/10 → 12/14 → 13/14 → 14/14 on the final build (installed at ~/Applications/Lane.app). Two further causes found on the way: a question's stated answer sitting mid-context was ignored by the 4B model in favour of concrete invoices in the excerpts, so decisive facts (`Constraints.leads`) are repeated in the CONSTRAINTS line right before the question and the checker treats a different first figure as a violation; the `--ask` path now sets `ask_active` so background extraction yields the model as it does for the UI. Two more rules from reading the 14/14 answers: an order marked pending, unpaid or waiting for payment is not money spent ("You spent $12.29 today" for an unpaid order), and a weekday named nowhere in the sources is flagged by `unverified_figures` like an invented figure ("the meeting on Tuesday" for a meeting held today). Cases 15 and 16 cover both. Lessons that became rules: never put a literal example sentence in the system prompt (the 4B model parrots it: "Nothing shows a purchase today" appeared on a question with no time word); a question without a time word is about all memories; the facts index is searched with content words only (question words let every fact match), all of them first and any of them as the fallback; list answers drop lines that disqualify their own item and trailing "Note:" lines (`drop_disqualified_lines`); a fact whose subject and attribute words are all in the question is put at the top as "FACTS THAT ANSWER THE QUESTION DIRECTLY" and, for amount questions over a week/month/year, a fact stating a total for that span as "STATED TOTAL FOR THE SPAN ASKED" (the 4B model otherwise picks single invoices over the stated total even when the fact is in the prompt).

**Three things, the why, the circle** (`src-tauri/src/signals.rs`, `components/three-things.tsx`, `components/golden-circle.tsx`): signal from noise. Every 30 min (and with the morning brief) `signals::refresh` scores candidates from open commitments (due words in the text → `due`, whether the memory was mail/chat/meeting → `waiting`, money in the memory → `stakes`, age → `fresh`), dates written in memories within a week (`date`, `soon`), people who came up often with nothing written down (`person`), entities on three or more distinct days this week (`thread`, `recurrence`, `store.threads`), calendar events in 48 h (`event`, `soon`), and closeness to the person's why (`align`: the why + how embedded once, `store.vector_search`, ≥ 0.4). Score = Σ weight × value with defaults in `default_weights`; the top seven go into the `signals` table for the day with a reason line ("due tomorrow · Sarah is waiting · ₹40 lakh"); rank 1–3 are the three things. States: open, pinned, done (a task signal marks its task done), noise. `learn` nudges the weights (+0.10 × value for pinned or lifted, −0.15 × value for noise, bounded 0.1–6) and saves them in `settings.signal_weights`. The why and how live in `settings.purpose_why` / `purpose_how` (Today's WhyLine, Settings → Your why); saving re-ranks at once. `alignment_day` scores each day (average closeness, memories, near count) into `alignment_days`; three days with ≥ 3 memories and none near earn one notch nudge a week. `store.remember_when` resurfaces, once a day after 11:00, a memory older than a month on a thread alive this week. `signals::connects_to` lists two graph neighbours of the entities named in the front window (notch tile "Connects to"). The thought check also receives the how lines as PRINCIPLES and reports crossings. Today shows the why line and the three things above every day's briefing (they are about today whichever day is open below). Explore shows the circle (`circle` command): why at the centre, how and what rings, projects on the outer ring, the month's memories as dots at a distance set by alignment, daily alignment bars underneath. The morning brief leads with the three things; the evening one says how many moved.

**Live meeting help** (`engine.rs: meeting_help_step`): each new stretch of live transcript (every ~30 s) is read for questions put to the user, commitments, dates and the names or topics on the table; memory is searched for facts and the last memory on each topic; the notch shows "They asked…", "Committed…", "You know: … (as of …)", "Last time on …".

**Thought check** (`thought_check_step`, opt-in, Settings → Capture): every 12 s of typing in a text field (via Accessibility, same exclusions as capture), sentences already written and figures that disagree with the user's own facts are pointed out in the notch, at most once per 90 s. Nothing typed is stored.

**Dictation** (`dictation.rs`): ⌥⇧Space (also in the menu bar and the notch card) records the mic; the second press transcribes with the bundled speech engine and inserts the words into the focused field through the clipboard and ⌘V, restoring the clipboard after. The text is also kept as a **Voice note** activity (`store.create_note(…, "Voice note")`), so it becomes a memory like a typed note; find it in Memories or by asking. The notch shows "Listening" while it runs and "Inserted and remembered" after; `dictation-state` and `dictation-done` events carry the same to the UI.

## Mail and Contacts

**Apple Mail** (Settings → Integrations): a built-in script connector (`scripts/mail.js`, JavaScript for Automation) reads Inbox and Sent of every account through Mail itself, so Lane never needs Full Disk Access; macOS asks once to let Lane control Mail. Messages are indexed (searchable, cited, opened in Mail via `message://` links) every 30 minutes; "Also make memories of mail" runs them through Rabbit. Turning it off removes the indexed messages. Unverified on the author's Mac (no Mail accounts): the script parses and runs.

**Contacts**: the `lane-contacts` helper reads names, nicknames and organisations once a day; a first name or nickname that belongs to exactly one contact becomes an alias of the person Lane already knows by full name. Nothing else is kept.

The onboarding and Settings permission text now distinguishes what Lane never asks for (Screen Recording, Camera, Full Disk Access, Location) from what it asks for only when a feature is turned on (Microphone and System Audio, Calendars, Contacts, Mail automation).

## Ask about what is on screen

⌥Space reads the front window *before* the overlay takes focus (`lib.rs: snapshot_screen`, same exclusions as capture) and shows a chip "About this window: <title>" with three quick actions (Summarise this, What should I do about this?, Draft a reply). Questions go to `engine.rs: answer_about_screen`: the on-screen text is source [S] (6,000 chars on 8 GB, 9,000 on 16 GB), plus the four memories and six facts most related to the question and the window title, so "who is this person" or "what did we agree before" is answered from memory while "summarise this" is answered from the screen. Click the chip to ask memories only. Nothing is stored by the overlay; the text lives in memory until the next ⌥Space.

**Daily briefings as notifications** (Settings → Capture): a morning brief at a chosen time (yesterday's briefing, written if needed) and "your day so far" in the evening (today, written fresh), each once a day with the first line in the notification. **Meeting prep is generated when the reminder fires**, so Prepare answers instantly (prepared answers are kept for two hours and served on the exact question). **Commitments by person**: "what do I owe Sarah" includes open tasks from every memory that mentions her, not only tasks whose text names her.

**Meeting reminders**: with Calendar on, a notification 10–20 minutes before each event (once per event) names who and what and points to Today → Prepare. Toggle under Settings → Integrations → Calendar.

## Signing and release

The app, every helper, the runtime and the disk image are signed with **Developer ID Application: Partha Borthakur (6AKUD88CVN)** with the hardened runtime and `src-tauri/Entitlements.plist` (audio input, Apple Events for Mail, calendars, contacts). Because the identity is stable, the Accessibility grant survives rebuilds; the per-build reset only applied to ad-hoc builds. **Notarisation** needs a one-time `xcrun notarytool store-credentials lane` (Apple ID, app-specific password, team 6AKUD88CVN); after that `npm run release` builds, signs, notarises and staples the DMG and writes the update manifest.

## UI pass (2026-09-20)

The app had been rendering in the browser's default serif because the font custom properties were never defined; they are now (system sans for body, Iowan Old Style / New York for headings, SF Mono for code). Lane green is the only accent in light and dark; dark mode follows the Mac's appearance automatically; radius and scrollbars are consistent. Visual design beyond that is left for Partha's own UI direction.

## Updates, retention, forgetting

**Auto-update** (`tauri-plugin-updater`): the app checks `https://lane.so/updates/darwin-aarch64.json` on launch and from Settings → About. Manifests and packages are signed with the minisign key in `~/.tauri/lane.key` (public key in `tauri.conf.json`); the key must be backed up, without it no update can ever be signed. `npm run release` builds, signs and writes `dist-updates/<version>/` (package, signature, manifest) to upload to lane.so. Only the app version, platform and architecture are sent. Ad-hoc-signed builds cannot be installed by the updater under Gatekeeper, so this goes live with the Developer ID signing step.

**Retention** (`store.prune_raw`, Settings → Your data): raw screen text is deleted after 30 days by default (0 = forever) once its memory exists. Memories, facts, tasks, briefings, meeting transcripts, notes and connector items stay.

**Forget** (`store.forget_term`, Settings → Your data, Board → Forget): a name or word is removed from entities and aliases, list fields, blanked in titles, summaries, decisions, raw text, briefings and indexed documents, its facts and tasks deleted, touched vectors re-embedded. Term-exact and case-insensitive; irreversible.

## Capture sweep (2026-09-19, Partha's Mac)

| App | Result | Notes |
|---|---|---|
| Chrome | Text, URL | up to 26K chars on Gmail; Chromium tree enabled via `AXManualAccessibility` |
| Safari | Text, URL | fixed: Safari nests its web area inside an `AXTabGroup`; the finder now skips only tab strips (a tab group of radio buttons). `AXEnhancedUserInterface` is also requested. 6,464 chars on the Wikipedia main page in 0.4 s |
| WPS Office | Title only | Qt exposes no document text; when the title names an indexed document, the document's own text stands in (`capture/mod.rs: document_name`, `store.file_text_by_name`) |
| Preview, Pages, Word, Acrobat | Same fallback as WPS | via the file index |
| VS Code, Terminal, Finder, System Settings | Title only, by design | asking Electron for its tree switches VS Code into screen-reader mode |
| Mail, Notes, Slack (native) | Unverified | not used on this Mac yet |

Probes: `RAT_PID=<pid> cargo test --lib live_app -- --ignored --nocapture` (text and URL), `live_tree` (role dump), `RAT_MODE=enhanced|manual` to set the WebKit / Chromium flags first.

## Engineering rules (learned the hard way)

Two bugs froze the app on 2026-09-19 and are now caught by tests in `lib.rs: hygiene_tests`, which scan the source on every `cargo test`:

1. **Never lock a mutex twice in one statement.** Rust keeps the first guard alive until the end of the statement, so the second take waits forever (a struct literal with two `lock(&state.settings)` fields deadlocked the settings save). Take one guard, copy what you need, release. `crate::lock` also logs any wait over a second with the waiting thread's name, so a stall is visible in `lane.log`.
2. **No command runs on the main thread** unless it must (window and dialog work, allow-listed in the test). Everything else is `#[tauri::command(async)]`.
3. **Every front-end command has a deadline** (`api.ts: invoke`): 30 s, or 10 min for model calls and restores. A command that does not answer rejects with a visible message instead of leaving a button dead, and unhandled promise rejections surface as toasts.
4. **Helpers are bundles in disguise**: each Swift helper carries an embedded Info.plist (`audio/helper-info.plist`) with the usage descriptions macOS requires, pumps its run loop while waiting for a permission answer, and is killed by Lane after a timeout.
5. **One runtime start at a time** (`START_LOCK`), transient server errors restart the runtime and leave the activity pending, prompts are trimmed and retried on a 400, failed briefings back off 30 minutes.

## Status

Verified on a real Mac (M1, macOS 15.5):
- Capture reads title, URL and page text from Chrome, for example 5,151 characters in 180 ms from a claude.ai window. It also identifies Chrome running from its update clone path.
- Chromium page text needs one `AXManualAccessibility` request per process. The tree builds asynchronously, so text appears from the next poll.
- The frontmost-app lookup does not depend on the system-wide focus query, which returned `kAXErrorCannotComplete` (-25204) in some process contexts.
- The installed `.app` is ad-hoc signed as `com.reattend.mac` with a valid signature, and launches at about 40 MB RSS.
- Ask, end to end on the author's 8 GB M1 over 105 real memories: "What was the budget for the Vatsalya scheme proposal?" → "₹35 lakh to ₹37 lakh inclusive of GST … [1]" citing the right Claude conversation. Indexing 105 memories took 57 s; a cold answer (model load included) 45 s, about 15 s warm.
- 105 unit tests pass (including a regression test for the memory search index migration that broke search on databases from before the `memories_content` view, and for the ranking bug where short "not worth keeping" memories filled the keyword pool and hid kept ones): store, index health, cleanup, memory pipeline, sessionizer (resume, due-time reads), privacy (cards, emails, phones), presets, settings migration, permission report, engine verification.
- Memory engine end to end on the author's 8 GB M1 with qwen3:4b: ~20 s per activity, 21 memories in 7 minutes, 0 invented strings after verification.

**Model benchmark** (`bench/`, 8 real cleaned captures, 8 GB M1, Ollama, fixed JSON schema):

| Model | Valid JSON | Speed | Latency/snapshot | Extracted strings literally in source |
|---|---|---|---|---|
| qwen3:4b (2.5 GB) | 8/8 | 18 tok/s | ~15 s | 94% (84/89) |
| llama3.2:3b (2.0 GB) | 8/8 | 20 tok/s | ~34 s (longer outputs) | 96% (81/84) |

Both extract and summarise well; both are inconsistent about *what is worth keeping* (a 40-minute proposal conversation judged "not worth keeping" by one, a job form judged "email" by the other) and both report ~0.9 confidence for everything. That judgement and calibration is what Rabbit fine-tuning targets; thumbs on memory cards collect the labels.

Not yet verified:
- A brand-new user completing setup with the installed app. Needs a human to flip the switch.
- Text capture in Safari, Notes, Mail, Word, Slack and VS Code. The live probes (`cargo test live_ -- --ignored --nocapture`) need each app's window on the current Space.

## Next

**Milestone 1: capture you'd trust (no AI)**
- [ ] Verify Accessibility text capture across Safari, Chrome, Slack, Notes, Mail, Word, VS Code.
- [ ] Browser extension (Chrome/Safari) for clean page text and URLs.
- [x] Encrypt the database at rest (SQLCipher, key in Keychain); sealed backups + restore.
- [x] Guided permission setup, Settings → Permissions, launch at login, single instance.
- [ ] Developer ID signing + notarization (needs Apple Developer account and full Xcode); then the updater goes live. Signing is done; `scripts/notarize.sh` waits on `xcrun notarytool store-credentials lane`.
- [x] Apple Mail via automation (index, optional memories) and Contacts aliases, both opt-in.
- [x] Ask about what is on screen (overlay chip + quick actions); meeting reminders.
- [x] Auto-update plumbing (signed manifest, Check for updates, install and restart); retention window; forget a name everywhere.
- [x] Email/phone redaction on by default; card numbers always.
- [x] Semantic search: local embedding model, vectors in SQLite, fused with keyword search.
- [ ] Measure: CPU, battery, MB/day over a real week.

**Milestone 2: Rabbit on device**
- [x] First base-model benchmark on real captures (qwen3:4b, llama3.2:3b via Ollama).
- [x] Memory engine: keep/ignore, classify, summarise, extract with source verification; Memories page with thumbs.
- [x] Bundle the runtime (llama.cpp) instead of depending on Ollama; tiered model download by RAM.
- [ ] Measure Rabbit M (8B) on a 16 GB Mac.
- [x] Merge sessions of the same thing into one memory card.
- [x] People, organisations, projects as nodes; tasks; the Board.
- [x] File index: what and where every document is, searchable by contents, cited by Ask.
- [x] Meetings: manual record, on-device transcription, You/Others turns → memory. Verified: mic + whisper end to end (12 s clip transcribed in 5.7 s, correct text). System-audio tap runs but was silent from a terminal (permission untested); to verify from the app.
- [x] Live transcript while recording; Today briefing; follow-up chips; label contribution client + worker.
- [x] Bundle whisper-cli (static build inside the app); larger speech model tier for 16 GB Macs (large-v3-turbo q5, unmeasured).
- [x] Decisions as a memory field; Capture tab (typed and voice notes); Ask draft mode; weekly review; Markdown export; DMG packaging.
- [x] Integrations: Calendar (meeting prep), Notion (import + export), Obsidian via Markdown; entity profiles on the Board.
- [x] Connectors (http / script / mcp → index + memories) and Lane as an MCP server for other local AI tools.
- [x] Reattend ports: intent-aware Ask with date windows, second retrieval hop, aliases, upcoming dates, memory-gap nudges, monthly roll-ups, Explore, dropped bin, image OCR.
- [x] Memories you own: edit, pin, protected from remakes. Facts with as-of dates, conflict detection, user corrections that override the model (schema v5).
- [ ] Deploy the labels worker (Cloudflare account, steps above); tester codes script ready.
- [x] Clipboard source (opt-in), Report a problem (local diagnostics file).
- [x] Tasks and people as Ask sources; meeting summary export with invitation footer.
- [x] Board rework: memory nodes, time range, pins, focus, highlight.
- [ ] Hosted sync service (E2E, same sealed format) and share-as-link; on-prem sync target for organisations.
- [ ] Windows build (capture via UI Automation, DPAPI vault, same runtime) and Developer ID signing + notarization — last, per plan.
- [ ] Distil from Rabbit 32B as teacher. Real captures are used only with consent and never sent to hosted models.
- [ ] Eval set: keep/ignore labels plus "find that thing" questions with gold answers.

**Milestone 3: memory you can talk to**
- [x] Ask over memories, answered by the on-device model with citations back to activities.
- [x] Conversation follow-ups, colleague voice, time-aware answers; first token ~10 s warm (was 15).
- [x] Recall overlay with global hotkey, hidden from screen sharing.
- [ ] Faster still: cache the memory context between follow-up questions.
- [ ] Memory graph (people, projects, documents) and daily recap.
- [ ] Train Rabbit S: pipeline in `rabbit/scripts/rabbit_s/` (collect → teacher → build → finetune → eval). The old 90K corpus was measured unfit (one-line inputs, 31–39% invented entities); data = human thumbs + teacher labels on consented captures + synthetic screen pages. ~$10–20 per version.

**Gate before enterprise:** under 3% CPU, 85%+ of real "find that thing" questions answered in the top 10, 90%+ keep/ignore agreement, and 5–10 daily users for 4 weeks.
