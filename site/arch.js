/* Lane — the architecture, running.
   Everything you do streams in from both sides in waves, lands in Rabbit,
   is written to the vault, and comes back out through the three surfaces. */
(function () {
  'use strict';

  var wrap = document.getElementById('arch');
  if (!wrap) return;
  var cv = wrap.querySelector('.arch-flow');
  var ctx = cv.getContext('2d');
  var reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  var DPR = Math.min(2, window.devicePixelRatio || 1);
  var streams = [], parts = [], W = 0, H = 0, running = true, corePulse = 0, vaultPulse = 0, core = null;

  function centre(el, box) {
    var r = el.getBoundingClientRect();
    return { x: r.left - box.left + r.width / 2, y: r.top - box.top + r.height / 2, w: r.width, h: r.height };
  }

  function edgePoint(a, b) {                    // leave a node from the side facing the target
    var dx = b.x - a.x, dy = b.y - a.y;
    if (Math.abs(dx) > Math.abs(dy)) return { x: a.x + (dx > 0 ? a.w / 2 : -a.w / 2), y: a.y };
    return { x: a.x, y: a.y + (dy > 0 ? a.h / 2 : -a.h / 2) };
  }

  function path(a, b) {
    var p0 = edgePoint(a, b), p1 = edgePoint(b, a);
    var dx = p1.x - p0.x, dy = p1.y - p0.y;
    var flat = Math.abs(dx) > Math.abs(dy);
    return flat
      ? [p0.x, p0.y, p0.x + dx * 0.5, p0.y, p1.x - dx * 0.5, p1.y, p1.x, p1.y]
      : [p0.x, p0.y, p0.x, p0.y + dy * 0.5, p1.x, p1.y - dy * 0.5, p1.x, p1.y];
  }

  function at(p, t) {
    var m = 1 - t, a = m * m * m, b = 3 * m * m * t, c = 3 * m * t * t, d = t * t * t;
    return { x: a * p[0] + b * p[2] + c * p[4] + d * p[6], y: a * p[1] + b * p[3] + c * p[5] + d * p[7] };
  }
  function tangent(p, t) {
    var m = 1 - t;
    var x = 3 * m * m * (p[2] - p[0]) + 6 * m * t * (p[4] - p[2]) + 3 * t * t * (p[6] - p[4]);
    var y = 3 * m * m * (p[3] - p[1]) + 6 * m * t * (p[5] - p[3]) + 3 * t * t * (p[7] - p[5]);
    var l = Math.hypot(x, y) || 1;
    return { x: x / l, y: y / l };
  }

  function build() {
    var box = wrap.getBoundingClientRect();
    W = box.width; H = box.height;
    cv.width = Math.round(W * DPR); cv.height = Math.round(H * DPR);
    ctx.setTransform(DPR, 0, 0, DPR, 0, 0);

    var coreEl = wrap.querySelector('[data-core]');
    var vaultEl = wrap.querySelector('[data-vault]');
    if (!coreEl || !vaultEl) return;
    core = centre(coreEl, box);
    var vault = centre(vaultEl, box);

    streams = [];
    var srcs = wrap.querySelectorAll('.node.src');
    for (var i = 0; i < srcs.length; i++) {
      var s = centre(srcs[i], box);
      streams.push({ p: path(s, core), hue: srcs[i].getAttribute('data-hue') || '#5a51e5', into: 'core', amp: 13 + (i % 3) * 7, every: 0.26 + Math.random() * 0.26, t: Math.random() });
    }
    streams.push({ p: path(core, vault), hue: '#5a51e5', into: 'vault', amp: 0, every: 0.2, t: 0, thick: true });
    var outs = wrap.querySelectorAll('.node.out');
    for (var j = 0; j < outs.length; j++) {
      var o = centre(outs[j], box);
      streams.push({ p: path(vault, o), hue: outs[j].getAttribute('data-hue') || '#5a51e5', into: 'out', amp: 6, every: 0.34, t: Math.random() });
    }
    parts = [];
    seed();
    for (var k = 0; k < srcs.length; k++) srcs[k].style.setProperty('--dot', srcs[k].getAttribute('data-hue'));
  }

  function spawn(s, t0) {
    s.wave = (s.wave || 0) + 0.9;
    parts.push({ s: s, t: t0 || 0, sp: 0.19 + Math.random() * 0.1, r: 2.1 + Math.random() * 1.7, ph: s.wave, a: 0.66 + Math.random() * 0.34 });
  }
  function seed() {                              // the streams are already running when you arrive
    for (var i = 0; i < streams.length; i++) for (var j = 1; j <= 7; j++) spawn(streams[i], j / 8 + Math.random() * 0.05);
  }

  function draw(dt) {
    ctx.clearRect(0, 0, W, H);

    for (var i = 0; i < streams.length; i++) {                 // the lines themselves, quietly
      var s = streams[i];
      ctx.strokeStyle = s.thick ? 'rgba(90,81,229,.3)' : s.hue;
      ctx.globalAlpha = s.thick ? 1 : 0.16;
      ctx.lineWidth = s.thick ? 1.6 : 1.1;
      ctx.beginPath(); ctx.moveTo(s.p[0], s.p[1]);
      ctx.bezierCurveTo(s.p[2], s.p[3], s.p[4], s.p[5], s.p[6], s.p[7]);
      ctx.stroke();
      ctx.globalAlpha = 1;
      if (dt) { s.t -= dt; if (s.t <= 0) { s.t = s.every; spawn(s); } }
    }

    for (var k = parts.length - 1; k >= 0; k--) {
      var p = parts[k], st = p.s;
      if (dt) p.t += p.sp * dt;
      if (p.t >= 1) {
        if (st.into === 'core') corePulse = 1; else if (st.into === 'vault') vaultPulse = 1;
        parts.splice(k, 1); continue;
      }
      var pos = at(st.p, p.t), tg = tangent(st.p, p.t);
      // a wave that flattens as it arrives: chaos on the way in, order at the model
      var sway = Math.sin(p.t * 5.2 + p.ph) * st.amp * (1 - p.t * 0.85) * (1 - p.t * 0.85);
      var x = pos.x - tg.y * sway, y = pos.y + tg.x * sway;
      var fade = Math.min(1, p.t * 6) * Math.min(1, (1 - p.t) * 7);
      ctx.fillStyle = st.hue;
      ctx.globalAlpha = p.a * fade;
      ctx.beginPath(); ctx.arc(x, y, p.r, 0, 6.2832); ctx.fill();
      ctx.globalAlpha = p.a * fade * 0.28;                     // a short comet tail
      var back = at(st.p, Math.max(0, p.t - 0.035));
      ctx.beginPath(); ctx.arc(back.x - tg.y * sway, back.y + tg.x * sway, p.r * 0.7, 0, 6.2832); ctx.fill();
      ctx.globalAlpha = 1;
    }

    if (core && corePulse > 0.01) {                            // the model lights when something lands
      var g = ctx.createRadialGradient(core.x, core.y, 0, core.x, core.y, 130);
      g.addColorStop(0, 'rgba(90,81,229,' + (corePulse * 0.15).toFixed(3) + ')');
      g.addColorStop(1, 'rgba(90,81,229,0)');
      ctx.fillStyle = g;
      ctx.beginPath(); ctx.arc(core.x, core.y, 130, 0, 6.2832); ctx.fill();
    }
    if (dt) { corePulse *= 0.94; vaultPulse *= 0.94; }
  }

  var last = 0;
  function frame(ts) {
    if (!running) return;
    var dt = last ? Math.min(0.05, (ts - last) / 1000) : 0.016;
    last = ts;
    draw(dt);
    requestAnimationFrame(frame);
  }

  function start() {
    build();
    if (reduced) { draw(0); return; }
    draw(0);
    requestAnimationFrame(frame);
  }

  if (document.readyState === 'complete') start();
  else window.addEventListener('load', start);

  var rt;
  window.addEventListener('resize', function () { clearTimeout(rt); rt = setTimeout(function () { build(); draw(0); }, 200); });

  if (!reduced && 'IntersectionObserver' in window) {
    new IntersectionObserver(function (es) {
      if (!es[0].isIntersecting) { running = false; }
      else if (!running) { running = true; last = 0; requestAnimationFrame(frame); }
    }, { threshold: 0.02 }).observe(wrap);
  }
  document.addEventListener('visibilitychange', function () {
    if (document.hidden) { running = false; }
    else if (!running && !reduced) { running = true; last = 0; requestAnimationFrame(frame); }
  });
})();
