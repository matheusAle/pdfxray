// pdfxray landing page. CSS scroll-driven animations do the work where supported;
// this file splits text for staggered reveals and provides the fallback.
(() => {
  const root = document.documentElement;
  const supported = CSS.supports('(animation-timeline: view()) and (animation-range: entry)');
  const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const fallback = !supported || reduce;
  if (fallback) root.classList.add('no-sda');

  // Words of the manifesto, each with its position for a staggered reveal.
  for (const el of document.querySelectorAll('[data-words]')) {
    const words = el.textContent.trim().split(/\s+/);
    el.textContent = '';
    words.forEach((w, i) => {
      const span = document.createElement('span');
      span.textContent = w;
      span.style.setProperty('--i', i);
      el.append(span, ' ');
    });
    el.style.setProperty('--n', words.length);
    el.setAttribute('aria-label', words.join(' '));
  }

  // Terminal lines type in one after another.
  document.querySelectorAll('.term-body .l').forEach((l, i) => l.style.setProperty('--i', i));

  // Showcase camera: each spot's centre becomes a stop the screenshot pans to at ZOOM,
  // clamped so the screenshot's edges never show.
  const ZOOM = 1.6;
  const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
  const stops = new Map();
  for (const track of document.querySelectorAll('.chapter .track')) {
    const list = [...track.querySelectorAll('.spot')].map((spot, i) => {
      const v = (k) => parseFloat(spot.style.getPropertyValue(`--${k}`)) / 100;
      const r = { l: v('l'), t: v('t'), w: v('w'), h: v('h') };
      r.tx = clamp(0.5 - (r.l + r.w / 2) * ZOOM, 1 - ZOOM, 0);
      r.ty = clamp(0.5 - (r.t + r.h / 2) * ZOOM, 1 - ZOOM, 0);
      track.style.setProperty(`--x${i + 1}`, `${(r.tx * 100).toFixed(2)}%`);
      track.style.setProperty(`--y${i + 1}`, `${(r.ty * 100).toFixed(2)}%`);
      return r;
    });
    stops.set(track.closest('.chapter'), list);
  }

  // Caption cards sit beside their spot as it appears at its stop: right, then left, then below,
  // then above, 28 px away and never over it.
  const GAP = 28;
  const EDGE = 16;
  const placeCaptions = () => {
    for (const [chapter, list] of stops) {
      const stage = chapter.querySelector('.stage').getBoundingClientRect();
      const W = stage.width;
      const H = stage.height;
      if (!W) continue;
      const ptr = chapter.querySelector('.ptr');
      list.forEach((r, i) => {
        const cap = chapter.querySelector(`.cap.k${i + 1}`);
        const cw = cap.offsetWidth;
        const ch = cap.offsetHeight;
        const sx = (r.l * ZOOM + r.tx) * W;
        const sy = (r.t * ZOOM + r.ty) * H;
        const sw = r.w * ZOOM * W;
        const sh = r.h * ZOOM * H;
        const midY = clamp(sy + sh / 2 - 40, EDGE, H - ch - EDGE);
        const atX = clamp(sx, EDGE, W - cw - EDGE);
        let side, x, y;
        if (W - sx - sw >= cw + GAP + EDGE) [side, x, y] = ['r', sx + sw + GAP, midY];
        else if (sx >= cw + GAP + EDGE) [side, x, y] = ['l', sx - GAP - cw, midY];
        else if (H - sy - sh >= ch + GAP + EDGE) [side, x, y] = ['b', atX, sy + sh + GAP];
        else [side, x, y] = ['a', atX, Math.max(EDGE, sy - GAP - ch)];
        cap.dataset.side = side;
        cap.style.setProperty('--cx', `${Math.round(x)}px`);
        cap.style.setProperty('--cy', `${Math.round(y)}px`);
        // The demo pointer clicks the middle of the spot.
        ptr?.style.setProperty(`--px${i + 1}`, `${Math.round(sx + sw / 2)}px`);
        ptr?.style.setProperty(`--py${i + 1}`, `${Math.round(sy + sh / 2)}px`);
      });
      ptr?.style.setProperty('--px0', `${Math.round(W * 0.86)}px`);
      ptr?.style.setProperty('--py0', `${Math.round(H * 1.04)}px`);
    }
  };
  placeCaptions();
  addEventListener('resize', placeCaptions);
  document.fonts?.ready.then(placeCaptions);

  // Hero sequence: the 1440×900 artboard scales to fit the viewport (--fit); the static hero
  // shows a 760 px wide crop of the couch scene (--fs). On phones a camera frames each state
  // (couch + notes, the page, cards 1–3, the glyph) in the space above the copy panel.
  const seq = document.querySelector('.seq');
  const CAMERA = [[700, 90, 1380, 860], [790, 100, 1350, 780], [60, 500, 1060, 900], [70, 300, 800, 880]];
  const sizeSeq = () => {
    if (!seq) return;
    seq.style.setProperty('--fit', Math.min(innerWidth / 1440, innerHeight / 900).toFixed(4));
    seq.style.setProperty('--fs', (seq.querySelector('.scene-box').clientWidth / 760).toFixed(4));
    if (innerWidth > 960) return;
    // Portrait: the camera box sits above the copy panel. Landscape phones: it takes the right half.
    const landscape = innerHeight <= 520 && innerWidth > innerHeight;
    const panel = Math.max(...[...seq.querySelectorAll('.hcopy')].map((c) => c.offsetHeight));
    const top = landscape ? 64 : 76;
    const left = landscape ? Math.round(innerWidth * 0.46) + 32 : 16;
    const W = innerWidth - left - 16;
    const H = landscape ? innerHeight - top - 12 : innerHeight - panel - 28 - 24 - top;
    CAMERA.forEach(([x0, y0, x1, y1], i) => {
      const k = Math.min(W / (x1 - x0), H / (y1 - y0));
      seq.style.setProperty(`--c${i + 1}s`, k.toFixed(4));
      seq.style.setProperty(`--c${i + 1}x`, `${(left + (W - (x1 - x0) * k) / 2 - x0 * k).toFixed(1)}px`);
      seq.style.setProperty(`--c${i + 1}y`, `${(top + (H - (y1 - y0) * k) / 2 - y0 * k).toFixed(1)}px`);
    });
  };
  sizeSeq();
  addEventListener('resize', sizeSeq);
  document.fonts?.ready.then(sizeSeq);

  // Sparks: the light around a spot only runs while its chapter is on screen.
  const live = new IntersectionObserver((entries) => {
    for (const e of entries) e.target.classList.toggle('live', e.isIntersecting);
  });
  document.querySelectorAll('.chapter').forEach((c) => live.observe(c));

  // Sparks: cards glow where the pointer is.
  if (matchMedia('(hover: hover) and (pointer: fine)').matches) {
    for (const card of document.querySelectorAll('.card')) {
      card.addEventListener('pointermove', (e) => {
        const r = card.getBoundingClientRect();
        card.style.setProperty('--mx', `${Math.round(e.clientX - r.left)}px`);
        card.style.setProperty('--my', `${Math.round(e.clientY - r.top)}px`);
      });
    }
  }

  // Sparks: the facts count up once, and the terminal types its command, when they come into view.
  if (!reduce) {
    const once = (el, fn, margin = '0px 0px -20% 0px') => {
      const io = new IntersectionObserver(([e]) => { if (e.isIntersecting || e.boundingClientRect.top < 0) { io.disconnect(); fn(); } }, { rootMargin: margin });
      io.observe(el);
    };
    for (const b of document.querySelectorAll('.facts b')) {
      const end = parseInt(b.textContent.replace(/,/g, ''), 10);
      if (Number.isNaN(end)) continue;
      if (end === 0) { once(b, () => b.classList.add('pulse')); continue; }
      b.textContent = '0';
      once(b, () => {
        const t0 = performance.now();
        const step = (t) => {
          const k = Math.min(1, (t - t0) / 900);
          b.textContent = Math.round(end * (1 - (1 - k) ** 3)).toLocaleString('en-US');
          if (k < 1) requestAnimationFrame(step);
        };
        requestAnimationFrame(step);
      });
    }
    const cmd = document.querySelector('.term-body .l');
    const text = cmd?.lastChild;
    if (text?.nodeType === Node.TEXT_NODE) {
      const full = text.textContent;
      const caret = document.createElement('span');
      caret.className = 'caret';
      caret.setAttribute('aria-hidden', 'true');
      text.textContent = '';
      cmd.append(caret);
      once(cmd, () => {
        let i = 0;
        const timer = setInterval(() => {
          text.textContent = full.slice(0, ++i);
          if (i >= full.length) clearInterval(timer);
        }, 28);
      }, '0px 0px -30% 0px');
    }
  }

  // Inertial wheel scrolling: wheel notches glide instead of jumping, so every scroll-linked
  // animation moves continuously too. Touch, keyboard, scrollbar and anchor jumps stay native.
  if (!reduce) {
    let target = scrollY;
    let current = scrollY;
    let running = false;
    let last = 0;
    const maxScroll = () => root.scrollHeight - innerHeight;
    const scrollsItself = (el) => {
      for (; el && el !== document.body; el = el.parentElement) {
        const oy = getComputedStyle(el).overflowY;
        if ((oy === 'auto' || oy === 'scroll') && el.scrollHeight > el.clientHeight) return true;
      }
      return false;
    };
    const tick = (t) => {
      const dt = Math.min(64, t - (last || t));
      last = t;
      current += (target - current) * (1 - Math.exp(-dt / 90));
      if (Math.abs(target - current) < 0.5) current = target;
      scrollTo({ top: current, behavior: 'instant' });
      if (current !== target) requestAnimationFrame(tick);
      else { running = false; last = 0; }
    };
    addEventListener('wheel', (e) => {
      if (e.ctrlKey || Math.abs(e.deltaX) > Math.abs(e.deltaY) || scrollsItself(e.target)) return;
      e.preventDefault();
      if (!running) { target = current = scrollY; }
      const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? innerHeight : 1;
      target = clamp(target + e.deltaY * unit, 0, maxScroll());
      if (!running) { running = true; requestAnimationFrame(tick); }
    }, { passive: false });
    // Keep in sync when something else scrolls the page (keys, scrollbar, anchor links).
    addEventListener('scroll', () => { if (!running) target = current = scrollY; }, { passive: true });
  }

  if (!fallback) return;

  // Hero sequence and showcase without scroll timelines: drive their paused animations through --p,
  // the progress across each pinned span.
  const chapters = [...document.querySelectorAll('.seq, .chapter')];
  if (!reduce && chapters.length) {
    let queued = false;
    const update = () => {
      queued = false;
      for (const ch of chapters) {
        const r = ch.getBoundingClientRect();
        if (r.bottom < -innerHeight || r.top > 2 * innerHeight) continue;
        ch.style.setProperty('--p', clamp(-r.top / (r.height - innerHeight), 0, 1).toFixed(4));
      }
    };
    const queue = () => { if (!queued) { queued = true; requestAnimationFrame(update); } };
    addEventListener('scroll', queue, { passive: true });
    addEventListener('resize', queue);
    update();
  }

  // Generic reveals.
  const rio = new IntersectionObserver((entries) => {
    for (const e of entries) if (e.isIntersecting) { e.target.classList.add('in'); rio.unobserve(e.target); }
  }, { rootMargin: '0px 0px -10% 0px' });
  document.querySelectorAll('.reveal').forEach((el) => rio.observe(el));
})();
