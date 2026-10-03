# What the testers said

One entry per report, newest first. Nothing here is built yet — the point is
to let it pile up and then do a single pass, rather than a release per remark.

Each entry says what they actually said, what it means underneath, and what it
would take. The verbatim quote is kept because the paraphrase is always a bit
more convenient than the truth.

Status: `open` · `being done` · `done in <version>` · `not doing` (with why).

---

## 2026-10-03 · Tester 2 · installed, two things

> "sweet! Just installed it (fyi the side spots arent working. i attached
> pictures of me trying to attach it to the left, before and after i hover.
> I'd also love integration into Vorssaint (it's open source i've progressively
> replaced more and more of my small apps with it) that way it would just be
> with all the other tools i already use and the drop down notch. i think it
> would be slick. (and it would just fit with my personal preference of not
> having anything ever permanently superimposed over my screen, this is
> completely hidden unless i hover over the notch, but if i had another notch
> app i could only do one or the other hence why i would want integration if it
> were an option. but no stress either way, just trying to give you as honest
> and thorough opinion as possible)"

### 1. The left and right edge spots do nothing — `open`, bug

He picked **Left edge** in Settings → Capture → the little screen, and the tab
never appeared there. His two screenshots, before and after hovering, are
identical.

Both halves of it are written and look right, which is why this needs
reproducing rather than guessing:

- `src/components/notch-position.tsx` offers six spots including `left`/`right`.
- `src-tauri/src/lib.rs` — `NotchPin::Left`/`Right` place the window against
  `visibleFrame`, vertically centred, and size the tab 14×180 instead of 180×14.
- `src/notch.tsx` — `tabSize()` returns the vertical size and the tab gets an
  `is-vertical` class.
- Changing it emits `notch-position`, and the window re-reads the pin.

So it is implemented end to end and still did not work for him. Leads worth
pulling, in order:
- Does the window actually resize to 14×180, or stay 180×14 and sit off screen?
  A 180-wide window pinned at `vf.origin.x` would mostly be on screen, so a
  wrong size is more likely than a wrong origin.
- Does it come good after a relaunch? That separates "never works" from "does
  not apply live", and they are different bugs.
- Does the card still open from a vertical tab? The card is 360 wide and the
  hover measurement was written for a horizontal pill.
- Which Mac — a notch MacBook or not. The housing inset only affects the top
  centre, but it is the sort of thing that turns out to matter.

**First job is to reproduce it on a side spot here.** Nobody should write a fix
for this before seeing it fail.

### 2. Make Lane work inside Vorssaint — `open`, the interesting one

[Vorssaint](https://github.com/) is an open-source Mac utility suite with a
Dynamic Island: music, controls and small tools in a drop-down from the notch.
He is deliberately collapsing his small apps into it.

What he is actually saying, underneath the feature request:

**The notch is exclusive, and he has already given it to somebody else.** Two
notch apps cannot both own that spot, so he has to choose, and Lane is arriving
second against a tool he has already committed to. He is not asking for a
nicety; he is telling us the price of entry.

**And he does not want anything permanently on his screen.** The thing he likes
about the island is that it is invisible until hovered. Lane's tab satisfies
that too — which is why he is not asking us to drop the notch, only to stop
requiring it to ourselves.

This is not one person's layout preference. Anyone running boring.notch,
NotchNook, Alcove, Notch Buddy or Vorssaint has the same conflict, and that is
a large slice of exactly the Mac audience Lane is for. We already half knew
this: `running_notch_apps()` in `lib.rs` exists so Lane steps aside for them.
It matches a name containing "notch", or "alcove" — **so Vorssaint is not
detected, and Lane did not step aside for him.** That is a real gap and it is
cheap to close.

Three ways to answer it, cheapest first:

1. **Make Lane whole without the notch.** If everything the tab gives you is
   also on a hotkey and in the menu bar, the conflict stops being a choice
   between two apps. This is the one that helps every tester with a notch app,
   not just him, and it needs nobody else's cooperation.
2. **Widen `running_notch_apps()`** beyond names containing "notch", so Lane
   actually stands aside for Vorssaint and the rest. Small, and it is the
   behaviour we already claim.
3. **A Vorssaint module.** The real ask, and the most work: it means an
   interface into Lane from another process, which is a decision about how open
   Lane is, not just a feature. Worth it only if more than one tester wants it.

Worth asking him what he would lose if Lane kept its own tab but also answered a
hotkey — it may be that (1) is the whole of what he needs, and it costs us an
afternoon rather than a design.

### Tone

Unprompted, detailed, screenshots attached, and explicitly trying to be useful
rather than to complain. Worth saying thank you properly, and worth telling him
what happened to both of these when it does.

---

## How to add to this

Paste the message verbatim under a new dated heading, then write what it means
underneath. Keep the verbatim bit even when it is rambling: the useful detail is
usually in the aside, as it was here with "if i had another notch app i could
only do one or the other".
