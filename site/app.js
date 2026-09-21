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

/* ================= the waitlist and the first two hundred seats =================
   The only part of this site that talks to a server. The app never does.
   It calls two Supabase functions, which return counts and never rows. */
(function () {
  var form = document.getElementById('wait');
  var seats = document.getElementById('seats');
  if (!form && !seats) return;

  var db = (document.querySelector('meta[name="lane-db"]') || {}).content || '';
  var key = (document.querySelector('meta[name="lane-key"]') || {}).content || '';
  db = db.replace(/\/+$/, '');
  var SEATS = 200;

  var msg = document.getElementById('wait-msg');
  var email = document.getElementById('wait-email');
  var left = document.getElementById('seats-left');
  var fill = document.getElementById('seats-fill');
  var button = form && form.querySelector('button');

  function call(fn, body) {
    return fetch(db + '/rest/v1/rpc/' + fn, {
      method: 'POST',
      headers: { apikey: key, Authorization: 'Bearer ' + key, 'Content-Type': 'application/json' },
      body: JSON.stringify(body || {}),
    }).then(function (r) {
      return r.json().then(function (d) {
        if (!r.ok) throw new Error((d && (d.message || d.hint)) || 'that did not go through');
        return d;
      });
    });
  }

  function say(text, kind) {
    if (!msg) return;
    msg.textContent = text;
    msg.className = 'wait-msg' + (kind ? ' ' + kind : '');
  }

  function paint(n) {
    if (typeof n !== 'number' || n < 0) return;
    if (left) left.textContent = String(n);
    if (fill) fill.style.width = Math.round(((SEATS - n) / SEATS) * 100) + '%';
  }

  /* How many seats are gone. If the database is unreachable the printed copy
     stands on its own, so the counter simply does not move. */
  if (db && key) {
    call('waitlist_stats')
      .then(function (d) {
        if (!d) return;
        if (typeof d.lifetimeSeats === 'number' && d.lifetimeSeats > 0) SEATS = d.lifetimeSeats;
        paint(d.lifetimeLeft);
      })
      .catch(function () {});
  }

  if (!form) return;

  var joined = false;
  try { joined = localStorage.getItem('lane.waitlist') === '1'; } catch (e) {}
  if (joined) say('You are already on the list. We will write to you.', 'good');

  form.addEventListener('submit', function (e) {
    e.preventDefault();
    var value = (email.value || '').trim();
    if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(value)) {
      email.setAttribute('aria-invalid', 'true');
      email.focus();
      say('That does not look like an email address.', 'bad');
      return;
    }
    email.removeAttribute('aria-invalid');
    if (!db || !key) { say('The list is not open yet. Write to hello@lane.so and we will add you.', 'bad'); return; }

    button.disabled = true;
    say('One moment…');

    call('join_waitlist', { p_email: value, p_source: 'site' })
      .then(function (d) {
        button.disabled = false;
        try { localStorage.setItem('lane.waitlist', '1'); } catch (e) {}
        paint(d.lifetimeLeft);
        form.reset();
        if (d.lifetime) {
          say('You are in, and a lifetime seat is held for you. We will write when the build is ready.', 'good');
        } else {
          say('You are on the list, number ' + d.position + '. We will write when the build is ready.', 'good');
        }
      })
      .catch(function (err) {
        button.disabled = false;
        say(String(err.message || err).indexOf('email address') >= 0
          ? 'That does not look like an email address.'
          : 'We could not reach the list. Write to hello@lane.so and we will add you by hand.', 'bad');
      });
  });
})();
