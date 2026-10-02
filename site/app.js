/* Lane — page behaviour. The marquee, gentle reveals, and the four small
   performances: Ask answering, a call being written down, recall opening and
   speech becoming text. */
(function () {
  'use strict';
  var reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  function seen(el, cb) {                       // runs cb(true/false) as it enters and leaves
    if (!('IntersectionObserver' in window)) { cb(true); return; }
    new IntersectionObserver(function (es) { cb(es[0].isIntersecting); }, { threshold: 0.2 }).observe(el);
  }
  function bars(el, n, min, max) {
    if (!el) return;
    for (var i = 0; i < n; i++) {
      var b = document.createElement('i');
      b.style.animationDelay = (-Math.random() * 1.1).toFixed(2) + 's';
      b.style.animationDuration = (min + Math.random() * (max - min)).toFixed(2) + 's';
      el.appendChild(b);
    }
  }

  /* the apps Lane already reads, running past */
  var marq = document.getElementById('marq');
  if (marq) {
    var apps = ['Chrome', 'Safari', 'Mail', 'Slack', 'Notion', 'Figma', 'Zoom', 'Google Meet',
                'Word', 'Excel', 'Pages', 'Preview', 'PDFs', 'VS Code', 'WhatsApp', 'Messages',
                'Calendar', 'Teams', 'Linear', 'Keynote'];
    var line = apps.concat(apps);
    for (var i = 0; i < line.length; i++) {
      var s = document.createElement('span');
      s.textContent = line[i];
      marq.appendChild(s);
    }
  }

  /* reveals, with a failsafe so nothing can stay invisible */
  var rvs = [].slice.call(document.querySelectorAll('.rv'));
  if (rvs.length && !reduced && 'IntersectionObserver' in window) {
    document.documentElement.classList.add('rv-on');
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (en) {
        if (en.isIntersecting) { en.target.classList.add('in'); io.unobserve(en.target); }
      });
    }, { rootMargin: '0px 0px -6% 0px', threshold: 0.06 });
    rvs.forEach(function (el) { io.observe(el); });
    setTimeout(function () { rvs.forEach(function (el) { el.classList.add('in'); }); }, 2500);
  }

  /* ---------------- Ask: a question typed, an answer built ---------------- */
  var box = document.getElementById('askbox');
  var qEl = document.getElementById('ask-text');
  var aEl = document.getElementById('ask-answer');
  var sEl = document.getElementById('ask-src');
  if (box && qEl && aEl) {
    var ASKS = [
      { q: 'what did Sarah say about the deadline?',
        a: 'She asked to hold the compliance certificate until the audit closes, and raised the 14 October submission date on the vendor call <span class="cite">[1]</span>. She has had no reply from you since Monday <span class="cite">[2]</span>.',
        s: ['<b>[1]</b> Vendor call · 14 October raised · today 09:38',
            '<b>[2]</b> Mail from Sarah · “hold the certificate until the audit” · Monday'] },
      { q: 'how much did I spend on domains this year?',
        a: '$2,525.13 across fourteen renewals, all on the one billing page <span class="cite">[1]</span>. Three of them were this month, and one more expires today <span class="cite">[2]</span>.',
        s: ['<b>[1]</b> Billing page · annual spend · today 09:36',
            '<b>[2]</b> Renewal notice · expires today · today 09:41'] },
      { q: 'who am I holding up right now?',
        a: 'Sarah, on the compliance certificate, due tomorrow <span class="cite">[1]</span>. The district office, who are waiting on a call you said you would make on Wednesday <span class="cite">[2]</span>.',
        s: ['<b>[1]</b> Vendor call · you said tomorrow · today 09:38',
            '<b>[2]</b> District office thread · they are waiting on you · 11:02'] }
    ];
    var n = 0, vis = false, timer = null;
    seen(box, function (v) { vis = v; if (v && !timer) run(); });

    function wait(ms, next) { timer = setTimeout(function () { timer = null; next(); }, ms); }
    function run() {
      if (!vis) { timer = setTimeout(function () { timer = null; run(); }, 900); return; }
      var item = ASKS[n % ASKS.length]; n++;
      aEl.innerHTML = ''; sEl.hidden = true; sEl.innerHTML = '';
      if (reduced) { qEl.textContent = item.q; aEl.innerHTML = item.a; srcs(item); wait(6000, run); return; }
      var i = 0;
      (function type() {
        qEl.textContent = item.q.slice(0, i++);
        if (i <= item.q.length) { timer = setTimeout(type, 30 + Math.random() * 42); }
        else { wait(450, function () { answer(item); }); }
      })();
    }
    function answer(item) {
      var words = item.a.split(' '), k = 0;
      (function next() {
        aEl.innerHTML = words.slice(0, ++k).join(' ');
        if (k < words.length) { timer = setTimeout(next, 42); }
        else { wait(220, function () { srcs(item); wait(4200, clear); }); }
      })();
    }
    function srcs(item) {
      sEl.innerHTML = item.s.map(function (r) { return '<div>' + r + '</div>'; }).join('');
      sEl.hidden = false;
    }
    function clear() {
      var q = qEl.textContent;
      (function back() {
        q = q.slice(0, -2); qEl.textContent = q;
        if (q.length) { timer = setTimeout(back, 14); }
        else { aEl.innerHTML = ''; sEl.hidden = true; wait(400, run); }
      })();
    }
  }

  /* ---------------- the call, written down as it happens ---------------- */
  var lines = document.getElementById('calllines');
  if (lines) {
    bars(document.getElementById('callwave'), 42, 0.75, 1.5);
    var items = [].slice.call(lines.children);
    var out = document.querySelector('.call-out');
    var whos = {}, cvis = false, ct = null;
    [].slice.call(document.querySelectorAll('.who')).forEach(function (w) {
      w.className.split(' ').forEach(function (c) { if (/^w\d$/.test(c)) whos[c] = w; });
    });
    seen(lines, function (v) { cvis = v; if (v && !ct) cycle(); });
    function cycle() {
      if (!cvis) { ct = setTimeout(function () { ct = null; cycle(); }, 900); return; }
      items.forEach(function (li) { li.classList.remove('in'); });
      if (out) out.classList.remove('in');
      Object.keys(whos).forEach(function (k) { whos[k].classList.remove('on'); });
      if (reduced) { items.forEach(function (li) { li.classList.add('in'); }); if (out) out.classList.add('in'); return; }
      var i = 0;
      (function step() {
        if (i >= items.length) {
          Object.keys(whos).forEach(function (k) { whos[k].classList.remove('on'); });
          ct = setTimeout(function () { if (out) out.classList.add('in'); ct = setTimeout(function () { ct = null; cycle(); }, 4200); }, 500);
          return;
        }
        var li = items[i++];
        Object.keys(whos).forEach(function (k) { whos[k].classList.remove('on'); });
        var w = whos[li.getAttribute('data-w')];
        if (w) w.classList.add('on');
        li.classList.add('in');
        ct = setTimeout(step, 1500);
      })();
    }
  }

  /* ---------------- recall opening ---------------- */
  var hits = document.querySelector('.ks-hits');
  if (hits) {
    var hl = [].slice.call(hits.children), hvis = false, ht = null;
    seen(hits, function (v) { hvis = v; if (v && !ht) hcycle(); });
    function hcycle() {
      if (!hvis) { ht = setTimeout(function () { ht = null; hcycle(); }, 900); return; }
      hl.forEach(function (li) { li.classList.remove('in'); });
      if (reduced) { hl.forEach(function (li) { li.classList.add('in'); }); return; }
      var i = 0;
      (function step() {
        if (i >= hl.length) { ht = setTimeout(function () { ht = null; hcycle(); }, 4200); return; }
        hl[i++].classList.add('in');
        ht = setTimeout(step, 260);
      })();
    }
  }

  /* ---------------- speech becoming text ---------------- */
  var dict = document.getElementById('dictated');
  if (dict) {
    bars(document.getElementById('micwave'), 22, 0.6, 1.2);
    var SAID = [
      'Tell Sarah the certificate goes out tomorrow morning, and ask the district office to confirm Wednesday.',
      'Note for later: the pilot domain expires today, renew it before anything else.',
      'Draft a reply to Danish saying 14 October works for us.'
    ];
    var rib = document.querySelector('.ribbon span');
    if (rib) rib.textContent = SAID.join('   ·   ') + '   ·   ';
    var d = 0, dvis = false, dt = null;
    seen(dict, function (v) { dvis = v; if (v && !dt) dcycle(); });
    function dcycle() {
      if (!dvis) { dt = setTimeout(function () { dt = null; dcycle(); }, 900); return; }
      var text = SAID[d % SAID.length]; d++;
      if (reduced) { dict.textContent = text; return; }
      var i = 0; dict.textContent = '';
      (function type() {
        dict.textContent = text.slice(0, i++);
        if (i <= text.length) { dt = setTimeout(type, 26 + Math.random() * 34); }
        else { dt = setTimeout(function () { dt = null; dcycle(); }, 3200); }
      })();
    }
  }
})();

/* ================= the two hundred seats =================
   The only part of this site that talks to a server. The app never does.
   One endpoint, /join: GET says how many seats are gone, POST takes one.
   Both forms on the page are the same form, so joining from the hero and
   joining from the bottom of the page do and say exactly the same thing. */
(function () {
  var SEATS = 200;
  var forms = [
    { form: 'wait-hero', email: 'wait-hero-email', msg: 'wait-hero-msg' },
    { form: 'wait', email: 'wait-email', msg: 'wait-msg' },
  ]
    .map(function (f) {
      return { form: document.getElementById(f.form), email: document.getElementById(f.email), msg: document.getElementById(f.msg) };
    })
    .filter(function (f) { return f.form && f.email; });

  var taken = document.getElementById('seats-taken');
  var left = document.getElementById('seats-left');
  var heroTaken = document.getElementById('hero-taken');
  var heroLeft = document.getElementById('hero-left');
  var fill = document.getElementById('seats-fill');
  var heroFill = document.getElementById('hero-fill');
  var tag = document.getElementById('seats-tag');
  if (!forms.length && !fill && !heroFill) return;

  function say(f, text, kind) {
    if (!f.msg) return;
    f.msg.textContent = text;
    f.msg.className = 'wait-msg' + (kind ? ' ' + kind : '');
  }

  /* The count, everywhere it appears. If the server cannot be reached the
     printed copy stands on its own and the bar simply does not move. */
  function paint(n) {
    if (!n || typeof n.claimed !== 'number') return;
    var seats = n.seats || SEATS;
    var gone = Math.min(seats, Math.max(0, n.claimed));
    var over = Math.max(0, seats - gone);
    if (taken) taken.textContent = String(gone);
    if (heroTaken) heroTaken.textContent = String(gone);
    if (left) left.textContent = String(over);
    if (heroLeft) heroLeft.textContent = String(over);
    // A bar that rounds to nothing looks broken rather than empty, so the
    // first seat is always worth a sliver.
    var pct = gone === 0 ? 0 : Math.max(1.5, Math.round((gone / seats) * 100));
    if (fill) fill.style.width = pct + '%';
    if (heroFill) heroFill.style.width = pct + '%';
    if (tag) {
      tag.className = 'seatbox-tag' + (over === 0 ? ' is-gone' : over <= 40 ? ' is-tight' : '');
      tag.textContent = over === 0 ? 'Round full' : over <= 40 ? 'Nearly full' : 'Open';
    }
  }

  fetch('/join')
    .then(function (r) { return r.json(); })
    .then(paint)
    .catch(function () {});

  /* Somebody who has already joined should not be asked again on their next
     visit, and should certainly not be told they are too late. */
  var joined = false;
  try { joined = localStorage.getItem('lane.waitlist') === '1'; } catch (e) {}

  forms.forEach(function (f) {
    if (joined) say(f, 'You are already on the list. The licence comes by email.', 'good');

    f.form.addEventListener('submit', function (e) {
      e.preventDefault();
      var value = (f.email.value || '').trim();
      if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(value)) {
        f.email.setAttribute('aria-invalid', 'true');
        f.email.focus();
        say(f, 'That does not look like an email address.', 'bad');
        return;
      }
      f.email.removeAttribute('aria-invalid');
      var button = f.form.querySelector('button');
      var was = button.textContent;
      button.disabled = true;
      button.textContent = 'One moment…';

      fetch('/join?source=site', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email: value }),
      })
        .then(function (r) { return r.json(); })
        .then(function (out) {
          if (!out.ok) throw new Error(out.error || 'That did not go through.');
          try { localStorage.setItem('lane.waitlist', '1'); } catch (e2) {}
          paint(out);
          forms.forEach(function (g) {
            g.form.reset();
            g.form.style.display = 'none';
            say(g, out.again
              ? 'You are already on the list. The licence comes by email.'
              : out.lifetime === false
                ? 'This round is full, but you are first in line for the next two hundred. I will write when it opens.'
                : 'You are in. The licence comes by email, usually the same day.', 'good');
          });
        })
        .catch(function (err) {
          button.disabled = false;
          button.textContent = was;
          say(f, String(err.message || err).indexOf('email address') >= 0
            ? 'That does not look like an email address.'
            : 'We could not reach the list. Write to pb@lane.so and we will add you by hand.', 'bad');
        });
    });
  });
})();


/* ── The two films ──────────────────────────────────────────────────────
   The poster is ours and the player is not. Nothing is requested from
   Google until somebody presses play, and then it goes through
   youtube-nocookie.com, which does not set a tracking cookie for a
   visitor who has not asked to be tracked. */
(function () {
  var posters = document.querySelectorAll('.film-poster[data-film]')
  if (!posters.length) return

  Array.prototype.forEach.call(posters, function (poster) {
    poster.addEventListener('click', function () {
      var id = poster.getAttribute('data-film')
      if (!/^[A-Za-z0-9_-]{6,20}$/.test(id)) return
      var frame = document.createElement('iframe')
      frame.className = 'film-frame'
      frame.src = 'https://www.youtube-nocookie.com/embed/' + id + '?autoplay=1&rel=0&modestbranding=1'
      frame.title = poster.getAttribute('data-title') || 'Lane'
      frame.allow = 'accelerometer; autoplay; encrypted-media; picture-in-picture; web-share'
      frame.referrerPolicy = 'strict-origin-when-cross-origin'
      frame.allowFullscreen = true
      poster.parentNode.replaceChild(frame, poster)
    })
  })
})()
