// Иерархическая раскрываемая матрица зависимостей (DSM).

const ArchDsm = (() => {
  const MAX_DSM_ROWS = 85;

  function render(hierarchy, appState, callbacks) {
    const container = document.getElementById('dsm-container');
    const note = document.getElementById('dsm-note');
    if (!container) return;

    const expanded = appState.dsmExpandedIds;
    const q = (appState.search || '').trim().toLowerCase();
    const lang = appState.lang || 'all';

    const rows = [];
    function matches(node) {
      if (lang !== 'all') {
        const has = Object.keys(node.languages || {}).some(k => k.toLowerCase() === lang);
        if (!has && (node.language || '').toLowerCase() !== lang) return false;
      }
      if (!q) return true;
      if (node.id.toLowerCase().includes(q) || node.label.toLowerCase().includes(q)) return true;
      return (node.files || []).some(f => f.toLowerCase().includes(q));
    }

    function walk(id, depth) {
      const node = hierarchy.nodes.get(id);
      if (!node || !matches(node)) return;
      const expandableKids = node.children.filter(cid => hierarchy.nodes.get(cid)?.kind !== 'symbol');
      const canExpand = expandableKids.length > 0;
      const isExpanded = canExpand && expanded.has(id);

      if (isExpanded) {
        for (const cid of expandableKids) walk(cid, depth + 1);
      } else {
        rows.push({
          id: node.id,
          label: node.label,
          kind: node.kind,
          parentId: node.parentId,
          depth,
          canExpand,
          node,
        });
      }
    }

    for (const r of hierarchy.roots) walk(r, 0);

    const rowSet = new Set(rows.map(r => r.id));
    function resolveRow(filePath) {
      let cur = `file:${filePath}`;
      while (cur) {
        if (rowSet.has(cur)) return cur;
        cur = hierarchy.nodes.get(cur)?.parentId || null;
      }
      return null;
    }

    const edgeMap = new Map();
    const caCount = new Map(rows.map(r => [r.id, 0]));
    const ceCount = new Map(rows.map(r => [r.id, 0]));
    let maxW = 1;

    for (const fe of hierarchy.fileEdges) {
      const u = resolveRow(fe.from);
      const v = resolveRow(fe.to);
      if (!u || !v || u === v) continue;
      const key = ArchModel.edgeKey(u, v);
      if (!edgeMap.has(key)) {
        edgeMap.set(key, {
          id: key, from: u, to: v, weight: 0, activeWeight: 0, dipWeight: 0,
          cutCount: 0, isRuleViolation: false, fileEdges: [],
        });
      }
      const agg = edgeMap.get(key);
      agg.weight += 1;
      if (fe.isCut) agg.cutCount += 1;
      else {
        agg.activeWeight += 1;
        ceCount.set(u, (ceCount.get(u) || 0) + 1);
        caCount.set(v, (caCount.get(v) || 0) + 1);
      }
      if (fe.is_dip) agg.dipWeight += 1;
      if (fe.isRuleViolation) agg.isRuleViolation = true;
      agg.fileEdges.push(fe);
      if (agg.weight > maxW) maxW = agg.weight;
    }

    let visibleRows = rows;
    if (visibleRows.length > MAX_DSM_ROWS) {
      const scored = [...visibleRows]
        .sort((a, b) => (caCount.get(b.id) + ceCount.get(b.id)) - (caCount.get(a.id) + ceCount.get(a.id)))
        .slice(0, MAX_DSM_ROWS);
      const keep = new Set(scored.map(r => r.id));
      visibleRows = visibleRows.filter(r => keep.has(r.id));
      note.textContent = `Топ ${MAX_DSM_ROWS} самых связанных из ${rows.length} (сузьте поиск или сверните модули)`;
    } else {
      note.textContent = `Матрица ${visibleRows.length} × ${visibleRows.length} · ▸ раскрыть модуль · клик по ячейке — разбор связи`;
    }

    if (visibleRows.length === 0) {
      container.innerHTML = `<div class="empty-state">Ничего не найдено по текущему фильтру.</div>`;
      return;
    }

    const ids = visibleRows.map(r => r.id);
    const activeEdges = [...edgeMap.values()].filter(e => e.activeWeight > 0 && rowSet.has(e.from) && rowSet.has(e.to));
    const { sccs, nodeScc, inCycle } = ArchModel.computeTarjanScc(ids, activeEdges);

    const sccCount = sccs.length;
    const revAdj = Array.from({ length: sccCount }, () => new Set());
    const indeg = new Array(sccCount).fill(0);
    for (const e of activeEdges) {
      const su = nodeScc.get(e.from);
      const sv = nodeScc.get(e.to);
      if (su != null && sv != null && su !== sv && !revAdj[sv].has(su)) {
        revAdj[sv].add(su);
        indeg[su]++;
      }
    }
    const instOf = id => {
      const ca = caCount.get(id) || 0;
      const ce = ceCount.get(id) || 0;
      return ca + ce === 0 ? 1.1 : ce / (ca + ce);
    };
    const ready = [];
    for (let i = 0; i < sccCount; i++) if (indeg[i] === 0) ready.push(i);
    const orderedIds = [];
    const sccAvgI = cid => {
      const members = sccs[cid];
      return members.reduce((s, id) => s + instOf(id), 0) / Math.max(1, members.length);
    };
    while (ready.length > 0) {
      ready.sort((a, b) => sccAvgI(a) - sccAvgI(b));
      const cid = ready.shift();
      const members = [...sccs[cid]].sort((a, b) => instOf(a) - instOf(b) || a.localeCompare(b));
      orderedIds.push(...members);
      for (const nxt of revAdj[cid]) {
        indeg[nxt]--;
        if (indeg[nxt] === 0) ready.push(nxt);
      }
    }

    const rowMap = new Map(visibleRows.map(r => [r.id, r]));
    const orderedRows = orderedIds.map(id => rowMap.get(id)).filter(Boolean);

    const isDark = document.documentElement.getAttribute('data-theme') === 'dark';

    let html = `<table class="dsm"><thead><tr><th class="row-hdr">Строка зависит от столбца &rarr;</th>`;
    orderedRows.forEach((r, idx) => {
      html += `<th data-col-idx="${idx}" data-tip="${ArchModel.esc(r.id.replace(/^(pkg|mod|file):/, ''))}">${idx + 1}</th>`;
    });
    html += `</tr></thead><tbody>`;

    orderedRows.forEach((row, r) => {
      const cleanName = row.id.replace(/^(pkg|mod|file):/, '');
      const padLeft = row.depth * 12;
      const isSel = row.id === appState.selectedNodeId;
      const toggleBtn = row.canExpand
        ? `<span class="tree-toggle" data-dsm-expand="${ArchModel.esc(row.id)}" data-tip="Раскрыть модуль до отдельных файлов">▸</span>`
        : row.parentId && expanded.has(row.parentId)
        ? `<span class="tree-toggle" data-dsm-collapse="${ArchModel.esc(row.parentId)}" data-tip="Свернуть файлы обратно в модуль">↩</span>`
        : `<span style="display:inline-block;width:18px"></span>`;

      html += `<tr><th class="row-hdr ${isSel ? 'selected' : ''}" data-row-idx="${r}" data-dsm-node="${ArchModel.esc(row.id)}" data-tip="${ArchModel.esc(cleanName)}" style="padding-left:${8 + padLeft}px">${toggleBtn}${r + 1}. ${ArchModel.esc(row.label)}</th>`;

      orderedRows.forEach((col, c) => {
        const sameScc = inCycle.has(row.id) && nodeScc.get(row.id) === nodeScc.get(col.id);
        if (r === c) {
          const diagTip = inCycle.has(row.id)
            ? 'Узел входит в циклическую зависимость (выделен красным)'
            : 'Диагональ: сам компонент';
          html += `<td class="diag ${inCycle.has(row.id) ? 'in-scc' : ''}" data-tip="${diagTip}">${r + 1}</td>`;
        } else {
          const edge = edgeMap.get(ArchModel.edgeKey(row.id, col.id));
          if (!edge) {
            html += `<td class="${sameScc ? 'cell-scc-box' : ''}"></td>`;
          } else {
            const isUpper = r < c;
            const isAllCut = edge.activeWeight === 0 && edge.cutCount > 0;
            const isDipOnly = edge.dipWeight === edge.weight && edge.weight > 0;
            const hasPartialDip = edge.dipWeight > 0 && !isDipOnly;
            const norm = Math.log2(edge.weight + 1) / Math.log2(maxW + 1);
            const semAlpha = (0.18 + 0.36 * norm).toFixed(2);
            const inkAlpha = (isDark ? 0.11 + 0.24 * norm : 0.08 + 0.26 * norm).toFixed(2);
            let bg;
            if (isAllCut) bg = isDark ? 'rgba(147, 147, 147, 0.14)' : 'rgba(120, 120, 120, 0.12)';
            else if (edge.isRuleViolation || isUpper || sameScc) {
              bg = isDark ? `rgba(222, 102, 112, ${semAlpha})` : `rgba(213, 73, 65, ${semAlpha})`;
            } else if (isDipOnly) {
              bg = isDark ? `rgba(70, 191, 150, ${semAlpha})` : `rgba(0, 136, 88, ${semAlpha})`;
            } else {
              bg = isDark ? `rgba(246, 246, 246, ${inkAlpha})` : `rgba(38, 38, 38, ${inkAlpha})`;
            }

            const extraStyle = hasPartialDip && !isAllCut ? ';border-bottom:2px solid var(--td-dip-color)' : '';
            const txt = isAllCut
              ? '×'
              : edge.isRuleViolation
              ? `!${edge.weight}`
              : isDipOnly
              ? `${edge.weight}d`
              : String(edge.weight);
            html += `<td class="cell-dep ${sameScc ? 'cell-scc-box' : ''}" data-r="${r}" data-c="${c}" data-from="${ArchModel.esc(row.id)}" data-to="${ArchModel.esc(col.id)}" style="background:${bg}${extraStyle}">${txt}</td>`;
          }
        }
      });
      html += `</tr>`;
    });

    html += `</tbody></table>`;
    container.innerHTML = html;

    container.querySelectorAll('[data-dsm-expand]').forEach(el => {
      el.addEventListener('click', evt => {
        evt.stopPropagation();
        callbacks.onDsmToggle(el.getAttribute('data-dsm-expand'), true);
      });
    });
    container.querySelectorAll('[data-dsm-collapse]').forEach(el => {
      el.addEventListener('click', evt => {
        evt.stopPropagation();
        callbacks.onDsmToggle(el.getAttribute('data-dsm-collapse'), false);
      });
    });
    container.querySelectorAll('th[data-dsm-node]').forEach(th => {
      th.addEventListener('click', () => callbacks.onSelectNode(th.getAttribute('data-dsm-node')));
    });
    container.querySelectorAll('td.cell-dep').forEach(td => {
      const from = td.getAttribute('data-from');
      const to = td.getAttribute('data-to');
      const rIdx = td.getAttribute('data-r');
      const cIdx = td.getAttribute('data-c');
      const edge = edgeMap.get(ArchModel.edgeKey(from, to));
      if (!edge) return;
      td.addEventListener('mouseenter', () => {
        container.querySelector(`th[data-row-idx="${rIdx}"]`)?.classList.add('row-hover');
        container.querySelector(`th[data-col-idx="${cIdx}"]`)?.classList.add('col-hover');
      });
      td.addEventListener('mousemove', evt => {
        const syms = [...new Set(edge.fileEdges.flatMap(fe => fe.symbols || []))].slice(0, 6);
        const samples = edge.fileEdges
          .slice(0, 4)
          .map(fe => `<div class="edge-sample">${ArchModel.esc(fe.from)}:${fe.line} &rarr; ${ArchModel.esc(fe.to)}</div>`)
          .join('');
        callbacks.showTip(
          evt,
          `<div class="mono" style="font-weight:700">${ArchModel.esc(from.replace(/^(pkg|mod|file):/, ''))} &rarr; ${ArchModel.esc(to.replace(/^(pkg|mod|file):/, ''))}</div>` +
          `<div><b>${edge.weight}</b> связей файлов · через интерфейсы (DIP): ${edge.dipWeight} ${edge.isRuleViolation ? '<span class="badge b-rule">Правило</span>' : ''}</div>` +
          (syms.length ? `<div class="mono" style="margin-top:3px">Символы: ${ArchModel.esc(syms.join(', '))}</div>` : '') +
          samples
        );
      });
      td.addEventListener('mouseleave', () => {
        container.querySelector(`th[data-row-idx="${rIdx}"]`)?.classList.remove('row-hover');
        container.querySelector(`th[data-col-idx="${cIdx}"]`)?.classList.remove('col-hover');
        callbacks.hideTip();
      });
      td.addEventListener('click', () => {
        callbacks.onSelectEdge({
          ...edge,
          isAllCut: edge.activeWeight === 0 && edge.cutCount > 0,
          isCycleEdge: inCycle.has(from) && nodeScc.get(from) === nodeScc.get(to),
          isSdpViolation: false,
          isDipOnly: edge.dipWeight === edge.weight && edge.weight > 0,
        });
      });
    });
  }

  return { render };
})();
