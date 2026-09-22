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


if __name__ == "__main__":
    for f in build():
        print("wrote", f.replace(str(ROOT) + "/", ""))
