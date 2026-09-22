#!/usr/bin/env python3
"""Build the use-case and feature pages.

Every page takes its header and footer straight out of index.html, so the
chrome can never drift: change the home page and run this again.

    python3 scripts/build-pages.py
"""

from __future__ import annotations

import html
import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parent.parent / "site"
HOME = (ROOT / "index.html").read_text()


# ── the shared chrome, lifted from the home page ──────────────────────────

def block(tag: str, cls: str) -> str:
    m = re.search(rf'<{tag} class="{cls}".*?</{tag}>', HOME, re.S)
    assert m, f"no <{tag} class={cls}> in index.html"
    return m.group(0)


def absolutise(s: str) -> str:
    """A subpage is not at the root, so its links and assets must be."""
    s = s.replace('href="#', 'href="/#')
    s = s.replace('src="logo.png"', 'src="/logo.png"')
    return s


HEADER = absolutise(block("header", "nav"))
FOOTER = absolutise(block("footer", "foot"))

ICON = {
    "eye": '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z"/><circle cx="12" cy="12" r="3"/></svg>',
    "ask": '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 12a8.5 8.5 0 0 1-12.3 7.6L3.5 21l1.4-5.2A8.5 8.5 0 1 1 21 12z"/></svg>',
    "list": '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 6h11M9 12h11M9 18h11"/><path d="M4 6h.01M4 12h.01M4 18h.01"/></svg>',
    "mic": '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5.5 11.5a6.5 6.5 0 0 0 13 0M12 18v3"/></svg>',
    "lock": '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4.5" y="10.5" width="15" height="10.5" rx="2.5"/><path d="M8 10.5V7.8a4 4 0 0 1 8 0v2.7"/></svg>',
    "clock": '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></svg>',
    "file": '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M14 3.5H7A2.5 2.5 0 0 0 4.5 6v12A2.5 2.5 0 0 0 7 20.5h10a2.5 2.5 0 0 0 2.5-2.5V9z"/><path d="M14 3.5V9h5.5"/></svg>',
    "check": '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="4" y="4" width="16" height="16" rx="3.5"/><path d="M8.5 12.2l2.4 2.4 4.8-5"/></svg>',
    "key": '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="8" cy="12" r="4"/><path d="M12 12h9M18 12v3.5M15.5 12v2.5"/></svg>',
    "bar": '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 20V10M10 20V4M16 20v-7M22 20H2"/></svg>',
    "people": '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="9" cy="8" r="3.5"/><path d="M2.5 20a6.5 6.5 0 0 1 13 0"/><path d="M16 5.2a3.5 3.5 0 0 1 0 5.6M17.5 20a6.5 6.5 0 0 0-2.2-4.9"/></svg>',
}


# ── small builders ────────────────────────────────────────────────────────

def facts(items: list[tuple[str, str, str]]) -> str:
    rows = "".join(
        f'<div class="fact">{ICON[i]}<div><b>{html.escape(t)}</b><p>{b}</p></div></div>'
        for i, t, b in items
    )
    return f'<div class="facts rv">{rows}</div>'


def asks(items: list[str]) -> str:
    rows = "".join(f"<li>{ICON['ask']}<span>“{html.escape(q)}”</span></li>" for q in items)
    return f'<ul class="asklist rv">{rows}</ul>'


def steps(items: list[tuple[str, str]]) -> str:
    rows = "".join(
        f'<li><span class="n">{n}</span><div><b>{html.escape(t)}</b><p>{b}</p></div></li>'
        for n, (t, b) in enumerate(items, 1)
    )
    return f'<ol class="steplist rv">{rows}</ol>'


def cards(items: list[tuple[str, str, str, str]]) -> str:
    rows = "".join(
        f'<a class="pcard" href="{u}"><span class="pcard-ic">{ICON[i]}</span>'
        f"<b>{html.escape(t)}</b><p>{html.escape(d)}</p>"
        f'<span class="tlink">Read more <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14M13 6l6 6-6 6"/></svg></span></a>'
        for i, t, d, u in items
    )
    return f'<div class="pcards rv">{rows}</div>'


def sec(label: str, head: str, body: str, cls: str = "sec") -> str:
    h = ""
    if label or head:
        h = (
            '<div class="sec-head rv">'
            + (f'<span class="label on">{html.escape(label)}</span>' if label else "")
            + (f'<h2 class="display">{html.escape(head)}</h2>' if head else "")
            + "</div>"
        )
    return f'<section class="{cls}"><div class="wrap">{h}{body}</div></section>'


CTA = '''  <section class="access" id="access">
    <div class="wrap access-in">
      <div class="cta-txt rv">
        <span class="label">Early access</span>
        <h2 class="display">Two months free. Then $9.</h2>
        <p>Leave your email and we send you the build, the moment your seat opens. The first 200 people on the list can buy Lane once, for $499, and keep it for life.</p>
        <form class="wait" id="wait" novalidate>
          <div class="wait-row">
            <input id="wait-email" name="email" type="email" autocomplete="email" placeholder="you@work.com" aria-label="Your email" required>
            <button class="btn btn-dark" type="submit">Join the waitlist</button>
          </div>
          <p class="wait-msg" id="wait-msg" role="status" aria-live="polite"></p>
        </form>
        <p class="cta-note">macOS 13+ · Apple silicon · 28 MB · no account · cancel in a click</p>
      </div>
      <div class="plans rv">
        <a class="plan is-first" href="/buy/lifetime">
          <span class="plan-top"><b>Lifetime</b><span class="plan-tag">Only 200 spots</span></span>
          <span class="plan-p">$499<i>once</i></span>
          <span class="plan-d">Every update, every version, no renewal, ever.</span>
          <span class="seats" id="seats">
            <span class="seats-bar"><i id="seats-fill" style="width:0%"></i></span>
            <span class="seats-n"><b id="seats-left">200</b> seats left</span>
          </span>
        </a>
        <div class="plan-pair">
          <a class="plan" href="/buy/monthly"><span class="plan-top"><b>Monthly</b></span><span class="plan-p">$9<i>a month</i></span></a>
          <a class="plan" href="/buy/yearly"><span class="plan-top"><b>Yearly</b><span class="plan-tag is-quiet">Two months off</span></span><span class="plan-p">$89<i>a year</i></span></a>
        </div>
      </div>
    </div>
  </section>'''


def page(slug: str, title: str, desc: str, kicker: str, h1: str, lede: str, body: str) -> None:
    doc = f'''<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>{html.escape(title)}</title>
<meta name="description" content="{html.escape(desc)}">
<link rel="canonical" href="https://lane.so/{slug}">
<meta property="og:type" content="website">
<meta property="og:site_name" content="Lane">
<meta property="og:title" content="{html.escape(title)}">
<meta property="og:description" content="{html.escape(desc)}">
<meta property="og:url" content="https://lane.so/{slug}">
<meta property="og:image" content="https://lane.so/og.png">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="{html.escape(title)}">
<meta name="twitter:description" content="{html.escape(desc)}">
<meta name="theme-color" content="#ffffff">
<link rel="icon" href="/favicon.svg">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Inter+Tight:wght@400;500;600;700&family=Inter:wght@400;450;500;600&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
<link rel="stylesheet" href="/styles.css">
</head>
<body>
<a class="skip" href="#main">Skip to content</a>
{HEADER}

<main id="main">
  <section class="subhero">
    <div class="wrap">
      <nav class="crumbs" aria-label="Breadcrumb"><a href="/">Lane</a><span>/</span><span>{html.escape(kicker)}</span></nav>
      <h1 class="display">{h1}</h1>
      <p class="lede">{lede}</p>
      <div class="actions">
        <a class="btn btn-primary btn-lg" href="#access">Get early access</a>
        <a class="btn btn-ghost btn-lg" href="/features">See every feature</a>
      </div>
    </div>
  </section>
{body}
{CTA}
</main>

{FOOTER}
<script src="/app.js"></script>
</body>
</html>
'''
    out = ROOT / slug / "index.html"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(doc)
    return out


# ── who it is for ─────────────────────────────────────────────────────────

AUDIENCES = [
    dict(
        slug="for/founders",
        kicker="For founders",
        title="Lane for founders · Your AI second brain, on your Mac",
        desc="You are the memory of the company. Lane remembers the investor calls, the hiring threads, the pricing decision and the promise you made on Slack, and answers from your own Mac.",
        h1="You are the memory of&nbsp;the&nbsp;company",
        lede="Six contexts before lunch: the investor, the candidate, the customer who is about to churn, the pricing argument, the contract, the bug. Nobody is taking notes for you, and the cost lands two weeks later when someone asks what you decided.",
        problem=("The tax you already pay",
                 "Nothing here is a tooling problem. It is the same day, paid for twice.", [
                     ("clock", "You switch context eleven times an hour", "Every switch drops the thread you were holding. The work survives; the reason you did it does not."),
                     ("people", "The promise lives in a DM", "You said you would send the deck by Friday. It is in a thread you will not open again until Monday."),
                     ("bar", "The decision has no address", "Pricing was settled on a call, refined in a doc, and reversed in a thread. Three months on, nobody can say which one won."),
                 ]),
        does=("What Lane does about it", [
            ("eye", "It watches the work you are already doing",
             "Pages, documents, calls and calendar. No filing, no tagging, no second app to keep up to date. Lane reads what your apps already publish for accessibility and writes the memory itself."),
            ("list", "Three things, ranked against your why",
             "You write one paragraph on what you are building and for whom. Every morning Lane ranks the day against it and gives you three things, each with the reason it made the list."),
            ("check", "Commitments it caught, and closes",
             "“I'll send it Friday” becomes an open commitment with the person waiting on it. When Lane sees you send it, it closes itself."),
            ("lock", "None of it leaves the machine",
             "Your cap table, your term sheet and your candidate feedback stay on your Mac, in an encrypted store, answered by a model that runs in the app."),
        ]),
        ask=["What did I promise the lead investor last week?",
             "Where did we land on pricing, and who pushed back?",
             "What is still open with Sarah?",
             "Summarise everything about the Series A since Monday.",
             "Which candidates have I not replied to?"],
    ),
    dict(
        slug="for/consultants",
        kicker="For consultants",
        title="Lane for consultants · Your AI second brain, on your Mac",
        desc="Reconstructing the week for an invoice is unpaid work. Lane keeps what you actually did for each client, and answers in your own words, from your own Mac.",
        h1="Stop reconstructing the&nbsp;week",
        lede="Friday afternoon, six clients, and a timesheet that is really an act of memory. You scroll back through documents and threads to work out what you did on Tuesday, and you write it down at a discount because you cannot quite prove the rest.",
        problem=("Where the hours go",
                 "The work is billable. Remembering it is not.", [
                     ("clock", "The timesheet is written from memory", "Two days later you round down, because you can only account for what you can recall."),
                     ("people", "Every client thinks they are the only one", "Context switching between four engagements a day means each one gets a version of you that has forgotten the last call."),
                     ("file", "The deliverable is not the record", "What you sent is in a folder. Why you sent it, and what they asked for, is scattered across calls and threads."),
                 ]),
        does=("What Lane does about it", [
            ("eye", "A truthful record of the week, written as you work",
             "Lane keeps what you read, wrote, opened and said, and groups it by the client and the project it belonged to."),
            ("mic", "Calls become notes, with who said what",
             "Record the call. Lane transcribes it on your machine, separates the speakers, and writes the notes, the decisions and the follow-ups."),
            ("ask", "Answers with the receipt attached",
             "Every answer cites the memory it came from, so a status update or an invoice line can be checked rather than trusted."),
            ("lock", "Client confidentiality by architecture",
             "Nothing about any engagement is uploaded, because Lane has no server to upload it to."),
        ]),
        ask=["What did I do for Acme this week?",
             "What did the client ask for on the last call?",
             "Which deliverables are due before Friday?",
             "What changed in the scope since the kickoff?",
             "What have I not billed yet?"],
    ),
    dict(
        slug="for/researchers",
        kicker="For researchers",
        title="Lane for researchers · Your AI second brain, on your Mac",
        desc="You read forty things to write one. Lane remembers every paper, page and figure you passed through, and tells you where each one came from.",
        h1="Where did I read&nbsp;that?",
        lede="Forty tabs, nine PDFs, a note to yourself in a file you cannot name. You remember the figure exactly. You cannot remember the source, and without the source the figure is worthless.",
        problem=("The cost of a citation you lost",
                 "Reading is fast. Finding your way back is not.", [
                     ("file", "The fact and its source come apart", "You copy the number into a draft. The paper it came from stays in a tab that closes when the machine restarts."),
                     ("clock", "Rereading is most of the work", "You reread a paper because searching your own memory is slower than reading it again."),
                     ("ask", "Search needs the keyword you have forgotten", "You cannot grep for a half-remembered idea in a paper whose title you never knew."),
                 ]),
        does=("What Lane does about it", [
            ("eye", "Every page and PDF you passed through",
             "Not bookmarks: what was actually on the screen, with the document it belonged to, indexed by its contents."),
            ("ask", "Ask in the words you actually have",
             "Describe the idea. Lane finds the passage, answers, and cites the document and the moment you were reading it."),
            ("file", "Your files, read and searchable",
             "Papers, notes, code, subtitles and images with text in them, indexed on your machine and cited in the answer."),
            ("lock", "Unpublished work stays unpublished",
             "Your drafts and your data are never sent anywhere, because nothing Lane does needs a network."),
        ]),
        ask=["Where did I read that figure about knowledge attrition?",
             "What did that paper say about the control group?",
             "Which sources have I already cited for this section?",
             "Summarise everything I read on this topic last month.",
             "Find the PDF with the table of results in it."],
    ),
    dict(
        slug="for/engineers",
        kicker="For engineers",
        title="Lane for engineers · Your AI second brain, on your Mac",
        desc="The code is in git. The reason is not. Lane remembers the threads, the incidents and the decisions behind the diff, on your own machine.",
        h1="The code is in git.<br>The reason is&nbsp;not.",
        lede="Six months later the diff is right there and makes no sense. The argument that produced it was in a thread, a call and a doc, and none of them are attached to the line you are staring at.",
        problem=("Why archaeology is part of the job",
                 "Everything about the change is recorded, except why.", [
                     ("file", "The decision is spread across four tools", "A thread, a review comment, a call and a doc. The commit message says “fix”."),
                     ("clock", "Incidents evaporate", "The three hours of the outage are the most information-dense of the quarter, and almost none of it survives the week."),
                     ("ask", "Onboarding is asking someone who remembers", "The context lives in a colleague's head, and the only interface to it is interrupting them."),
                 ]),
        does=("What Lane does about it", [
            ("eye", "It reads what you read",
             "Pull requests, issues, logs, dashboards, docs and the code you opened, kept in order, with when and where."),
            ("ask", "Ask why, not just what",
             "Lane answers from what you actually saw, and cites it, so the answer points back at the thread rather than guessing."),
            ("check", "Decisions kept as decisions",
             "When something is settled, Lane records it as a decision with its date and its reason, so the next argument starts from the last one."),
            ("lock", "Your source never leaves the machine",
             "No code is uploaded to anyone, including us. Rabbit, the model that does the thinking, runs inside the app."),
        ]),
        ask=["Why did we choose Postgres over Mongo?",
             "What broke last Tuesday, and what fixed it?",
             "What did the reviewer object to on that pull request?",
             "What did I change in the auth flow last month?",
             "Which decisions are still open on the migration?"],
    ),
    dict(
        slug="for/sales",
        kicker="For sales teams",
        title="Lane for sales teams · Your AI second brain, on your Mac",
        desc="Lane remembers what you promised, who is waiting and which deals have gone quiet, without a single field to fill in.",
        h1="The follow-up you meant&nbsp;to&nbsp;send",
        lede="Nine calls, four demos, and a promise in each one. The notes go into the system if there is time, which there is not, and the deal goes quiet in a way nobody notices for three weeks.",
        problem=("What the pipeline does not know",
                 "The system of record only knows what somebody typed into it.", [
                     ("mic", "The call was the whole conversation", "The objection, the timeline and the name of the person who actually signs were all said out loud, and then gone."),
                     ("check", "You promised something specific", "A price, a date, a document. Nobody wrote it down, and the prospect certainly remembers."),
                     ("clock", "Silence looks like progress", "A deal with no next step looks exactly like a deal that is going well, right up until it is not."),
                 ]),
        does=("What Lane does about it", [
            ("mic", "Calls recorded, transcribed and written up",
             "On your machine, with speakers separated, ending in notes, decisions and the promises made on both sides."),
            ("check", "Every commitment, with who is waiting",
             "Lane catches “I'll send the pricing by Thursday” and keeps it in front of you until it is done."),
            ("list", "Three things that matter today",
             "Ranked against what you are trying to close, each with the reason it is on the list."),
            ("lock", "Your pipeline is nobody else's dataset",
             "No CRM to feed, no account to make, nothing sent to a server."),
        ]),
        ask=["What did I promise Priya?",
             "Which deals have gone quiet this month?",
             "What was the objection on the Acme call?",
             "Who am I waiting on, and since when?",
             "What did we agree the pricing would be?"],
    ),
    dict(
        slug="for/lawyers",
        kicker="For lawyers",
        title="Lane for lawyers · Your AI second brain, on your Mac",
        desc="A memory tool that cannot leak, because it cannot send. Lane keeps matters, deadlines and what was agreed, entirely on your own machine.",
        h1="A memory that cannot&nbsp;leak",
        lede="The reason you cannot use most of these tools is not that they are bad. It is that using them means handing a record of privileged work to a third party, and no policy page makes that acceptable.",
        problem=("Why the usual answer does not work",
                 "Confidentiality is not a setting. It is either architectural or it is a promise.", [
                     ("lock", "Cloud notes mean disclosure", "Every uploaded page is a copy of privileged material held by somebody else, under their retention rules."),
                     ("clock", "Matters run for years", "The thing you agreed in March matters in November, and by then the thread is thirty messages long."),
                     ("people", "Who said what, precisely", "A deadline moved on a call. Recollection is not a record, and paraphrase is not a quote."),
                 ]),
        does=("What Lane does about it", [
            ("lock", "There is nothing to disclose",
             "Lane has no server and no account. Reading, writing and answering all happen on your Mac, in an encrypted store whose key lives in your Keychain."),
            ("mic", "Calls, transcribed on the machine",
             "Speakers separated, notes written, with what each side actually said kept as text you can check."),
            ("ask", "Answers that cite the source",
             "Every answer points at the memory behind it, with its date, so it can be verified rather than believed."),
            ("check", "Deadlines and undertakings",
             "What was promised, by whom, and by when, kept in front of you until it closes."),
        ]),
        ask=["What was agreed about the 14 October deadline?",
             "What did the other side say about the indemnity?",
             "What is outstanding on this matter?",
             "When did I last write to the client, and what did I say?",
             "Which undertakings are still open?"],
    ),
    dict(
        slug="for/students",
        kicker="For students",
        title="Lane for students · Your AI second brain, on your Mac",
        desc="Lectures, readings and deadlines, remembered as they happen. Lane answers from what you actually saw, on a laptop that works on a train with no signal.",
        h1="Remember the term, not&nbsp;just&nbsp;the&nbsp;week",
        lede="The lecture made sense at the time. The reading made sense at the time. Eight weeks later there is an exam, and what you have is a folder of files with names like Untitled 3.",
        problem=("Why revision feels like starting again",
                 "You did the work. You just cannot get back to it.", [
                     ("file", "Notes and sources come apart", "The idea is in your notes. Which lecture or paper it came from is not."),
                     ("clock", "Deadlines arrive without warning", "The date was said out loud in week two and written on one slide."),
                     ("ask", "Search needs the exact words", "You remember the shape of the argument, not the phrase to search for."),
                 ]),
        does=("What Lane does about it", [
            ("eye", "Everything you read, kept in order",
             "Slides, PDFs, pages and your own notes, indexed by what is in them rather than what they are called."),
            ("mic", "Record the lecture, get the notes",
             "Transcribed on your laptop, written up, with the dates and tasks pulled out."),
            ("ask", "Ask the question you actually have",
             "In your own words, answered from your own term, with the source attached."),
            ("lock", "It works with the wifi off, and costs nothing to run",
             "No subscription to a cloud model, no API key, no data going anywhere."),
        ]),
        ask=["What did the lecturer say about the exam?",
             "What is due this week?",
             "Where did I read the argument about incentives?",
             "Summarise everything from week four.",
             "What did I write in my notes about the essay question?"],
    ),
]


# ── the features ──────────────────────────────────────────────────────────

FEATURES = [
    dict(
        slug="features/ask", icon="ask", nav="Ask your memory",
        kicker="Ask your memory",
        title="Ask your memory · Lane for Mac",
        desc="Ask a question in your own words and get an answer built from what you actually saw, with the source attached, answered on your own Mac.",
        h1="An answer you can&nbsp;check",
        lede="Search needs the word you have forgotten. Ask needs the question you actually have. Lane answers from your own day, cites what it used, and says so plainly when it does not know.",
        card="Not a chatbot with your files attached. A model that only sees what is on your machine, answering from memories it wrote itself.",
        how=[("Ask in your own words", "Press ⌥Space anywhere, or open Lane and type. No syntax, no filters, no folder to choose first."),
             ("Lane finds what it saw", "It searches your memories, files, meetings, people and commitments together, by meaning as well as by word."),
             ("The answer arrives with its receipt", "Every claim points back at the memory behind it, with the date and the app it came from, so you can check it in one click.")],
        detail=[("ask", "It says when it does not know", "An answer with nothing behind it is worse than no answer. If your memory does not contain it, Lane says so instead of inventing something."),
                ("clock", "It understands when", "“Yesterday”, “last week”, “since the call” are read as time, so a question about today is not answered with something from March."),
                ("file", "Documents count as memory", "Your PDFs, notes, code and images with text in them are indexed and cited alongside what was on screen."),
                ("lock", "Offline, always", "The model runs in the app. On a plane with the wifi off, Ask works exactly the same.")],
        ask=["What did Sarah say about the deadline?", "What did I work on last Tuesday?", "What do I owe people right now?", "Where did I see that pricing table?"],
    ),
    dict(
        slug="features/three-things", icon="list", nav="Three things",
        kicker="Three things",
        title="Three things · Lane for Mac",
        desc="Every morning Lane ranks your day against what you are actually trying to do, and gives you three things, each with the reason it made the list.",
        h1="Three things, and why&nbsp;each&nbsp;one",
        lede="A to-do list is a record of everything you once thought was a good idea. Lane reads the day you actually had, scores it against the thing you are trying to build, and lifts three items out of it.",
        card="Hundreds of things happen in a day. Lane ranks them and lifts out the three that actually matter, with the reason each one made the list.",
        how=[("Write your why, once", "One paragraph on what you are building and for whom. It is the only thing you have to type, and even that is optional."),
             ("Lane scores the day against it", "Every commitment, deadline and loose thread is weighed for how close it sits to the why you wrote."),
             ("Three, with reasons", "Not a list of everything. Three, each with the sentence explaining why it is there, and what happens if it waits.")],
        detail=[("check", "It knows what is a commitment", "“I'll send it Friday” outranks an article you meant to read, because somebody is waiting."),
                ("clock", "It knows what expires", "A domain that lapses today or a deadline tomorrow is treated as urgent without you flagging it."),
                ("people", "It knows who is waiting", "A promise made to a person carries the person with it, so you can see who you are holding up."),
                ("bar", "It tells you when you drift", "If a week of work has nothing to do with the why you wrote, Lane says so rather than quietly agreeing.")],
        ask=["What should I do first today?", "Why is this on my list?", "What did I say I would do this week?", "What is about to expire?"],
    ),
    dict(
        slug="features/meetings", icon="mic", nav="Meeting notes",
        kicker="Meeting notes",
        title="Meeting notes · Lane for Mac",
        desc="Record the call, get the notes. Transcribed on your own machine, speakers separated, decisions and promises pulled out, with no bot joining the meeting.",
        h1="Who said what, without a bot&nbsp;in&nbsp;the&nbsp;call",
        lede="No third party is invited, nobody is told the meeting is being recorded by a service, and nothing is uploaded. Lane hears what your Mac hears, and writes it up on the machine.",
        card="No meeting bot joins. Nothing is uploaded. It separates the voices, names them once and remembers who owes what.",
        how=[("Press record, or let it notice", "From the menu bar, the notch tab or the app. Lane can also nudge you when a call starts, if you want it to."),
             ("It listens to the room and the call", "Your microphone and the audio your Mac is playing, so both sides of a video call are captured."),
             ("Notes, decisions, commitments", "Transcribed on device, with speakers told apart, ending in a summary, the decisions taken and who promised what.")],
        detail=[("people", "Speakers, named once", "Tell Lane who is who on the first call and it recognises them on the next one."),
                ("check", "Promises become commitments", "What was agreed on the call joins the same list as everything else you owe."),
                ("clock", "It stops when the room goes quiet", "If nothing is heard for six minutes the recording ends itself and keeps what it had."),
                ("lock", "The audio never leaves", "Transcription happens in the app, on your Mac. There is no service to send the recording to.")],
        ask=["What was decided on the call?", "What did I promise in the meeting?", "Who raised the objection about timing?", "Summarise the last call with the client."],
    ),
    dict(
        slug="features/recall", icon="eye", nav="Recall overlay",
        kicker="Recall overlay",
        title="Recall overlay · Lane for Mac",
        desc="One keystroke anywhere on the Mac brings up your memory, over whatever you are doing, and it is hidden from screen sharing.",
        h1="Your memory, one keystroke&nbsp;away",
        lede="You are in a call, or a document, or a terminal, and you need something you saw last week. Switching apps to look for it is how the thought gets lost. Press ⌥Space instead.",
        card="⌥Space over any app. It reads what is in front of you, answers, and never appears in a screen share.",
        how=[("Press ⌥Space, anywhere", "Over any app, including another app's full screen. Lane does not take the window you were working in."),
             ("Ask, or just look", "Type the question, or glance at what Lane knows about the window in front of you right now."),
             ("Take it and go", "Copy the answer, open the memory behind it, or dismiss it with Escape. You are back where you were.")],
        detail=[("lock", "Invisible in a screen share", "The overlay is content protected, so it does not appear in a shared screen or a recording. What you look up is yours."),
                ("eye", "It knows what is in front of you", "Lane can answer about the page or document you are looking at, not just your history."),
                ("ask", "The same answers, the same citations", "Everything Ask does, in a panel that closes when you are done."),
                ("clock", "Quick, because the model is local", "No round trip to a server, so the first words arrive while you are still reading the question.")],
        ask=["What is this document about?", "What did we agree with this person?", "When did I last see this?", "What do I owe them?"],
    ),
    dict(
        slug="features/dictation", icon="mic", nav="Dictation",
        kicker="Dictation",
        title="Dictation · Lane for Mac",
        desc="Hold a key, speak, and the words land in whatever app you are in. Transcribed on your own machine, with no service listening.",
        h1="Speak it into any&nbsp;app",
        lede="Typing is the slowest part of writing something you have already worked out in your head. Press ⌥⇧Space, say it, and the text lands where your cursor is.",
        card="⌥⇧Space, speak, and the text lands where your cursor is. Transcribed on the machine, in any app.",
        how=[("Press ⌥⇧Space", "In a mail client, an editor, a form, a terminal. Lane does not care which app has focus."),
             ("Say what you mean", "Lane transcribes on the machine as you speak, so there is no upload and no wait for a server."),
             ("Press again to place it", "The text is inserted where the cursor is, in the app you were already using.")],
        detail=[("lock", "No service is listening", "The speech model ships inside the app. Nothing is streamed anywhere, and there is nothing to switch off."),
                ("mic", "It is the same engine as meetings", "The accuracy you get in a dictated note is the accuracy you get in a transcript."),
                ("clock", "It works offline", "On a train, on a plane, in a basement. The wifi is irrelevant."),
                ("ask", "Dictate a question too", "Ask by voice and read the answer, when typing is not convenient.")],
        ask=["Dictate a reply to the client.", "Add a note about what just happened.", "Write down the three things I need to do."],
    ),
    dict(
        slug="features/notch", icon="eye", nav="The notch tab",
        kicker="The notch tab",
        title="The notch tab · Lane for Mac",
        desc="A sliver under the menu bar that grows into a card when you hover it, with your day, your commitments and a line to ask, over every app.",
        h1="A sliver under the&nbsp;menu&nbsp;bar",
        lede="The width of a fingernail until you need it. Hover, and it becomes a card with what is happening now, what is next, what you owe people and a line to ask your memory, over whatever is in front of you.",
        card="The width of a fingernail until you need it. Hover and it unfurls, over every app, including full screen ones.",
        how=[("It sits where you already look", "Under the menu bar at the top of the screen, and below the camera housing on the MacBooks that have one."),
             ("Hover to open it", "The card unfurls out of the tab: recent memories, today at a glance, your commitments, and quick actions."),
             ("Click to keep it", "Click the tab and the card stays open while you work. Escape or the close button puts it away.")],
        detail=[("bar", "It says what Lane is doing", "One coloured dot: remembering, recording, listening, or paused. You never have to wonder."),
                ("mic", "Record from it", "Start a meeting recording, dictate, or capture what is on screen without opening the app."),
                ("eye", "It steps aside in full screen", "The tab sits exactly where you reach for the menu bar, so it hides itself while a window is full screen."),
                ("lock", "Switch it off entirely", "One item in the menu bar turns the tab off, and it stays off after a restart.")],
        ask=["What is coming up?", "What do I owe people right now?", "Summarise my day so far.", "Show me recent files."],
    ),
    dict(
        slug="features/commitments", icon="check", nav="Commitments",
        kicker="Commitments",
        title="Commitments · Lane for Mac",
        desc="Lane catches the promises you make in passing, keeps them with the person who is waiting, and closes them when it sees you deliver.",
        h1="The promises you forgot you&nbsp;made",
        lede="“I'll send it Friday.” You meant it when you wrote it, in a thread you will not open again. The person on the other end is keeping score whether or not you are.",
        card="Lane catches the promise in the thread, keeps it with the person waiting, and closes it when it sees you deliver.",
        how=[("It reads them out of the work", "From a thread, a call, a document. Only explicit promises, not everything that looks like a task."),
             ("It keeps the person with the promise", "A commitment carries who is waiting and when it is due, because that is what makes it real."),
             ("It closes them for you", "When Lane sees the thing was sent, the commitment closes itself. You can close one by hand at any time.")],
        detail=[("people", "Both directions", "What you owe others, and what others owe you, kept in the same place."),
                ("list", "They feed the three things", "An overdue promise to a person outranks almost anything else when Lane ranks the day."),
                ("clock", "Due dates, read from the sentence", "“By Friday”, “before the audit”, “end of the month” become dates without you typing one."),
                ("ask", "Ask about them", "“What do I owe people?” is one of the questions Lane is best at.")],
        ask=["What do I owe people right now?", "Who is waiting on me?", "What did I promise Sarah?", "What is overdue?"],
    ),
]


# ── put them together ─────────────────────────────────────────────────────

def related(current: str) -> str:
    items = [(f["icon"], f["nav"], f["card"], "/" + f["slug"]) for f in FEATURES if f["slug"] != current][:3]
    return cards(items)


def build() -> list[str]:
    made = []

    for a in AUDIENCES:
        p_label, p_head, p_items = a["problem"]
        d_head, d_items = a["does"]
        body = (
            sec(p_label, p_head, facts(p_items), "sec mist")
            + sec("What changes", d_head, facts(d_items))
            + sec("Ask it", "Questions this answers on day one",
                  asks(a["ask"]) + '<p class="aside rv">Every answer cites the memory behind it, so you can check it rather than trust it. <a class="tlink" href="/features/ask">How Ask works</a></p>',
                  "sec mist")
            + sec("More", "The parts you would use most", related(""))
        )
        made.append(str(page(a["slug"], a["title"], a["desc"], a["kicker"], a["h1"], a["lede"], body)))

    for f in FEATURES:
        body = (
            sec("How it works", "Three steps, and none of them are filing", steps(f["how"]), "sec mist")
            + sec("Details", "The things that matter once you use it", facts(f["detail"]))
            + sec("Ask it", "What you would ask", asks(f["ask"]), "sec mist")
            + sec("More", "Other things Lane does", related(f["slug"]))
        )
        made.append(str(page(f["slug"], f["title"], f["desc"], f["kicker"], f["h1"], f["lede"], body)))

    # the hub
    every = cards([(f["icon"], f["nav"], f["card"], "/" + f["slug"]) for f in FEATURES])
    who = cards([("people", a["kicker"].replace("For ", "").capitalize(), a["lede"].split(".")[0] + ".", "/" + a["slug"]) for a in AUDIENCES])
    rest = facts([
        ("file", "Files, read and searchable", "Documents, notes, code, subtitles and images with text in them, indexed on your machine and cited in answers."),
        ("people", "People, projects and organisations", "The names that keep coming back become things Lane knows, with what you owe them and when you last spoke."),
        ("bar", "The board", "Everything you know as a map you can move through, with a walkthrough you can play and record."),
        ("lock", "An encrypted vault", "Memories sit in an encrypted store whose key lives in your Keychain, with sealed backups you control."),
        ("eye", "Capture on purpose", "One item in the menu bar writes down whatever is in front of you, bypassing every rule about what to ignore."),
        ("clock", "Retention you set", "Keep raw text for as long as you like, or not at all. Memories survive; the raw text can expire."),
    ])
    body = (
        sec("Every feature", "The seven you would use daily", every, "sec mist")
        + sec("And the rest", "Quieter things, still yours", rest)
        + sec("Who it is for", "The same app, pointed at your work", who, "sec mist")
    )
    made.append(str(page("features", "Features · Lane for Mac",
                         "Everything Lane does: ask your memory, three things, meeting notes, the recall overlay, dictation, the notch tab and commitments. All of it on your own Mac.",
                         "Features", "Everything Lane does, on&nbsp;your&nbsp;Mac",
                         "Lane watches the work you are already doing, writes it into memory with a model that runs in the app, and answers when you ask. Nothing is filed by you, and nothing is sent anywhere.",
                         body)))
    return made


# ── comparisons ───────────────────────────────────────────────────────────
#
# Other people's products are described as we understand them at the time of
# writing, in general terms, and every page says so and invites a correction.
# Where they are the better answer, the page says that too.

FAIR = ('<p class="aside rv">We describe other products as we understand them at the time of writing, '
        'and they change. If we have something wrong, write to <a class="tlink" href="mailto:pb@lane.so">pb@lane.so</a> and we will correct it.</p>')


def table(other: str, rows: list[tuple[str, str, str]]) -> str:
    body = "".join(
        f'<tr><th scope="row">{html.escape(a)}</th><td>{html.escape(b)}</td><td class="own">{html.escape(c)}</td></tr>'
        for a, b, c in rows
    )
    return (
        '<div class="table-scroll rv"><table><thead><tr>'
        f'<th scope="col">&nbsp;</th><th scope="col">{html.escape(other)}</th><th scope="col" class="own">Lane</th>'
        f"</tr></thead><tbody>{body}</tbody></table></div>"
    )


COMPARISONS = [
    dict(
        slug="compare/rewind", nav="vs Rewind", other="Rewind",
        title="Lane vs Rewind · A memory that reads instead of records",
        desc="Rewind records your screen so you can play it back. Lane reads what was on it, writes memories, and answers questions about your day with a model that runs on your Mac.",
        h1="Lane vs&nbsp;Rewind",
        lede="Rewind made the case that your Mac should remember your day, and it was right. The difference is what gets kept, and where the thinking happens.",
        what="Rewind is a Mac app that records your screen and your calls and makes that recording searchable, so you can scrub back to the moment something happened. Its AI features send context to a hosted model.",
        rows=[("What is kept", "A recording of the screen, searchable", "Memories written from the text that was on screen"),
              ("How you get an answer", "Find the moment and watch it again", "Ask the question and read a cited answer"),
              ("Where the thinking happens", "A hosted model, for the AI features", "Rabbit, inside the app, on your machine"),
              ("Works with the network off", "Search does; the AI answers do not", "Everything, always"),
              ("Disk it wants", "Video, which grows with the hours", "Text and memories, which are small"),
              ("What it decides for you", "Nothing; you go and look", "Three things that matter today, with reasons")],
        instead="You want literal playback. Sometimes the only thing that settles an argument is watching the screen again, frame by frame, and Rewind does that and Lane does not.",
    ),
    dict(
        slug="compare/limitless", nav="vs Limitless", other="Limitless",
        title="Lane vs Limitless · On your Mac, or in their cloud",
        desc="Limitless captures conversations, including away from your desk, and thinks about them in the cloud. Lane captures the working day on your Mac and thinks about it there.",
        h1="Lane vs&nbsp;Limitless",
        lede="Both want to give you a memory. They disagree about where it should live, and that disagreement decides almost everything else.",
        what="Limitless is a cloud product for capturing and recalling conversations, with a wearable for catching the ones that happen away from a computer. Your recordings and the work of understanding them happen on their servers.",
        rows=[("Where your day lives", "Their cloud", "Your Mac, encrypted"),
              ("Where the thinking happens", "Their servers", "Rabbit, inside the app"),
              ("What it captures best", "Conversations, including away from the desk", "The whole working day at a computer: pages, documents, calls, files"),
              ("Works with the network off", "No", "Yes, all of it"),
              ("What a breach would expose", "What is held for you on their side", "Nothing, because nothing is held anywhere else"),
              ("Account required", "Yes", "No account at all")],
        instead="Most of what you need to remember is said out loud, away from a screen. A wearable catches the corridor conversation and the lunch, and Lane, which lives on your Mac, does not.",
    ),
    dict(
        slug="compare/granola", nav="vs Granola", other="Granola",
        title="Lane vs Granola · Meetings, or the whole day",
        desc="Granola is a very good notepad for meetings. Lane remembers meetings too, and the eight hours around them, without a bot and without the cloud.",
        h1="Lane vs&nbsp;Granola",
        lede="Granola is excellent at the thing it does. The question is whether meetings are the part of your day worth remembering, or just the part that is easiest to record.",
        what="Granola sits beside your calls, takes the sparse notes you type, and turns them into a clean write-up afterwards with a hosted model. It is meeting-shaped by design.",
        rows=[("What it captures", "Your meetings, and the notes you type in them", "Meetings, plus pages, documents, files and commitments"),
              ("Where the thinking happens", "A hosted model", "Rabbit, inside the app, on your machine"),
              ("What you have to do", "Type notes while the call runs", "Nothing; Lane reads the work you are already doing"),
              ("Between the meetings", "Nothing is captured", "That is most of what Lane keeps"),
              ("Works with the network off", "No", "Yes"),
              ("Who hears the call", "Their service processes it", "Your Mac, and only your Mac")],
        instead="Your job is meetings, you like typing as you listen, and you want a beautifully edited write-up to share straight after. Granola is built for exactly that and is a pleasure to use.",
    ),
    dict(
        slug="compare/otter", nav="vs Otter", other="Otter",
        title="Lane vs Otter · Transcripts, or memory",
        desc="Otter gives you transcripts of your calls in the cloud. Lane transcribes on your Mac, and then remembers the rest of your day as well.",
        h1="Lane vs&nbsp;Otter",
        lede="A transcript is a record of what was said. A memory is a record of what happened, and of what you now owe people because of it.",
        what="Otter is a long-established cloud transcription service: it records meetings, transcribes them on its servers, and makes the text searchable and shareable, on the web and on a phone.",
        rows=[("What you get", "A transcript, searchable and shareable", "Notes, decisions, commitments and an answer to the question you actually have"),
              ("Where the audio goes", "Their servers", "Nowhere; it is transcribed on your Mac"),
              ("Outside meetings", "Nothing", "Pages, documents, files and the promises in them"),
              ("Works with the network off", "No", "Yes"),
              ("Phone and web", "Yes", "No; Lane is a Mac app"),
              ("Sharing with a team", "Built for it", "Export what you choose, one file at a time")],
        instead="You need transcripts that colleagues can open, on any device, without owning a Mac. That is a real requirement and Lane does not meet it.",
    ),
    dict(
        slug="compare/notion", nav="vs Notion", other="Notion",
        title="Lane vs Notion · A place to file, or a memory that fills itself",
        desc="Notion is where you put things. Lane is what remembers them without being told. They solve different halves of the same problem.",
        h1="Lane vs&nbsp;Notion",
        lede="Notion answers questions about what you wrote down. Lane answers questions about what you did. Most of what you need later was never written down at all.",
        what="Notion is a shared workspace: documents, databases and wikis that you and your team fill in, with an assistant that answers over what is in them. It lives in the cloud so that other people can reach it.",
        rows=[("Who fills it in", "You do, deliberately", "Nobody; Lane reads the work as it happens"),
              ("What it knows", "What was written into it", "What was on your screen, said in your calls, and kept in your files"),
              ("Where it lives", "Their cloud, so a team can share it", "Your Mac, encrypted, for you alone"),
              ("Where the thinking happens", "Their servers", "Rabbit, inside the app"),
              ("Works with the network off", "Partly", "Entirely"),
              ("Good at", "Structure a team agrees on", "Everything nobody had time to structure")],
        instead="You need one place a team can read and edit together: a wiki, a roadmap, a shared database. Lane is a private memory for one person and is no substitute for that.",
    ),
    dict(
        slug="compare/obsidian", nav="vs Obsidian", other="Obsidian",
        title="Lane vs Obsidian · Both local, only one fills itself",
        desc="Obsidian keeps plain files on your disk that you write by hand. Lane keeps memories on your disk that it writes for you.",
        h1="Lane vs&nbsp;Obsidian",
        lede="Obsidian people and Lane people want the same thing: your knowledge on your own disk, in your own control. The difference is who does the typing.",
        what="Obsidian is a local-first editor over a folder of markdown files you own outright. It has no capture and no model of its own; what the vault knows is what you took the time to write into it.",
        rows=[("Where it lives", "Plain files on your disk", "An encrypted store on your disk"),
              ("Who writes it", "You, every time", "Lane, from the day you already had"),
              ("What it knows", "What you remembered to record", "What actually happened, including what you forgot"),
              ("Answers", "Plugins, usually calling a cloud model", "Rabbit, inside the app, with citations"),
              ("Portability", "Markdown, yours forever", "Markdown export, and a sealed backup you hold"),
              ("Effort to keep up", "Real, and it never stops", "None")],
        instead="The writing is the thinking. If keeping the vault is how you understand things, an automatic memory is not a replacement for it, and Lane exports markdown into yours if you want both.",
    ),
    dict(
        slug="compare/apple-notes", nav="vs Apple Notes", other="Apple Notes",
        title="Lane vs Apple Notes · The note you did not take",
        desc="Apple Notes keeps the notes you write. Lane keeps the day you did not have time to write about.",
        h1="Lane vs Apple&nbsp;Notes",
        lede="Apple Notes is free, already on your Mac, and perfectly good. It only ever knows the part of the day you stopped to type.",
        what="Apple Notes is the note app that ships with macOS: quick, reliable, synced through iCloud to your phone and iPad, and entirely manual.",
        rows=[("What it knows", "What you typed into it", "What was on screen, in your calls and in your files"),
              ("When it needs you", "Every single time", "Never"),
              ("Where it lives", "Your devices and iCloud", "Your Mac, encrypted, no sync"),
              ("Asking it something", "Search for the word you used", "Ask in your own words and get a cited answer"),
              ("Meetings", "You type during them", "Recorded, transcribed and written up on device"),
              ("Price", "Free with your Mac", "Two months free, then $9 a month")],
        instead="All you need is somewhere to jot a phone number and have it on your phone a second later. Nothing beats the app that is already there.",
    ),
]


# ── prose pages ───────────────────────────────────────────────────────────

def doc(sections: list[tuple], updated: str = "22 September 2026") -> str:
    """A long text page: a list of anchors, then the sections.

    Each section is (title, *body parts), so a section can mix paragraphs
    and lists without wrapping them by hand.
    """
    toc = "".join(f'<a href="#{slugify(s[0])}">{html.escape(s[0])}</a>' for s in sections)
    out = "".join(
        f'<section class="doc-sec" id="{slugify(s[0])}"><h2>{html.escape(s[0])}</h2>{"".join(s[1:])}</section>'
        for s in sections
    )
    return (f'<section class="sec"><div class="wrap doc">'
            f'<nav class="doc-toc" aria-label="On this page"><span class="label">On this page</span>{toc}'
            f'<p class="doc-when">Last updated {html.escape(updated)}</p></nav>'
            f'<div class="doc-body">{out}</div></div></section>')


def slugify(t: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", t.lower()).strip("-")


def p(*paras: str) -> str:
    return "".join(f"<p>{x}</p>" for x in paras)


def ul(*items: str) -> str:
    return "<ul>" + "".join(f"<li>{i}</li>" for i in items) + "</ul>"


MAKER = "Lane is made by Partha Borthakur."
CONTACT = 'Write to <a href="mailto:pb@lane.so">pb@lane.so</a>.'


def build_rest() -> list[str]:
    made = []

    # ---- comparisons ----
    for c in COMPARISONS:
        others = cards([("bar", x["nav"], f'How Lane and {x["other"]} differ, and when {x["other"]} is the better answer.', "/" + x["slug"])
                        for x in COMPARISONS if x["slug"] != c["slug"]][:3])
        body = (
            sec("What it is", f'What {c["other"]} is built for', p(c["what"]) , "sec mist")
            + sec("Side by side", "Where they part company", table(c["other"], c["rows"]) + FAIR)
            + sec("Be fair", f'When {c["other"]} is the better answer', p(c["instead"]), "sec mist")
            + sec("More", "Other comparisons", others)
        )
        made.append(str(page(c["slug"], c["title"], c["desc"], c["nav"], c["h1"], c["lede"], body)))

    # ---- the comparison hub ----
    hub = cards([("bar", x["nav"], x["lede"].split(".")[0] + ".", "/" + x["slug"]) for x in COMPARISONS])
    body = (
        sec("Every comparison", "Lane next to the tools people already use", hub, "sec mist")
        + sec("The short version", "What actually separates them",
              facts([("lock", "Where your day lives", "Almost every tool here keeps your day on somebody's servers. Lane keeps it in an encrypted store on your Mac and has no server to send it to."),
                     ("eye", "Who does the filing", "Notes apps and wikis know what you typed into them. Lane reads the work you were doing anyway."),
                     ("ask", "Where the thinking happens", "Most AI features call a hosted model. Rabbit runs inside the app, which is why Lane still answers with the wifi off."),
                     ("clock", "What is captured", "Meeting tools know your calls. Lane knows the calls and the eight hours around them.")])
              + FAIR)
    )
    made.append(str(page("compare", "Compare · Lane for Mac",
                         "How Lane compares with Rewind, Limitless, Granola, Otter, Notion, Obsidian and Apple Notes, including when each of them is the better answer.",
                         "Compare", "Lane, next to the&nbsp;alternatives",
                         "Every one of these is good at something. Here is what each is built for, where Lane differs, and when you should pick the other one.",
                         body)))

    # ---- Rabbit ----
    body = (
        sec("Why", "A memory app that calls a model elsewhere ships your day elsewhere",
            p("Every other way of building this ends with your working day on somebody's server: to answer a question about your week, the week has to be sent somewhere to be read. That is not a policy problem you can solve with a promise. It is an architecture problem, and the only honest fix is to do the thinking on the machine that already has the day on it.",
              "So we trained and shipped our own. Rabbit runs inside Lane, on your Mac. There is no API key to paste, no account to make, no usage bill, and no request to intercept, because there is no request."),
            "sec mist")
        + sec("What it does", "Four jobs, all of them local",
              facts([("eye", "Decides what is worth keeping", "Most of what crosses a screen is noise. Rabbit decides what deserves to become a memory, and most things do not."),
                     ("file", "Writes the memory", "A title, a summary, the people and projects in it, the decisions, and anything promised, each checked back against the text it came from."),
                     ("list", "Ranks the day", "Scores everything open against the why you wrote, and lifts out three things with the reason each one made the list."),
                     ("ask", "Answers the question", "Finds what it saw, answers in your words, cites the memory, and says so when your memory does not contain the answer.")]))
        + sec("Honest about it", "What a model on a laptop can and cannot do",
              facts([("bar", "It is sized to your Mac", "A smaller model on an 8 GB machine, a larger one where there is 16 GB or more. Lane picks the tier and downloads it once."),
                     ("check", "It is held to the source", "Extractions are verified against the text they came from, and answers carry citations, because a confident invention is worse than no answer."),
                     ("clock", "It is not the fastest thing in the world", "The first words of an answer arrive in a few seconds on a cold start, quicker once it is warm. That is the price of not sending your day away."),
                     ("lock", "It never phones home", "No telemetry, no prompts logged anywhere, no usage counted. We cannot see what you ask it, by construction.")]),
              "sec mist")
        + sec("More", "The parts it powers", cards([(f["icon"], f["nav"], f["card"], "/" + f["slug"]) for f in FEATURES[:3]]))
    )
    made.append(str(page("rabbit", "Rabbit, the model · Lane for Mac",
                         "Rabbit is the model that does Lane's thinking. It runs inside the app, on your Mac, which is the only way a memory app can honestly promise privacy.",
                         "Rabbit", "Our own model, inside&nbsp;the&nbsp;app",
                         "No OpenAI, no Anthropic, no Google, no API key. Rabbit reads your day, writes the memories, ranks what matters and answers your questions, and it does all of it on your machine.",
                         body)))
    return made


def build_company() -> list[str]:
    made = []

    # ---- pricing ----
    plans = '''<div class="pricetable rv">
      <div class="pcol is-first">
        <span class="plan-tag">Only 200 spots</span>
        <b>Lifetime</b><span class="plan-p">$499<i>once</i></span>
        <p>Every update, every version, for as long as Lane exists. No renewal, ever.</p>
        <a class="btn btn-primary" href="/buy/lifetime">Buy once</a>
      </div>
      <div class="pcol"><b>Monthly</b><span class="plan-p">$9<i>a month</i></span>
        <p>Cancel in a click, from inside the app or by writing to us.</p>
        <a class="btn btn-ghost" href="/buy/monthly">Choose monthly</a></div>
      <div class="pcol"><b>Yearly</b><span class="plan-tag is-quiet">Two months off</span><span class="plan-p">$89<i>a year</i></span>
        <p>The same thing, billed once a year, for the price of ten months.</p>
        <a class="btn btn-ghost" href="/buy/yearly">Choose yearly</a></div>
    </div>'''
    body = (
        sec("", "", plans + '<p class="aside rv">Two months free first, with everything switched on and no card to start. <a class="tlink" href="/#access">Join the waitlist</a></p>', "sec")
        + sec("What you get", "The same app in every plan",
              facts([("check", "Everything, from the first day", "There is no tier that reads your day and gives you less. Every plan is the whole app."),
                     ("eye", "No free tier, on purpose", "A memory app funded by anything other than the person using it is a contradiction. If Lane is running, you are the customer."),
                     ("lock", "No account, ever", "A licence is a signed note checked on your own Mac. There is nothing to log into and nothing for us to hold."),
                     ("key", "One key, your machines", "Use your licence on the Macs you work on. We do not count them or phone home to check.")]),
              "sec mist")
        + sec("Billing", "The dull but important part",
              facts([("clock", "The trial is two months", "It starts the first time Lane runs, and it is kept on your machine, so reinstalling does not restart it and does not extend it."),
                     ("bar", "Payments are handled by Dodo Payments", "Our payment processor handles the card and the tax. We never see your card details."),
                     ("check", "Changed your mind", "Write within 14 days of buying and we refund you, no argument. The trial exists so it should not come to that."),
                     ("file", "When the trial ends", "Lane stops reading and stops making memories. Nothing is deleted: everything you already have stays on your Mac and comes back the moment you unlock it.")]))
        + sec("Questions", "The ones people ask about money",
              '<div class="faq rv">'
              '<details><summary>What does lifetime actually mean?</summary><p>You pay once and use every version of Lane for Mac that we ship, for as long as we ship it. It is limited to the first 200 people because it is a bet on us by people who are early, and it would not be a sustainable price for everybody.</p></details>'
              '<details><summary>What happens if you disappear?</summary><p>Lane keeps working. It needs no server, so there is nothing to switch off. Your memories stay in a store on your disk, and you can export them to markdown whenever you like.</p></details>'
              '<details><summary>Is there a team or company plan?</summary><p>Not yet. Lane is a private memory for one person. An organisation version is the reason Rabbit exists, but it is not what we are selling today.</p></details>'
              '<details><summary>Do you take purchasing power into account?</summary><p>If $9 is genuinely out of reach where you are, write to us and say so. We would rather you used it.</p></details>'
              '</div>', "sec mist")
    )
    made.append(str(page("pricing", "Pricing · Lane for Mac",
                         "Two months free, then $9 a month or $89 a year. The first 200 people can buy Lane once for $499 and keep it for life. No account, no free tier that reads your day.",
                         "Pricing", "Two months free.<br>Then nine&nbsp;dollars.",
                         "One app, one price, no tier that quietly does less. The trial is long enough to see whether Lane is worth it before you pay for it.",
                         body)))

    # ---- security ----
    body = doc([
        ("How Lane is built", p(
            "Lane is a Mac app that does its work on your machine. It has no server, no account system and no network calls for anything it does with your data. That is not a configuration you can get wrong; it is the shape of the program.",
            "There are exactly two moments Lane touches the network, and both are optional and visible: checking for an update, and downloading the model the first time you run it.")),
        ("Where your data sits", p(
            "Memories, activities, files and transcripts live in an encrypted store on your disk, using SQLCipher. The key is generated on your machine and kept in your login Keychain, not in a file beside the database and not with us.",
            "Backups are sealed with a passphrase you choose. A backup is useless to anyone who does not have it, including us.")),
        ("What Lane reads", p(
            "By default, Lane reads the text your applications already publish through macOS accessibility, which is the same information a screen reader uses. That is text, not pixels.",
            "Screen recording is off unless you switch it on, and is used only to keep a small picture with a memory, or to record a walkthrough you asked for. The microphone is used only while you are recording a meeting or dictating, and you start both."),
         ul("Accessibility, so Lane can read text from windows. Required.",
            "Microphone, for meetings and dictation. Only while recording.",
            "Screen Recording, optional, for screenshots and the system audio side of a call.",
            "Calendar and Contacts, optional, for meeting prep and names.")),
        ("The code you run", p(
            "The app is signed with an Apple Developer ID and notarised by Apple, so macOS can verify it came from us and has not been altered.",
            "Updates are signed with a key we hold offline. The app checks the signature before installing anything, so an update that did not come from us is refused.",
            "Licence keys are signed the same way and checked on your machine, which is why unlocking Lane needs no server.")),
        ("Our website", p(
            "lane.so is a static site. It sets no cookies and runs no analytics. If you join the waitlist, your email address is stored so we can write to you. If you buy, our payment processor handles the card and we never see it.",
            'The full detail is in the <a href="/privacy">privacy policy</a>.')),
        ("Reporting something", p(
            "If you find a security problem, please tell us before you tell anyone else, and give us a reasonable chance to fix it. " + CONTACT,
            "We will confirm we have received it, keep you posted, and credit you when it is fixed, unless you would rather we did not.")),
    ])
    made.append(str(page("security", "Security · Lane for Mac",
                         "How Lane is built, where your data sits, what it reads, how the code is signed, and how to report a problem.",
                         "Security", "Nothing to breach, because nothing&nbsp;is&nbsp;sent",
                         "The strongest security claim a memory app can make is that there is no copy of your day anywhere else. That is the only claim Lane makes, and everything below is how it holds.",
                         body)))

    # ---- privacy, explained ----
    body = doc([
        ("The short version", p(
            "Lane does not send your day anywhere. Not to us, not to a model provider, not to an analytics service. There is no account to make and no server holding anything of yours.",
            "This is not restraint on our part. Lane has no code that could upload your memories, which is why we are comfortable putting it this plainly.")),
        ("Why it is built this way", p(
            "A record of everything you do is the single worst thing to hand to a server. It is the most valuable thing about you and the most damaging thing to lose, and no promise about retention or encryption in transit changes that.",
            "So we did the harder thing and trained our own model, which runs inside the app. Rabbit is why Lane can be useful without being told anything about you by anybody else.")),
        ("What Lane sees, and what it ignores", p(
            "Lane reads the text your apps publish for accessibility: the page you are reading, the document you are writing, the title of the window. It never reads keystrokes and it never stores what you type into a password field.",
            "You can pause it from the menu bar at any moment, and exclude any app or any website permanently. Anything you exclude is never read, not read and discarded.")),
        ("What we can see", p(
            "Almost nothing. We know how many people downloaded the app, because the download link counts clicks. We know who joined the waitlist, because they told us their email address. We know who bought a licence, because they paid us.",
            "We do not know how you use Lane, what is in your memories, what you ask it, or whether you have opened it since installing. There is no telemetry.")),
        ("Things you can choose to send", p(
            "Feedback, which opens your mail app with the note you wrote and nothing attached. Labels for improving Rabbit, which are off unless you turn them on and which you can review before they go.",
            "Both are explicit, both are optional, and neither is on by default.")),
        ("Deleting things", p(
            "Remove a single memory, a day, or everything, from inside the app. Set how long raw screen text is kept, or set it to zero and keep only the memories.",
            'Deleted means deleted off your disk. There is no copy of it anywhere for us to delete. The formal document is the <a href="/privacy">privacy policy</a>.')),
    ])
    made.append(str(page("privacy-explained", "Privacy, explained · Lane for Mac",
                         "In plain words: what Lane sees, what it ignores, what we can see, and why a memory app that calls a model elsewhere is a memory app that ships your day elsewhere.",
                         "Privacy", "Your memory should never become someone else's&nbsp;dataset",
                         "The legal document is on another page. This one is the same thing in plain words, because a privacy promise nobody can read is not a promise.",
                         body)))
    return made


def build_legal() -> list[str]:
    made = []

    # ---- privacy policy ----
    body = doc([
        ("Who we are", p(
            MAKER + " In this policy, Lane means both the Mac application and the website at lane.so. " + CONTACT,
            'If you want this in plain words rather than legal ones, read <a href="/privacy-explained">privacy, explained</a>. Where the two differ, this page is the one that governs.')),
        ("The application", p(
            "The Lane application does not collect, transmit or receive any personal data. It has no accounts, no telemetry and no analytics, and it does not send your content to us or to any third party.",
            "Everything the application records about your work is written to an encrypted store on your own device. We have no access to it, and no ability to obtain it.",
            "Two optional features send something, and only when you start them: the feedback form, which opens your own mail application with a message you can read and edit before sending; and the contribution of labels for improving our model, which is off by default and which you review before anything is sent.")),
        ("The website", p("When you use lane.so we process a small amount of data:"),
         ul("<b>Waitlist.</b> If you submit the form, we store your email address, any name you give, and the page you came from, so that we can write to you about early access. Lawful basis: your consent.",
            "<b>Downloads.</b> When the download link is used we record that a download happened, and the source parameter in the link. No IP address and no identifier is stored with it.",
            "<b>Feedback.</b> If you send feedback through the website we store what you wrote and, if you give one, your email address.",
            "<b>Purchases.</b> If you buy a licence we store your email address, the plan and the amount, so that we can support the purchase and meet our tax obligations."),
         p("The website sets no cookies and runs no analytics or advertising scripts. Your browser stores one flag locally to remember that you have already joined the waitlist; it never leaves your browser.")),
        ("Who else is involved", p("We use two processors, and no others:"),
         ul("<b>Supabase</b> hosts the database holding the waitlist, feedback and purchase records.",
            "<b>Dodo Payments</b> processes payments and acts as merchant of record. They handle your card details; we never receive them.",
            "<b>Netlify</b> serves the website and, like any web server, processes requests in order to answer them."),
         p("We do not sell your data, share it for advertising, or use it to train any model.")),
        ("How long we keep it", p(
            "Waitlist entries are kept until you ask us to remove them, or until the waitlist is closed and everyone on it has been contacted. Purchase records are kept for as long as tax law requires us to keep them. Feedback is kept until it has been acted on.")),
        ("Your rights", p(
            "You can ask us for a copy of what we hold about you, ask us to correct it, or ask us to delete it. Because the application holds nothing of yours, any such request concerns only the website data listed above.",
            "Write to <a href=\"mailto:pb@lane.so\">pb@lane.so</a> and we will answer within 30 days. If you are in the UK or the EU and are unhappy with our answer, you may complain to your data protection authority.")),
        ("Children", p("Lane is not directed at children and we do not knowingly collect data from anyone under 16.")),
        ("Changes", p(
            "If this policy changes in a way that affects you, we will say so on this page and, where it is significant and we have your address, by email. The date at the top is the date of the current version.")),
    ])
    made.append(str(page("privacy", "Privacy policy · Lane",
                         "What Lane collects, which is nothing in the app, and the small amount the website processes when you join the waitlist or buy a licence.",
                         "Privacy policy", "Privacy policy",
                         "The application collects nothing. The website collects an email address if you give us one. This is the formal version of both.",
                         body)))

    # ---- terms ----
    body = doc([
        ("These terms", p(
            "These terms govern your use of the Lane application and the lane.so website. " + MAKER + " By installing or using Lane you accept them.")),
        ("Your licence", p(
            "We grant you a personal, non-exclusive, non-transferable licence to install and use Lane on the Macs you personally use, for as long as your plan is current or, on the lifetime plan, permanently."),
         ul("You may not resell, rent, sublicense or share your licence key.",
            "You may not reverse engineer, decompile or attempt to extract the model shipped with the application, except where that restriction is void under the law that applies to you.",
            "You may use Lane for commercial work. It is a tool; what you do with it is yours.")),
        ("The trial", p(
            "Lane runs in full for two months from the first time you open it, with no payment details required. The trial is recorded on your own machine, so reinstalling neither restarts nor extends it.",
            "When the trial ends, Lane stops reading and stops writing new memories. It does not delete anything: everything already on your disk stays there, and becomes available again as soon as you enter a licence key.")),
        ("Paying", p(
            "Prices are shown on the pricing page and are in US dollars. Payments are taken by Dodo Payments, who act as merchant of record and who handle tax.",
            "Subscriptions renew automatically until cancelled, and you can cancel at any time; cancelling stops the next payment and leaves your current period running.",
            "The lifetime plan is a single payment for all future versions of Lane for Mac, limited to the first 200 purchasers. If we ever make a separate product, it is a separate product.",
            "If you change your mind, write to us within 14 days of buying and we will refund you in full.")),
        ("What we promise, and what we do not", p(
            "We will make a reasonable effort to keep Lane working, to fix what is broken, and to be straight with you about what it does.",
            "Lane is provided as is. We do not warrant that it will be uninterrupted or error free, that it will suit a particular purpose, or that what it tells you will be correct. It is a memory aid built on a model, and models are wrong sometimes. Do not rely on it as the sole record of anything that matters legally, medically or financially.")),
        ("Your data is your responsibility", p(
            "Because everything stays on your machine, we cannot recover it for you. We have no copy of your memories, your licence key or your encryption key. Keep backups. Lane can make sealed ones for you; where you put them is up to you.")),
        ("Liability", p(
            "To the fullest extent the law allows, we are not liable for indirect or consequential loss, for lost profits, or for lost data. Where liability cannot be excluded, it is limited to the amount you have paid us in the twelve months before the claim.",
            "Nothing here limits liability for death or personal injury caused by negligence, for fraud, or for anything else that cannot lawfully be limited. If you are a consumer, your statutory rights are unaffected.")),
        ("Ending it", p(
            "You can stop using Lane whenever you like; uninstalling is enough. We may end your licence if you breach these terms in a way you do not put right after we have asked.")),
        ("Changes and law", p(
            "We may change these terms. If a change matters, we will say so on this page and date it. Continuing to use Lane after that means you accept it.",
            "These terms are governed by the laws of India, and the courts of India have jurisdiction, except where the law of your own country gives you rights you cannot be deprived of by agreement.")),
        ("Getting in touch", p(CONTACT + " A real person reads it.")),
    ])
    made.append(str(page("terms", "Terms · Lane",
                         "The terms for using Lane: your licence, the two month trial, paying, refunds, what we promise and what we do not.",
                         "Terms", "Terms",
                         "Short, in plain English where plain English will do. If anything here is unclear, write to us and we will explain it rather than hide behind it.",
                         body)))
    return made


def build_updates() -> list[str]:
    made = []

    # ---- changelog ----
    entries = '''<ol class="log rv">
      <li><div class="log-when"><b>1.0</b><span>In testing</span></div><div class="log-what">
        <p>The first release. Passive capture of the working day, memories written on device by Rabbit, Ask with citations, three things ranked against your why, meetings recorded and transcribed on the machine, the recall overlay, dictation, the notch tab, commitments, the board, and an encrypted store with sealed backups.</p>
        <p class="log-note">Being tested on real Macs before it goes out. <a class="tlink" href="/#access">Join the waitlist</a></p></div></li>
      <li><div class="log-when"><b>0.9</b><span>September 2026</span></div><div class="log-what">
        <p>The notch tab learned to unfurl rather than appear, and to sit below the camera housing on the MacBooks that have one. Capture on purpose, from the menu bar. Memories grouped by place and category, with a card you can share.</p></div></li>
      <li><div class="log-when"><b>0.8</b><span>September 2026</span></div><div class="log-what">
        <p>Answers got harder to fool: every source now carries when it happened, and a date written inside a document is no longer mistaken for the date of the memory. File ingestion widened to code, notes, data and subtitles.</p></div></li>
      <li><div class="log-when"><b>0.7</b><span>September 2026</span></div><div class="log-what">
        <p>Meetings that save what they heard even when something goes wrong, a six minute silence watchdog, and live transcripts while recording. The recall overlay became glass, and hides itself from screen sharing.</p></div></li>
      <li><div class="log-when"><b>0.6</b><span>September 2026</span></div><div class="log-what">
        <p>Rabbit shipped inside the app: no Ollama, no API key, a model tier chosen for the RAM you have. Semantic search over memories and files. The encrypted vault, with the key in your Keychain.</p></div></li>
    </ol>'''
    body = sec("", "", entries + '<p class="aside rv">Lane updates itself, and every update is signed. <a class="tlink" href="/security">How that works</a></p>')
    made.append(str(page("changelog", "Changelog · Lane for Mac",
                         "What changed in each version of Lane, newest first.",
                         "Changelog", "What changed, and&nbsp;when",
                         "Newest first. Versions before 1.0 were tested on our own machines and with a small number of people.",
                         body)))

    # ---- roadmap ----
    body = (
        sec("Next", "What we are working on now",
            facts([("check", "Getting 1.0 onto other people's Macs", "Notarised, tested on machines that are not ours, and out to the waitlist."),
                   ("bar", "Measuring the cost", "CPU, battery and disk over a real working week, published rather than claimed."),
                   ("ask", "Faster answers", "Keeping the context warm between follow-up questions, so the second question is quicker than the first.")]),
            "sec mist")
        + sec("After that", "Decided, not yet built",
              facts([("file", "A browser extension", "Cleaner page text and real URLs from Chrome and Safari, rather than reading the window."),
                     ("people", "A memory graph, and a daily recap", "People, projects and documents as one map, and a short account of the day at the end of it."),
                     ("lock", "Sync that we cannot read", "An end to end encrypted vault between your own Macs. We would hold sealed bytes and no key."),
                     ("bar", "Windows", "The same app, the same architecture, capture through UI Automation and the vault through DPAPI.")]))
        + sec("Later", "Honest about the distance",
              facts([("ask", "A better Rabbit", "Training our own small model on consented captures, rather than shipping a general one with a Lane-shaped prompt."),
                     ("people", "Something for organisations", "On premises, for teams who cannot send their work anywhere. It is the reason Rabbit exists."),
                     ("clock", "A phone", "Reading your memory from your pocket, without a server in the middle. We do not yet know the honest way to do this.")]),
              "sec mist")
        + sec("Not planned", "Things people ask for that we will not do",
              facts([("lock", "A cloud that holds your day", "It would make several of these easier. It is the one thing Lane exists not to do."),
                     ("eye", "A free tier that reads your work", "If the app is running, the person using it should be the customer."),
                     ("bar", "Selling anything derived from your memories", "There is nothing to sell, because we never have it.")]))
    )
    made.append(str(page("roadmap", "Roadmap · Lane for Mac",
                         "What we are building next, what comes after, and the things we have decided not to do.",
                         "Roadmap", "What is next, and what&nbsp;never&nbsp;will&nbsp;be",
                         "No dates, because dates on a roadmap are usually fiction. This is the order, and the last section is the part most roadmaps leave out.",
                         body)))

    # ---- blog ----
    body = (
        sec("", "",
            '<div class="empty rv"><b>Nothing here yet.</b>'
            '<p>We would rather ship the thing than write about shipping it. When there is something worth reading, about how Rabbit is trained, what a local model can honestly do, or what we got wrong, it will be here.</p>'
            '<p>In the meantime the <a class="tlink" href="/changelog">changelog</a> says what changed, and the <a class="tlink" href="/roadmap">roadmap</a> says what is coming.</p></div>')
    )
    made.append(str(page("blog", "Blog · Lane",
                         "Notes on building a memory app that keeps everything on your own machine. Nothing published yet.",
                         "Blog", "Notes, when there is something worth&nbsp;saying",
                         "On local models, on memory, and on what we get wrong along the way.",
                         body)))
    return made


if __name__ == "__main__":
    for f in build() + build_rest() + build_company() + build_legal() + build_updates():
        print("wrote", f.replace(str(ROOT) + "/", ""))
