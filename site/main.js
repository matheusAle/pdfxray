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
  document.querySelectorAll('.term-body').forEach((pane) => pane.querySelectorAll('.l').forEach((l, i) => l.style.setProperty('--i', i)));

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
    // Each pane types its command once it is shown; a tab switch replays the new pane's command.
    const typers = new Map();
    for (const pane of document.querySelectorAll('.term-body')) {
      const cmd = pane.querySelector('.l');
      const text = cmd?.lastChild;
      if (text?.nodeType !== Node.TEXT_NODE) continue;
      const full = text.textContent;
      const caret = document.createElement('span');
      caret.className = 'caret';
      caret.setAttribute('aria-hidden', 'true');
      cmd.append(caret);
      let timer;
      typers.set(pane, () => {
        clearInterval(timer);
        let i = 0;
        text.textContent = '';
        timer = setInterval(() => {
          text.textContent = full.slice(0, ++i);
          if (i >= full.length) clearInterval(timer);
        }, 28);
      });
      text.textContent = '';
    }
    const term = document.querySelector('.term');
    if (term) once(term, () => { term.dataset.seen = '1'; typers.get(term.querySelector('.term-body:not([hidden])'))?.(); }, '0px 0px -30% 0px');
    window.__typeTerm = (pane) => { if (term) term.dataset.seen = '1'; typers.get(pane)?.(); };
  }

  // CLI tabs: a real tablist (arrow keys move between tabs).
  const tabs = [...document.querySelectorAll('.tabs [role="tab"]')];
  const pick = (tab) => {
    for (const t of tabs) {
      const on = t === tab;
      t.setAttribute('aria-selected', on);
      t.tabIndex = on ? 0 : -1;
      document.getElementById(t.getAttribute('aria-controls')).hidden = !on;
    }
    window.__typeTerm?.(document.getElementById(tab.getAttribute('aria-controls')));
  };
  tabs.forEach((t, i) => {
    t.addEventListener('click', () => pick(t));
    t.addEventListener('keydown', (e) => {
      const d = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
      if (!d) return;
      e.preventDefault();
      const next = tabs[(i + d + tabs.length) % tabs.length];
      next.focus();
      pick(next);
    });
  });

  // Copy buttons: copy, show a check for a moment, and announce it.
  const announcer = document.getElementById('live');
  for (const btn of document.querySelectorAll('.copy-btn')) {
    const label = btn.getAttribute('aria-label');
    btn.addEventListener('click', async () => {
      try { await navigator.clipboard.writeText(btn.dataset.copy); } catch { return; }
      btn.classList.add('copied');
      btn.setAttribute('aria-label', 'Copied');
      if (announcer) announcer.textContent = 'Copied: ' + btn.dataset.copy;
      setTimeout(() => { btn.classList.remove('copied'); btn.setAttribute('aria-label', label); }, 1600);
    });
  }
  // A command too long for its box fades at the right edge, the cue that it scrolls.
  const fit = () => document.querySelectorAll('.cmd').forEach((c) => { const k = c.querySelector('code'); c.classList.toggle('over', k.scrollWidth > k.clientWidth + 1); });
  fit();
  addEventListener('resize', fit);

  // Windows viewers and screens: the popover's controls swap in pdfxray's real renders of the test page.
  const emu = document.querySelector('.emu-demo');
  if (emu) {
    // Measured from the renders: the 0.4 pt rule's pixel rows (ink %), and where the narrow run ends (px).
    const M = {
      96: { size: '266 × 106', rule: { pdfium: [30, 70], acrobat: [100] }, run: { pdfium: 136, acrobat: 127 } },
      120: { size: '333 × 133', rule: { pdfium: [50, 50], acrobat: [100] }, run: { pdfium: 170, acrobat: 186 } },
      144: { size: '400 × 160', rule: { pdfium: [70, 30], acrobat: [100] }, run: { pdfium: 204, acrobat: 191 } },
    };
    const NAME = { pdfxray: 'pdfxray', chrome: 'Chrome', edge: 'Edge', acrobat: 'Acrobat' };
    const WHY = {
      pdfxray: 'pdfxray’s own rounding, on the emulated screen’s pixels.',
      pdfium: 'PDFium: each glyph starts on a whole pixel, rectangular clips grow out to whole pixels, upright images cover a whole number of pixels. No snapping, no fill overlap, no stroke adjustment.',
      acrobat: 'Adobe’s engine: coordinates within 0.0001 px snap to the pixel, fills overlap 0.01 px, every stroke is a whole number of pixels (at least one), and each glyph advance is rounded, so long runs drift.',
    };
    const state = { viewer: 'acrobat', dpi: '120' };
    const $ = (id) => document.getElementById(id);
    const render = () => {
      const { viewer, dpi } = state;
      const m = M[dpi];
      const eng = viewer === 'acrobat' ? 'acrobat' : 'pdfium';
      const pct = { 96: 100, 120: 125, 144: 150 }[dpi];
      const img = $('emu-img');
      img.src = `img/emu/${viewer}-${dpi}.png?v=1`;
      img.alt = `Part of a test page drawn as ${NAME[viewer]} on a ${pct} % screen, magnified so each device pixel shows`;
      $('emu-read').textContent = `${(dpi / 72).toFixed(3)} device px per pt · page ${m.size} px`;
      $('emu-why').textContent = WHY[viewer === 'pdfxray' ? 'pdfxray' : eng];
      $('emu-sb').textContent = viewer === 'pdfxray' ? `pdfxray · 1920×1080 · ${dpi} DPI · 100%` : `${NAME[viewer]} · 1920×1080 · ${dpi} DPI · 100%`;
      const rows = m.rule[eng];
      $('emu-rule').textContent = rows.length === 1
        ? 'One solid black pixel row: Acrobat makes every stroke a whole number of pixels.'
        : `Two grey pixel rows, ${rows[0]} % and ${rows[1]} % ink: the line lands between pixels and nothing moves it.`;
      const d = m.run.acrobat - m.run.pdfium;
      $('emu-run').textContent = eng === 'acrobat'
        ? `Ends ${Math.abs(d)} px ${d > 0 ? 'later' : 'earlier'} than in Chrome: each advance is rounded, and the error adds up.`
        : `Ends ${m.run.pdfium} px in, where its advances add up to. The same run ends ${Math.abs(d)} px ${d > 0 ? 'later' : 'earlier'} in Acrobat.`;
      for (const g of emu.querySelectorAll('[data-emu]')) {
        for (const b of g.querySelectorAll('button')) {
          const on = b.dataset.v === state[g.dataset.emu];
          b.setAttribute('aria-checked', on);
          b.tabIndex = on ? 0 : -1;
        }
      }
    };
    // The facts are announced only for the reader's own changes, never for the automatic flip.
    let touched = false;
    const stop = () => { touched = true; emu.querySelector('.emu-facts').setAttribute('aria-live', 'polite'); };
    emu.addEventListener('pointerenter', stop);
    emu.addEventListener('focusin', stop);
    for (const g of emu.querySelectorAll('[data-emu]')) {
      const btns = [...g.querySelectorAll('button')];
      btns.forEach((b, i) => {
        b.addEventListener('click', () => { stop(); state[g.dataset.emu] = b.dataset.v; render(); });
        b.addEventListener('keydown', (e) => {
          const d = ['ArrowRight', 'ArrowDown'].includes(e.key) ? 1 : ['ArrowLeft', 'ArrowUp'].includes(e.key) ? -1 : 0;
          if (!d) return;
          e.preventDefault();
          const n = btns[(i + d + btns.length) % btns.length];
          n.focus(); n.click();
        });
      });
    }
    for (const v of ['pdfxray', 'chrome', 'edge', 'acrobat']) for (const d of [96, 120, 144]) { const im = new Image(); im.src = `img/emu/${v}-${d}.png?v=1`; }
    render();
    // While on screen and untouched, flip between Chrome and Acrobat so the difference shows itself.
    if (!reduce) {
      let t;
      new IntersectionObserver(([e]) => {
        clearInterval(t);
        if (e.isIntersecting) t = setInterval(() => { if (touched) return clearInterval(t); state.viewer = state.viewer === 'acrobat' ? 'chrome' : 'acrobat'; render(); }, 2200);
      }, { threshold: 0.5 }).observe(emu);
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
