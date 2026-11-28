// Вторичные представления: Баланс слоёв (A/I) с раздвижением подписей, Карта кода (Treemap), Проблемы и Git.

const ArchCharts = (() => {
  const CLUSTER_PALETTE = [
    '#5f5f5f', '#2ba471', '#e37318', '#366ef4', '#6d28d9',
    '#d54941', '#0d9488', '#8b8b8b', '#a75d2b', '#4582e6',
  ];

  function cssVar(name) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  function zoneBadge(zone, activePain = false) {
    if (activePain) {
      return `<span class="badge b-pain" data-tip="Много входящих связей, мало абстракций и высокая частота правок в Git — изменения здесь рискованны">Зона риска</span>`;
    }
    const map = {
      main_sequence: ['Сбалансирован', 'b-main', 'Компонент близок к линии баланса A + I = 1 (D ≤ 0.30)'],
      zone_of_pain: ['Стабильное ядро', 'b-calm', 'Много входящих связей и конкретные типы, но редко меняется в Git — обычно безопасно (DTO, домен, утилиты)'],
      zone_of_uselessness: ['Лишние абстракции', 'b-useless', 'Высокая доля интерфейсов/трейтов, но на модуль почти никто не ссылается'],
      isolated: ['Изолирован', 'b-muted', 'Нет внутренних входящих и исходящих связей'],
      no_entities: ['Без типов', 'b-muted', 'Нет публичных классов, структур, трейтов или функций'],
    };
    const [label, cls, tip] = map[zone] || [zone, 'b-muted', ''];
    return `<span class="badge ${cls}"${tip ? ` data-tip="${ArchModel.esc(tip)}"` : ''}>${label}</span>`;
  }

  function nodeColorMainSeq(n) {
    if (n.in_cycle || n.active_pain) return cssVar('--td-error-color');
    if (n.zone === 'zone_of_pain') return cssVar('--td-warning-color');
    if (n.zone === 'zone_of_uselessness') return cssVar('--td-purple-color');
    if (n.zone === 'main_sequence') return cssVar('--td-success-color');
    return cssVar('--td-text-color-placeholder');
  }

  // =========================================================================
  // 1. Баланс слоёв (A / I) с честным раздвижением пересекающихся подписей
  // =========================================================================
  function renderMainSeq(report, view, filteredNodes, appState, callbacks) {
    const svg = document.getElementById('ai-svg');
    if (!svg) return;
    const W = 920, H = 600;
    svg.setAttribute('viewBox', `0 0 ${W} ${H}`);
    const pad = { l: 74, r: 44, t: 30, b: 64 };
    const pw = W - pad.l - pad.r;
    const ph = H - pad.t - pad.b;
    const band = report.main_sequence_band || 0.30;

    const sx = i => pad.l + i * pw;
    const sy = a => pad.t + (1 - a) * ph;

    const plotted = filteredNodes.filter(n => n.instability != null && n.abstractness != null);
    const unplotted = filteredNodes.filter(n => n.instability == null || n.abstractness == null);
    const hint = document.getElementById('ai-count-hint');
    if (hint) {
      hint.textContent = `На графике: ${plotted.length} из ${filteredNodes.length} (вне графика: ${unplotted.length})`;
    }

    const sizeVal = n =>
      appState.sizeMode === 'files'
        ? n.file_count
        : appState.sizeMode === 'coupling'
        ? n.afferent_coupling + n.efferent_coupling
        : appState.sizeMode === 'commits'
        ? Math.max(1, n.commits || 0)
        : n.loc;
    const maxSizeVal = Math.max(1, ...plotted.map(sizeVal));

    const borderCol = cssVar('--td-component-stroke');
    const mutedCol = cssVar('--td-text-color-secondary');
    const textCol = cssVar('--td-text-color-primary');
    const errCol = cssVar('--td-error-color');
    const purpCol = cssVar('--td-purple-color');
    const okCol = cssVar('--td-success-color');
    const brandCol = cssVar('--td-brand-color');
    const surfCol = cssVar('--td-bg-color-container');
    const pageCol = cssVar('--td-bg-color-page');
    const parts = [];

    parts.push(`<rect x="${pad.l}" y="${pad.t}" width="${pw}" height="${ph}" fill="${pageCol}" rx="6" />`);
    const pCut = Math.max(0, 1 - band);
    const uCut = Math.min(1, band);
    parts.push(`<polygon points="${sx(0)},${sy(0)} ${sx(pCut)},${sy(0)} ${sx(0)},${sy(pCut)}" fill="${errCol}" fill-opacity="0.08" />`);
    parts.push(`<polygon points="${sx(1)},${sy(1)} ${sx(uCut)},${sy(1)} ${sx(1)},${sy(uCut)}" fill="${purpCol}" fill-opacity="0.09" />`);
    parts.push(`<polygon points="${sx(0)},${sy(pCut)} ${sx(0)},${sy(1)} ${sx(uCut)},${sy(1)} ${sx(1)},${sy(uCut)} ${sx(1)},${sy(0)} ${sx(pCut)},${sy(0)}" fill="${okCol}" fill-opacity="0.06" />`);

    for (let k = 0; k <= 5; k++) {
      const v = k / 5;
      const gx = sx(v), gy = sy(v);
      parts.push(`<line x1="${gx}" y1="${pad.t}" x2="${gx}" y2="${pad.t + ph}" stroke="${borderCol}" stroke-width="1" />`);
      parts.push(`<line x1="${pad.l}" y1="${gy}" x2="${pad.l + pw}" y2="${gy}" stroke="${borderCol}" stroke-width="1" />`);
      parts.push(`<text x="${gx}" y="${pad.t + ph + 22}" text-anchor="middle" fill="${mutedCol}" font-size="12.5">${v.toFixed(1)}</text>`);
      parts.push(`<text x="${pad.l - 12}" y="${gy + 4}" text-anchor="end" fill="${mutedCol}" font-size="12.5">${v.toFixed(1)}</text>`);
    }

    parts.push(`<line x1="${sx(0)}" y1="${sy(1)}" x2="${sx(1)}" y2="${sy(0)}" stroke="${okCol}" stroke-width="2" stroke-dasharray="6 5" />`);
    parts.push(`<line x1="${sx(0)}" y1="${sy(pCut)}" x2="${sx(pCut)}" y2="${sy(0)}" stroke="${errCol}" stroke-opacity="0.35" stroke-dasharray="3 4" />`);
    parts.push(`<line x1="${sx(uCut)}" y1="${sy(1)}" x2="${sx(1)}" y2="${sy(uCut)}" stroke="${purpCol}" stroke-opacity="0.35" stroke-dasharray="3 4" />`);

    // Размещаем названия зон так, чтобы они не мешали точкам вдоль A = 0
    parts.push(`<text x="${sx(0.03)}" y="${sy(0.46)}" fill="${errCol}" font-size="13" font-weight="700" opacity="0.82">Зона риска (много входящих, мало абстракций)</text>`);
    parts.push(`<text x="${sx(0.97)}" y="${sy(0.94)}" text-anchor="end" fill="${purpCol}" font-size="13" font-weight="700" opacity="0.82">Лишние абстракции</text>`);
    parts.push(`<text x="${sx(0.50)}" y="${sy(0.52)}" fill="${okCol}" font-size="12.5" font-weight="600" transform="rotate(32, ${sx(0.50)}, ${sy(0.52)})">Линия баланса (A + I = 1)</text>`);
    parts.push(`<text x="${pad.l + pw / 2}" y="${H - 14}" text-anchor="middle" fill="${textCol}" font-size="13" font-weight="600">Нестабильность I  (0 = стабильное ядро, 1 = верхний слой)</text>`);
    parts.push(`<text x="20" y="${pad.t + ph / 2}" text-anchor="middle" fill="${textCol}" font-size="13" font-weight="600" transform="rotate(-90, 20, ${pad.t + ph / 2})">Абстрактность A  (доля интерфейсов и трейтов)</text>`);

    // Разводим совпадающие точки по аккуратной спирали
    const buckets = new Map();
    plotted.forEach(n => {
      const key = `${n.instability.toFixed(2)}_${n.abstractness.toFixed(2)}`;
      if (!buckets.has(key)) buckets.set(key, []);
      buckets.get(key).push(n);
    });

    const positioned = [];
    buckets.forEach(group => {
      group.forEach((n, idx) => {
        let cx = sx(n.instability);
        let cy = sy(n.abstractness);
        if (group.length > 1) {
          const angle = idx * 2.39996 - Math.PI / 2;
          const dist = Math.min(28, 7 + Math.sqrt(idx) * 8);
          cx = Math.max(pad.l + 14, Math.min(pad.l + pw - 14, cx + Math.cos(angle) * dist));
          cy = Math.max(pad.t + 14, Math.min(pad.t + ph - 14, cy + Math.sin(angle) * dist));
        }
        const r = 6 + Math.sqrt(sizeVal(n) / maxSizeVal) * 13;
        positioned.push({ n, cx, cy, r });
      });
    });
    positioned.sort((a, b) => b.r - a.r);

    const selViewId = (appState.selectedNodeId || '').replace(/^(pkg|mod|file):/, '');
    if (selViewId) {
      const posMap = new Map(positioned.map(p => [p.n.id, p]));
      const selPos = posMap.get(selViewId);
      if (selPos) {
        view.edges.forEach(e => {
          if (e.from === selViewId && posMap.has(e.to)) {
            const t = posMap.get(e.to);
            parts.push(`<line x1="${selPos.cx}" y1="${selPos.cy}" x2="${t.cx}" y2="${t.cy}" stroke="${e.is_cycle_edge ? errCol : brandCol}" stroke-width="1.6" stroke-opacity="0.6" />`);
          } else if (e.to === selViewId && posMap.has(e.from)) {
            const s = posMap.get(e.from);
            parts.push(`<line x1="${s.cx}" y1="${s.cy}" x2="${selPos.cx}" y2="${selPos.cy}" stroke="${e.is_cycle_edge ? errCol : okCol}" stroke-width="1.6" stroke-opacity="0.6" />`);
          }
        });
      }
    }

    positioned.forEach(({ n, cx, cy, r }) => {
      const col = nodeColorMainSeq(n);
      const isSel = n.id === selViewId;
      if (n.active_pain) {
        parts.push(`<circle cx="${cx.toFixed(1)}" cy="${cy.toFixed(1)}" r="${(r + 4.5).toFixed(1)}" fill="none" stroke="${errCol}" stroke-width="1.6" stroke-opacity="0.55" stroke-dasharray="3 2" />`);
      }
      parts.push(
        `<circle class="ai-node" data-id="${ArchModel.esc(n.id)}" cx="${cx.toFixed(1)}" cy="${cy.toFixed(1)}" r="${r.toFixed(1)}" ` +
        `fill="${col}" fill-opacity="${isSel ? 0.96 : 0.82}" stroke="${isSel ? textCol : surfCol}" stroke-width="${isSel ? 2.6 : 1.5}" style="cursor:pointer" />`
      );
    });

    // Решатель коллизий для подписей: никаких наложений и выхода за правый край!
    if (appState.showLabels && positioned.length <= 80) {
      const labelLimit = positioned.length <= 30 ? positioned.length : 22;
      const toLabel = [...positioned]
        .sort((a, b) => {
          if (a.n.id === selViewId) return -1;
          if (b.n.id === selViewId) return 1;
          return Number(b.n.active_pain) - Number(a.n.active_pain) || (b.n.distance || 0) - (a.n.distance || 0) || b.r - a.r;
        })
        .slice(0, labelLimit)
        .map(p => {
          const shortName = p.n.id.length > 22 ? '…' + p.n.id.slice(-21) : p.n.id;
          const lw = shortName.length * 7.2 + 8;
          const lh = 17;
          const flipLeft = p.cx > pad.l + pw * 0.68;
          const lx = flipLeft ? p.cx - p.r - lw - 6 : p.cx + p.r + 6;
          const ly = p.cy - lh / 2 - 4;
          return { ...p, shortName, lw, lh, lx, ly };
        });

      // 30 итераций мягкого расталкивания прямоугольников подписей
      for (let iter = 0; iter < 30; iter++) {
        for (let i = 0; i < toLabel.length; i++) {
          for (let j = i + 1; j < toLabel.length; j++) {
            const a = toLabel[i], b = toLabel[j];
            const ox = Math.min(a.lx + a.lw, b.lx + b.lw) - Math.max(a.lx, b.lx);
            const oy = Math.min(a.ly + a.lh, b.ly + b.lh) - Math.max(a.ly, b.ly);
            if (ox > 0 && oy > 0) {
              const pushY = (oy / 2 + 2) * (a.ly <= b.ly ? -1 : 1);
              a.ly += pushY;
              b.ly -= pushY;
            }
          }
          // Зажимаем строго внутри области графика
          const it = toLabel[i];
          it.lx = Math.max(pad.l + 4, Math.min(pad.l + pw - it.lw - 4, it.lx));
          it.ly = Math.max(pad.t + 4, Math.min(pad.t + ph - it.lh - 4, it.ly));
        }
      }

      for (const it of toLabel) {
        const anchorX = it.lx + it.lw / 2;
        const anchorY = it.ly + it.lh / 2;
        if (Math.hypot(anchorX - it.cx, anchorY - it.cy) > it.r + 16) {
          parts.push(
            `<line x1="${it.cx.toFixed(1)}" y1="${it.cy.toFixed(1)}" x2="${(it.lx < it.cx ? it.lx + it.lw : it.lx).toFixed(1)}" y2="${anchorY.toFixed(1)}" stroke="${mutedCol}" stroke-opacity="0.45" stroke-width="1" style="pointer-events:none" />`
          );
        }
        parts.push(
          `<rect x="${it.lx.toFixed(1)}" y="${it.ly.toFixed(1)}" width="${it.lw.toFixed(1)}" height="${it.lh}" rx="3" fill="${surfCol}" fill-opacity="0.9" style="pointer-events:none" />` +
          `<text x="${(it.lx + 4).toFixed(1)}" y="${(it.ly + 12.5).toFixed(1)}" fill="${textCol}" font-size="11.5" font-family="var(--td-font-mono)" style="pointer-events:none">${ArchModel.esc(it.shortName)}</text>`
        );
      }
    }

    svg.innerHTML = parts.join('');
    const nodeMap = new Map(filteredNodes.map(n => [n.id, n]));
    svg.querySelectorAll('.ai-node').forEach(el => {
      const id = el.getAttribute('data-id');
      const n = nodeMap.get(id);
      if (!n) return;
      el.addEventListener('mousemove', evt => {
        callbacks.showTip(
          evt,
          `<div class="mono" style="font-weight:700;margin-bottom:4px">${ArchModel.esc(n.id)}</div>` +
          `<div>Вх: <b>${n.afferent_coupling}</b> · Исх: <b>${n.efferent_coupling}</b> · Файлов: ${n.file_count} · Строк: ${n.loc.toLocaleString()}</div>` +
          `<div>I = <b>${ArchModel.fmt(n.instability)}</b> · A = <b>${ArchModel.fmt(n.abstractness)}</b> · D = <b>${ArchModel.fmt(n.distance)}</b></div>` +
          `<div>Git: <b>${n.commits || 0}</b> коммитов · Индекс риска: <b>${ArchModel.fmt(n.hotspot_score)}</b></div>` +
          `<div style="margin-top:4px">${zoneBadge(n.zone, n.active_pain)} ${n.in_cycle ? '<span class="badge b-pain">В цикле</span>' : ''}</div>`
        );
      });
      el.addEventListener('mouseleave', callbacks.hideTip);
      el.addEventListener('click', () => callbacks.onSelectViewNode(n.id));
    });

    const strip = document.getElementById('unplotted-strip');
    if (strip) {
      if (unplotted.length > 0) {
        strip.classList.remove('hidden');
        document.getElementById('unplotted-title').textContent =
          `Вне графика (${unplotted.length} без связей или без типов — нажмите для просмотра):`;
        const pills = document.getElementById('unplotted-pills');
        pills.innerHTML = unplotted.slice(0, 50).map(n =>
          `<span class="pill ${n.id === selViewId ? 'selected' : ''}" data-id="${ArchModel.esc(n.id)}">${ArchModel.esc(n.id)}</span>`
        ).join('');
        pills.querySelectorAll('.pill[data-id]').forEach(p => {
          p.addEventListener('click', () => callbacks.onSelectViewNode(p.getAttribute('data-id')));
        });
      } else {
        strip.classList.add('hidden');
      }
    }
  }

  // =========================================================================
  // 2. Карта кода (Squarified Treemap)
  // =========================================================================
  function squarify(items, x, y, w, h) {
    const rects = [];
    const total = items.reduce((s, it) => s + it.value, 0);
    if (total <= 0 || w <= 1 || h <= 1) return rects;
    const sorted = [...items].sort((a, b) => b.value - a.value);

    let cx = x, cy = y, cw = w, ch = h;
    let remTotal = total;
    let idx = 0;

    while (idx < sorted.length) {
      const vertical = cw >= ch;
      const side = vertical ? ch : cw;
      let row = [];
      let rowSum = 0;
      let bestWorst = Infinity;

      while (idx < sorted.length) {
        const cand = sorted[idx];
        const nextSum = rowSum + cand.value;
        const areaScale = (cw * ch) / remTotal;
        const rowArea = nextSum * areaScale;
        const thickness = rowArea / Math.max(1, side);

        let worst = 0;
        for (const it of [...row, cand]) {
          const len = (it.value * areaScale) / Math.max(0.01, thickness);
          const ratio = Math.max(thickness / Math.max(0.01, len), len / Math.max(0.01, thickness));
          if (ratio > worst) worst = ratio;
        }
        if (row.length > 0 && worst > bestWorst) break;
        bestWorst = worst;
        row.push(cand);
        rowSum = nextSum;
        idx++;
      }

      const frac = rowSum / Math.max(1e-9, remTotal);
      if (vertical) {
        const rw = cw * frac;
        let ry = cy;
        for (const it of row) {
          const rh = ch * (it.value / Math.max(1e-9, rowSum));
          rects.push({ ...it, x: cx, y: ry, w: rw, h: rh });
          ry += rh;
        }
        cx += rw;
        cw -= rw;
      } else {
        const rh = ch * frac;
        let rx = cx;
        for (const it of row) {
          const rw = cw * (it.value / Math.max(1e-9, rowSum));
          rects.push({ ...it, x: rx, y: cy, w: rw, h: rh });
          rx += rw;
        }
        cy += rh;
        ch -= rh;
      }
      remTotal -= rowSum;
    }
    return rects;
  }

  function heatColor(t, isDark) {
    const c = Math.max(0, Math.min(1, t));
    if (isDark) {
      if (c < 0.25) return '#276b53';
      if (c < 0.5) return '#4b4b4b';
      if (c < 0.75) return '#b35c24';
      return '#a83c44';
    }
    if (c < 0.25) return '#2ba471';
    if (c < 0.5) return '#5f5f5f';
    if (c < 0.75) return '#e37318';
    return '#d54941';
  }

  function renderTreemap(report, hierarchy, appState, callbacks) {
    const svg = document.getElementById('treemap-svg');
    if (!svg) return;
    const isDark = document.documentElement.getAttribute('data-theme') === 'dark';
    const rect = svg.getBoundingClientRect();
    const W = Math.max(820, Math.round(rect.width || 1080));
    const H = Math.max(540, Math.round(rect.height || 640));
    svg.setAttribute('viewBox', `0 0 ${W} ${H}`);

    const fg = report.file_graph || { files: [], edges: [] };
    const q = (appState.search || '').trim().toLowerCase();
    const lang = appState.lang || 'all';
    const areaMetric = appState.treemapArea || 'loc';
    const colorMetric = appState.treemapColor || 'hotspot';

    const degMap = new Map();
    for (const e of fg.edges || []) {
      degMap.set(e.from, (degMap.get(e.from) || 0) + 1);
      degMap.set(e.to, (degMap.get(e.to) || 0) + 1);
    }

    const files = (fg.files || []).filter(f => {
      if (lang !== 'all' && (f.language || '').toLowerCase() !== lang) return false;
      if (q && !f.path.toLowerCase().includes(q) && !(f.module_id || '').toLowerCase().includes(q)) return false;
      return true;
    });

    if (files.length === 0) {
      svg.innerHTML = `<text x="${W / 2}" y="${H / 2}" text-anchor="middle" fill="${cssVar('--td-text-color-secondary')}" font-size="14">Нет файлов по текущему фильтру.</text>`;
      return;
    }

    const maxCommits = Math.max(1, ...files.map(f => (f.git ? f.git.commits : 0)));
    const fileVal = f => {
      if (areaMetric === 'coupling') return Math.max(1, (degMap.get(f.path) || 0) + 1);
      if (areaMetric === 'commits') return Math.max(1, (f.git ? f.git.commits : 0) + 1);
      return Math.max(10, f.loc || 10);
    };

    const modGroups = new Map();
    for (const f of files) {
      const m = f.module_id || f.package || '<root>';
      if (!modGroups.has(m)) modGroups.set(m, []);
      modGroups.get(m).push(f);
    }

    const modItems = [...modGroups.entries()].map(([modId, list]) => ({
      modId,
      files: list,
      value: list.reduce((s, f) => s + fileVal(f), 0),
    }));

    const modRects = squarify(modItems, 4, 4, W - 8, H - 8);
    const parts = [];
    const surfaceCol = cssVar('--td-bg-color-container');
    const borderCol = cssVar('--td-component-stroke');
    const textCol = cssVar('--td-text-color-primary');

    for (const mr of modRects) {
      const padTop = mr.h > 50 && mr.w > 88 ? 22 : 4;
      parts.push(
        `<rect x="${mr.x.toFixed(1)}" y="${mr.y.toFixed(1)}" width="${Math.max(1, mr.w - 4).toFixed(1)}" height="${Math.max(1, mr.h - 4).toFixed(1)}" ` +
        `fill="${surfaceCol}" stroke="${borderCol}" stroke-width="1" rx="6" />`
      );
      if (padTop > 10) {
        const maxC = Math.max(4, Math.floor((mr.w - 14) / 7.5));
        const lbl = mr.modId.length > maxC ? '…' + mr.modId.slice(-(Math.max(3, maxC - 1))) : mr.modId;
        parts.push(
          `<text x="${(mr.x + 7).toFixed(1)}" y="${(mr.y + 15.5).toFixed(1)}" fill="${textCol}" font-size="12" font-weight="700" font-family="var(--td-font-mono)">${ArchModel.esc(lbl)}</text>`
        );
      }

      const fItems = mr.files.map(f => ({ f, value: fileVal(f) }));
      const fRects = squarify(fItems, mr.x + 3, mr.y + padTop, Math.max(1, mr.w - 10), Math.max(1, mr.h - padTop - 7));
      for (const fr of fRects) {
        const f = fr.f;
        let fill = isDark ? '#4b4b4b' : '#5f5f5f';
        if (colorMetric === 'hotspot') fill = heatColor(f.git ? f.git.hotspot_score : 0, isDark);
        else if (colorMetric === 'commits') fill = heatColor((f.git ? f.git.commits : 0) / maxCommits, isDark);
        else if (colorMetric === 'busfactor') fill = heatColor(f.git ? f.git.top_author_share : 0, isDark);
        else if (colorMetric === 'louvain') {
          const cid = hierarchy.nodes.get(`file:${f.path}`)?.louvainCluster || 0;
          fill = CLUSTER_PALETTE[cid % CLUSTER_PALETTE.length];
        }

        const isSel = appState.selectedNodeId === `file:${f.path}`;
        parts.push(
          `<rect class="tm-cell" data-file="${ArchModel.esc(f.path)}" x="${fr.x.toFixed(1)}" y="${fr.y.toFixed(1)}" ` +
          `width="${Math.max(1, fr.w - 2).toFixed(1)}" height="${Math.max(1, fr.h - 2).toFixed(1)}" ` +
          `fill="${fill}" fill-opacity="${isSel ? 0.98 : 0.86}" stroke="${isSel ? textCol : surfaceCol}" stroke-width="${isSel ? 2.4 : 1}" rx="3" style="cursor:pointer" />`
        );
        if (fr.w > 66 && fr.h > 22) {
          const base = f.path.split('/').pop();
          const maxCh = Math.max(3, Math.floor((fr.w - 10) / 7.1));
          const lbl = base.length > maxCh ? base.slice(0, Math.max(2, maxCh - 1)) + '…' : base;
          parts.push(
            `<text x="${(fr.x + 6).toFixed(1)}" y="${(fr.y + 15.5).toFixed(1)}" fill="#fff" font-size="11.5" font-weight="600" font-family="var(--td-font-mono)" style="pointer-events:none">${ArchModel.esc(lbl)}</text>`
          );
        }
      }
    }

    svg.innerHTML = parts.join('');
    const fileByPath = new Map(files.map(f => [f.path, f]));
    svg.querySelectorAll('.tm-cell').forEach(el => {
      const fp = el.getAttribute('data-file');
      const f = fileByPath.get(fp);
      if (!f) return;
      el.addEventListener('mousemove', evt => {
        const g = f.git;
        callbacks.showTip(
          evt,
          `<div class="mono" style="font-weight:700">${ArchModel.esc(f.path)}</div>` +
          `<div>Модуль: <b>${ArchModel.esc(f.module_id)}</b> · Строк: <b>${f.loc}</b> · Связей: <b>${degMap.get(f.path) || 0}</b></div>` +
          (g
            ? `<div>Git: <b>${g.commits}</b> коммитов (+${g.lines_added}/-${g.lines_deleted}) · Индекс риска: <b>${ArchModel.fmt(g.hotspot_score)}</b></div>` +
              `<div>Основной автор: <b>${ArchModel.esc(g.top_author)}</b> (${Math.round(g.top_author_share * 100)}%, всего авторов: ${g.authors})</div>`
            : `<div style="color:var(--td-text-color-secondary)">Нет коммитов за период</div>`)
        );
      });
      el.addEventListener('mouseleave', callbacks.hideTip);
      el.addEventListener('click', () => callbacks.onSelectFile(f.path));
    });
  }

  // =========================================================================
  // 3. Объединённая вкладка «Проблемы и Git»: таблица, циклы, SDP, правила, Git
  // =========================================================================
  function renderDiagnostics(report, view, filteredNodes, appState, callbacks) {
    const key = appState.sortKey;
    const dir = appState.sortAsc ? 1 : -1;
    const selViewId = (appState.selectedNodeId || '').replace(/^(pkg|mod|file):/, '');

    const sorted = [...filteredNodes].sort((a, b) => {
      const va = a[key], vb = b[key];
      if (va == null && vb == null) return a.id.localeCompare(b.id);
      if (va == null) return 1;
      if (vb == null) return -1;
      if (typeof va === 'string') return va.localeCompare(vb) * dir;
      if (typeof va === 'boolean') return (Number(va) - Number(vb)) * dir;
      return (va - vb) * dir;
    });

    const tbody = document.getElementById('metrics-tbody');
    if (tbody) {
      tbody.innerHTML = sorted.map(n => `
        <tr data-id="${ArchModel.esc(n.id)}" class="${n.id === selViewId ? 'selected' : ''}">
          <td class="mono">${ArchModel.esc(n.id)}</td>
          <td>${n.file_count}</td>
          <td>${n.loc.toLocaleString()}</td>
          <td>${n.afferent_coupling}</td>
          <td>${n.efferent_coupling}</td>
          <td>${ArchModel.fmt(n.instability)}</td>
          <td>${ArchModel.fmt(n.abstractness)}</td>
          <td><b>${ArchModel.fmt(n.distance)}</b></td>
          <td>${n.commits || 0}</td>
          <td>${ArchModel.fmt(n.hotspot_score)}</td>
          <td>${zoneBadge(n.zone, n.active_pain)}</td>
          <td>${n.in_cycle ? '<span class="badge b-pain">ДА</span>' : '—'}</td>
        </tr>
      `).join('');
      tbody.querySelectorAll('tr[data-id]').forEach(tr => {
        tr.addEventListener('click', () => callbacks.onSelectViewNode(tr.getAttribute('data-id')));
      });
    }

    // Циклы и минимальные разрезы
    document.getElementById('cycles-hdr').textContent = `Циклы зависимостей и план разрыва (${view.cycles.length})`;
    const cycEl = document.getElementById('cycles-list');
    if (view.cycles.length === 0) {
      cycEl.innerHTML = `<div class="card-item" style="color:var(--td-success-color)">Циклических зависимостей не найдено.</div>`;
    } else {
      cycEl.innerHTML = view.cycles.map(c => `
        <div class="card-item">
          <div style="font-weight:600;margin-bottom:6px">
            <span class="badge b-pain">${c.nodes.length} узла · ${c.edge_count} связей</span>
          </div>
          <div class="pill-list" style="margin-bottom:8px">
            ${c.nodes.map(id => `<span class="pill" data-id="${ArchModel.esc(id)}">${ArchModel.esc(id)}</span>`).join('')}
          </div>
          ${(c.break_suggestions || []).length > 0 ? `
            <div style="font-size:12px;font-weight:700;color:var(--td-text-color-primary);margin:6px 0 4px">Минимальный разрез для устранения цикла:</div>
            ${c.break_suggestions.map(brk => `
              <div class="edge-sample">
                <span><span class="badge b-muted">Разрез</span> <b>${ArchModel.esc(brk.from)}</b> &rarr; <b>${ArchModel.esc(brk.to)}</b> (${brk.weight} имп.)</span>
                ${(brk.file_edges || []).slice(0, 3).map(fe => `
                  <div style="width:100%;padding-left:12px">
                    <a href="${ArchModel.ideUri(report.root_path, fe.from, fe.line)}">${ArchModel.esc(fe.from)}:${fe.line}</a>
                    &rarr; ${ArchModel.esc(fe.to)}
                    ${fe.symbols?.length ? `<span style="color:var(--td-text-color-secondary)">[${ArchModel.esc(fe.symbols.slice(0, 3).join(', '))}]</span>` : ''}
                    <button class="sm" data-sim-cut="${ArchModel.esc(ArchModel.edgeKey(fe.from, fe.to))}">Убрать в симуляции</button>
                  </div>
                `).join('')}
              </div>
            `).join('')}
          ` : ''}
        </div>
      `).join('');
      cycEl.querySelectorAll('.pill[data-id]').forEach(p => {
        p.addEventListener('click', () => callbacks.onSelectViewNode(p.getAttribute('data-id')));
      });
      cycEl.querySelectorAll('[data-sim-cut]').forEach(btn => {
        btn.addEventListener('click', () => callbacks.onToggleCut(btn.getAttribute('data-sim-cut')));
      });
    }

    // Нарушения стабильности (SDP) и правил
    const rules = report.rule_violations || [];
    document.getElementById('sdp-hdr').textContent = rules.length > 0
      ? `Нарушения стабильности (${view.sdp_violations.length}) и правил (${rules.length})`
      : `Нарушения принципа стабильности (${view.sdp_violations.length})`;
    const sdpEl = document.getElementById('sdp-list');
    let rightHtml = '';

    if (rules.length > 0) {
      rightHtml += rules.slice(0, 20).map(rv => `
        <div class="card-item" style="border-left:3px solid var(--td-error-color)">
          <div><span class="badge b-rule">${ArchModel.esc(rv.rule_name)}</span> <span style="font-size:12.5px;color:var(--td-text-color-secondary)">${ArchModel.esc(rv.message)}</span></div>
          <div class="edge-sample">
            <a href="${ArchModel.ideUri(report.root_path, rv.from_file, rv.line)}">${ArchModel.esc(rv.from_file)}:${rv.line}</a>
            &rarr; ${ArchModel.esc(rv.to_file)}
          </div>
        </div>
      `).join('');
    }

    if (view.sdp_violations.length === 0) {
      rightHtml += `<div class="card-item" style="color:var(--td-success-color)">Нарушений направления стабильности нет.</div>`;
    } else {
      rightHtml += view.sdp_violations.slice(0, 25).map(v => `
        <div class="card-item">
          <div class="mono" style="font-size:13px">
            <a href="#" data-id="${ArchModel.esc(v.from)}" class="code-link">${ArchModel.esc(v.from)}</a>
            <span style="color:var(--td-text-color-secondary)">(I=${v.from_instability.toFixed(2)})</span>
            &rarr;
            <a href="#" data-id="${ArchModel.esc(v.to)}" class="code-link">${ArchModel.esc(v.to)}</a>
            <span style="color:var(--td-text-color-secondary)">(I=${v.to_instability.toFixed(2)})</span>
          </div>
          <div style="margin-top:5px;font-size:12.5px;color:var(--td-text-color-secondary)">
            <span class="badge b-calm">&Delta;I = +${v.delta.toFixed(2)}</span>
            ${v.is_dip_only ? '<span class="badge b-dip">через интерфейсы</span>' : ''}
            · связей: ${v.weight}
          </div>
        </div>
      `).join('');
    }
    sdpEl.innerHTML = rightHtml;
    sdpEl.querySelectorAll('a[data-id]').forEach(a => {
      a.addEventListener('click', e => { e.preventDefault(); callbacks.onSelectViewNode(a.getAttribute('data-id')); });
    });

    // Скрытые связи из Git (совместные коммиты без прямого импорта)
    const tcEl = document.getElementById('temporal-list');
    const git = report.git;
    if (!git || !git.temporal_couplings || git.temporal_couplings.length === 0) {
      tcEl.innerHTML = `<div class="card-item" style="color:var(--td-text-color-secondary)">Сильных скрытых связей по коммитам не найдено.</div>`;
    } else {
      const hiddenOnly = git.temporal_couplings.filter(t => !t.has_static_edge);
      const list = hiddenOnly.length > 0 ? hiddenOnly : git.temporal_couplings;
      document.getElementById('temporal-hdr').textContent = `Скрытые связи из Git (${hiddenOnly.length} без прямого импорта)`;
      tcEl.innerHTML = list.slice(0, 15).map(tc => `
        <div class="card-item">
          <div class="mono" style="font-size:12.5px">
            <a href="#" class="code-link" data-jump-file="${ArchModel.esc(tc.file_a)}">${ArchModel.esc(tc.file_a)}</a>
            &harr;
            <a href="#" class="code-link" data-jump-file="${ArchModel.esc(tc.file_b)}">${ArchModel.esc(tc.file_b)}</a>
          </div>
          <div style="margin-top:5px;font-size:12.5px;color:var(--td-text-color-secondary)">
            <b>${tc.shared_commits}</b> общих коммитов (${Math.round(tc.strength * 100)}%)
            ${tc.cross_module ? ' · <span class="badge b-calm">разные модули</span>' : ''}
          </div>
        </div>
      `).join('');
      tcEl.querySelectorAll('[data-jump-file]').forEach(a => {
        a.addEventListener('click', evt => { evt.preventDefault(); callbacks.onSelectFile(a.getAttribute('data-jump-file')); });
      });
    }

    // Файлы с притяжением к другому модулю (Louvain)
    const mpEl = document.getElementById('misplaced-list');
    const misplaced = report.misplaced_files || [];
    if (misplaced.length === 0) {
      mpEl.innerHTML = `<div class="card-item" style="color:var(--td-success-color)">Файлов, лежащих явно не в своём модуле, не обнаружено.</div>`;
    } else {
      mpEl.innerHTML = misplaced.slice(0, 12).map(mf => `
        <div class="card-item">
          <div style="display:flex;justify-content:space-between;gap:8px;align-items:center;flex-wrap:wrap">
            <a href="#" class="code-link" data-jump-file="${ArchModel.esc(mf.file)}">${ArchModel.esc(mf.file)}</a>
            <button class="sm" data-whatif-move="${ArchModel.esc(mf.file)}" data-to-mod="${ArchModel.esc(mf.suggested_module)}">Перенести в симуляции</button>
          </div>
          <div style="margin-top:5px;font-size:12.5px;color:var(--td-text-color-secondary)">
            Сейчас в <b class="mono">${ArchModel.esc(mf.current_module)}</b> (${mf.internal_edges}) &rarr;
            <b>${Math.round(mf.affinity * 100)}%</b> связей ведут в <b class="mono">${ArchModel.esc(mf.suggested_module)}</b> (${mf.suggested_module_edges})
          </div>
        </div>
      `).join('');
      mpEl.querySelectorAll('[data-jump-file]').forEach(a => {
        a.addEventListener('click', evt => { evt.preventDefault(); callbacks.onSelectFile(a.getAttribute('data-jump-file')); });
      });
      mpEl.querySelectorAll('[data-whatif-move]').forEach(btn => {
        btn.addEventListener('click', () => {
          callbacks.onSimulateMove(btn.getAttribute('data-whatif-move'), btn.getAttribute('data-to-mod'));
        });
      });
    }
  }

  return {
    zoneBadge,
    renderMainSeq,
    renderTreemap,
    renderDiagnostics,
  };
})();
