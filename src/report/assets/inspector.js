// Инспектор справа: свойства узла, файла, символа, связи, поиска пути и симуляции рефакторинга.

const ArchInspector = (() => {
  const KIND_LABELS = {
    package: 'ПАКЕТ',
    module: 'МОДУЛЬ',
    file: 'ФАЙЛ',
    symbol: 'СИМВОЛ',
  };

  function cleanId(id) {
    return String(id || '').replace(/^(pkg|mod|file|sym):/, '');
  }

  function render(report, hierarchy, graphEdges, appState, callbacks) {
    const el = document.getElementById('inspector');
    if (!el) return;

    if (appState.pathFromId && appState.pathToId) {
      renderPathInspector(el, report, hierarchy, graphEdges, appState, callbacks);
      return;
    }

    if (appState.selectedEdge) {
      renderEdgeInspector(el, report, appState.selectedEdge, appState, callbacks);
      return;
    }

    if (appState.selectedNodeId && hierarchy.nodes.has(appState.selectedNodeId)) {
      renderNodeInspector(el, report, hierarchy, graphEdges, appState, callbacks);
      return;
    }

    el.innerHTML = `
      <div class="empty-state">
        <div style="font-weight:700;color:var(--td-text-color-primary);margin-bottom:8px">Инспектор архитектуры</div>
        <div>Нажмите на любой <b>узел</b> или <b>стрелку</b> на графе, матрице или карте кода, чтобы увидеть точные импорты, символы и метрики.</div>
        <div style="margin-top:14px;font-size:12.5px;text-align:left">
          <b>Как пользоваться:</b><br/>
          • <b>Двойной клик</b> — раскрыть модуль до файлов и символов<br/>
          • <b>Shift + клик</b> по двум узлам — найти цепочку зависимостей<br/>
          • <b>Клик по стрелке</b> — посмотреть строки кода и смоделировать разрыв связи
        </div>
      </div>
    `;
  }

  function renderEdgeInspector(el, report, edge, appState, callbacks) {
    const fileEdges = edge.fileEdges || [];
    const cutSet = appState.whatIf.cutEdges;
    const allCut = fileEdges.length > 0 && fileEdges.every(fe => cutSet.has(`${fe.from}→${fe.to}`));

    if (edge.isTemporal && edge.temporalInfo) {
      const tc = edge.temporalInfo;
      el.innerHTML = `
        <div class="section-hdr no-border">
          <span>Совместные изменения (Git)</span>
          <button class="sm" id="btn-edge-close">Закрыть</button>
        </div>
        <h2>${ArchModel.esc(cleanId(edge.from))} &harr; ${ArchModel.esc(cleanId(edge.to))}</h2>
        <div style="margin:6px 0">
          <span class="badge b-useless">Нет прямого импорта</span>
          <span class="badge b-calm">${Math.round(tc.strength * 100)}% совпадений</span>
        </div>
        <div class="stat-grid">
          <div class="stat-box"><div class="k">Общих</div><div class="v">${tc.shared_commits}</div></div>
          <div class="stat-box"><div class="k">Коммитов A</div><div class="v">${tc.commits_a}</div></div>
          <div class="stat-box"><div class="k">Коммитов B</div><div class="v">${tc.commits_b}</div></div>
        </div>
        <div class="section-hdr">Связанные файлы</div>
        <div class="edge-sample"><a href="${ArchModel.ideUri(report.root_path, tc.file_a, 1)}">${ArchModel.esc(tc.file_a)}</a></div>
        <div class="edge-sample"><a href="${ArchModel.ideUri(report.root_path, tc.file_b, 1)}">${ArchModel.esc(tc.file_b)}</a></div>
      `;
      document.getElementById('btn-edge-close')?.addEventListener('click', () => callbacks.onClearSelection());
      return;
    }

    const symbols = [...new Set(fileEdges.flatMap(fe => fe.symbols || []))].sort();
    el.innerHTML = `
      <div class="section-hdr no-border">
        <span>Связь зависимостей</span>
        <button class="sm" id="btn-edge-close">Закрыть</button>
      </div>
      <h2>${ArchModel.esc(cleanId(edge.from))} &rarr; ${ArchModel.esc(cleanId(edge.to))}</h2>
      <div style="display:flex;flex-wrap:wrap;gap:5px;margin:8px 0">
        <span class="badge b-muted">Импортов: ${edge.weight}</span>
        ${edge.dipWeight > 0 ? `<span class="badge b-dip">Через интерфейсы: ${edge.dipWeight}/${edge.weight}</span>` : ''}
        ${edge.isCycleEdge ? '<span class="badge b-pain">В цикле</span>' : ''}
        ${edge.isSdpViolation ? '<span class="badge b-calm">Нарушение слоёв</span>' : ''}
        ${edge.isRuleViolation ? '<span class="badge b-rule">Нарушение правила</span>' : ''}
        ${allCut ? '<span class="badge b-useless">Убрано в симуляции</span>' : ''}
      </div>

      <div class="action-row">
        <button id="btn-cut-agg" class="${allCut ? 'whatif-on' : 'danger'}">
          ${allCut ? 'Вернуть связь' : 'Убрать в симуляции'}
        </button>
        <button id="btn-focus-edge-graph">Показать на графе</button>
      </div>

      ${symbols.length > 0 ? `
        <div class="section-hdr">Используемые символы (${symbols.length})</div>
        <div class="pill-list">
          ${symbols.slice(0, 28).map(s => `<span class="pill">${ArchModel.esc(s)}</span>`).join('')}
          ${symbols.length > 28 ? `<span class="pill">+${symbols.length - 28}</span>` : ''}
        </div>
      ` : ''}

      <div class="section-hdr">Импорты по файлам (${fileEdges.length})</div>
      <div style="max-height:380px;overflow-y:auto">
        ${fileEdges.map(fe => {
          const k = `${fe.from}→${fe.to}`;
          const isCut = cutSet.has(k);
          return `
            <div class="card-item" style="padding:9px 11px;margin-bottom:7px;${isCut ? 'opacity:0.55;outline:1px dashed var(--td-purple-color)' : ''}">
              <div class="mono" style="font-size:12.5px;overflow-wrap:anywhere">
                <a class="code-link" href="${ArchModel.ideUri(report.root_path, fe.from, fe.line)}">${ArchModel.esc(fe.from)}:${fe.line}</a>
                &rarr;
                <a class="code-link" href="${ArchModel.ideUri(report.root_path, fe.to, 1)}">${ArchModel.esc(fe.to)}</a>
              </div>
              <div style="margin-top:6px;display:flex;justify-content:space-between;align-items:center;gap:6px;flex-wrap:wrap">
                <div>
                  <span class="badge ${fe.is_dip ? 'b-dip' : 'b-muted'}">${fe.is_dip ? 'DIP / интерфейс' : ArchModel.esc(fe.kind)}</span>
                  ${fe.isRuleViolation ? '<span class="badge b-rule">Правило</span>' : ''}
                </div>
                <button class="sm" data-cut-file="${ArchModel.esc(k)}">${isCut ? 'Вернуть' : 'Убрать'}</button>
              </div>
              ${(fe.symbols || []).length > 0 ? `<div class="mono" style="font-size:12px;color:var(--td-text-color-secondary);margin-top:4px">${ArchModel.esc(fe.symbols.slice(0, 8).join(', '))}</div>` : ''}
            </div>
          `;
        }).join('')}
      </div>
    `;

    document.getElementById('btn-edge-close')?.addEventListener('click', () => callbacks.onClearSelection());
    document.getElementById('btn-cut-agg')?.addEventListener('click', () => {
      const keys = fileEdges.map(fe => `${fe.from}→${fe.to}`);
      callbacks.onToggleCutBatch(keys, !allCut);
    });
    document.getElementById('btn-focus-edge-graph')?.addEventListener('click', () => {
      callbacks.onFocusEdgeInGraph(edge);
    });
    el.querySelectorAll('[data-cut-file]').forEach(btn => {
      btn.addEventListener('click', () => callbacks.onToggleCut(btn.getAttribute('data-cut-file')));
    });
  }

  function renderPathInspector(el, report, hierarchy, graphEdges, appState, callbacks) {
    const paths = appState.pathResults || [];
    const edgeLookup = new Map(graphEdges.map(e => [`${e.from}→${e.to}`, e]));

    el.innerHTML = `
      <div class="section-hdr no-border">
        <span>Поиск пути (A &rarr; B)</span>
        <button class="sm" id="btn-clear-path">Сбросить</button>
      </div>
      <h2>${ArchModel.esc(cleanId(appState.pathFromId))} &rarr; ${ArchModel.esc(cleanId(appState.pathToId))}</h2>
      <div style="margin:8px 0;display:flex;gap:8px;align-items:center">
        <span class="badge ${paths.length ? 'b-cyan' : 'b-muted'}">Найдено путей: ${paths.length}</span>
        <button class="sm" id="btn-swap-path">Поменять A &harr; B</button>
      </div>
      ${paths.length === 0 ? `<div class="card-item">Прямого или транзитивного пути от A к B в текущем графе нет.</div>` :
        paths.map((p, idx) => `
          <div class="card-item">
            <div style="font-weight:700;font-size:12.5px;margin-bottom:5px">Путь #${idx + 1} (шагов: ${p.length - 1})</div>
            ${p.slice(0, -1).map((fromId, i) => {
              const toId = p[i + 1];
              const edge = edgeLookup.get(`${fromId}→${toId}`);
              const syms = edge ? [...new Set((edge.fileEdges || []).flatMap(fe => fe.symbols || []))].slice(0, 4) : [];
              return `
                <div class="edge-sample">
                  <a href="#" data-jump-node="${ArchModel.esc(fromId)}">${ArchModel.esc(cleanId(fromId))}</a>
                  &rarr;
                  <a href="#" data-jump-node="${ArchModel.esc(toId)}">${ArchModel.esc(cleanId(toId))}</a>
                  ${edge ? `<span class="badge b-muted">${edge.weight}</span>` : ''}
                  ${syms.length ? `<span style="color:var(--td-text-color-secondary)">[${ArchModel.esc(syms.join(', '))}]</span>` : ''}
                </div>
              `;
            }).join('')}
          </div>
        `).join('')}
    `;

    document.getElementById('btn-clear-path')?.addEventListener('click', () => callbacks.onClearPath());
    document.getElementById('btn-swap-path')?.addEventListener('click', () => callbacks.onSwapPath());
    el.querySelectorAll('[data-jump-node]').forEach(a => {
      a.addEventListener('click', evt => {
        evt.preventDefault();
        callbacks.onClearPath();
        callbacks.onSelectNode(a.getAttribute('data-jump-node'));
      });
    });
  }

  function computeNodeDetails(report, hierarchy, graphEdges, node) {
    const id = node.id;
    const visibleMatch = ArchGraph.getNodes().find(n => n.id === id && !n.isExpanded);
    if (visibleMatch && visibleMatch.metrics) {
      return {
        metrics: visibleMatch.metrics,
        outgoing: graphEdges.filter(e => e.from === id && !e.isTemporal).sort((a, b) => b.weight - a.weight),
        incoming: graphEdges.filter(e => e.to === id && !e.isTemporal).sort((a, b) => b.weight - a.weight),
      };
    }

    const ownFiles = new Set(node.files || []);
    const outMap = new Map();
    const inMap = new Map();
    const caSet = new Set();
    const ceSet = new Set();

    const targetGroupForFile = fp => {
      if (node.kind === 'file') return `file:${fp}`;
      const info = hierarchy.fileMap.get(fp);
      return info ? `mod:${info.effectiveMod}` : `file:${fp}`;
    };

    for (const fe of hierarchy.fileEdges) {
      const fromOwn = ownFiles.has(fe.from);
      const toOwn = ownFiles.has(fe.to);
      if (fromOwn === toOwn) continue;
      if (fromOwn) {
        if (!fe.isCut) ceSet.add(fe.to);
        const tgt = targetGroupForFile(fe.to);
        const k = `${id}→${tgt}`;
        if (!outMap.has(k)) {
          outMap.set(k, {
            id: k, from: id, to: tgt, weight: 0, activeWeight: 0, dipWeight: 0, cutCount: 0,
            isRuleViolation: false, isCycleEdge: false, isSdpViolation: false, isDipOnly: false, fileEdges: [],
          });
        }
        const agg = outMap.get(k);
        agg.weight++;
        if (fe.isCut) agg.cutCount++; else agg.activeWeight++;
        if (fe.is_dip) agg.dipWeight++;
        if (fe.isRuleViolation) agg.isRuleViolation = true;
        agg.fileEdges.push(fe);
      } else {
        if (!fe.isCut) caSet.add(fe.from);
        const src = targetGroupForFile(fe.from);
        const k = `${src}→${id}`;
        if (!inMap.has(k)) {
          inMap.set(k, {
            id: k, from: src, to: id, weight: 0, activeWeight: 0, dipWeight: 0, cutCount: 0,
            isRuleViolation: false, isCycleEdge: false, isSdpViolation: false, isDipOnly: false, fileEdges: [],
          });
        }
        const agg = inMap.get(k);
        agg.weight++;
        if (fe.isCut) agg.cutCount++; else agg.activeWeight++;
        if (fe.is_dip) agg.dipWeight++;
        if (fe.isRuleViolation) agg.isRuleViolation = true;
        agg.fileEdges.push(fe);
      }
    }

    const finalize = arr =>
      arr.map(e => ({ ...e, isDipOnly: e.weight > 0 && e.dipWeight === e.weight })).sort((a, b) => b.weight - a.weight);

    const ca = caSet.size;
    const ce = ceSet.size;
    const instability = ca + ce === 0 ? null : ce / (ca + ce);
    const totalSyms = node.abstractCount + node.concreteCount;
    const abstractness = totalSyms === 0 ? null : node.abstractCount / totalSyms;
    const distance = instability != null && abstractness != null ? Math.abs(abstractness + instability - 1.0) : null;
    const band = (report && report.main_sequence_band) || 0.30;
    let zone = 'isolated';
    if (instability != null) {
      if (abstractness == null) zone = 'no_entities';
      else if (distance <= band) zone = 'main_sequence';
      else if (abstractness + instability < 1.0) zone = 'zone_of_pain';
      else zone = 'zone_of_uselessness';
    }
    return {
      metrics: { ca, ce, instability, abstractness, distance, zone, activePain: zone === 'zone_of_pain' && node.hotspot >= 0.30, inCycle: false },
      outgoing: finalize([...outMap.values()]),
      incoming: finalize([...inMap.values()]),
    };
  }

  function renderNodeInspector(el, report, hierarchy, graphEdges, appState, callbacks) {
    const id = appState.selectedNodeId;
    const node = hierarchy.nodes.get(id);
    if (!node) return;

    const clean = cleanId(id);
    const { metrics: m, outgoing, incoming } = computeNodeDetails(report, hierarchy, graphEdges, node);
    const allLocalEdges = [...outgoing, ...incoming];
    const canExpand = node.children.length > 0;
    const isExpanded = appState.expandedIds.has(id);

    const modIds = hierarchy.moduleMetrics.modIds || [];
    const isFile = node.kind === 'file';
    const currentFileMod = isFile ? (appState.whatIf.movedFiles.get(node.fileNode.path) || node.fileNode.module_id) : '';

    el.innerHTML = `
      <div class="section-hdr no-border">
        <span>${KIND_LABELS[node.kind] || node.kind}</span>
        ${isFile ? `<a class="code-link" href="${ArchModel.ideUri(report.root_path, node.fileNode.path, 1)}">Открыть в IDE ↗</a>` : ''}
      </div>
      <h2>${ArchModel.esc(clean)}</h2>
      <div style="display:flex;flex-wrap:wrap;gap:6px;margin:8px 0">
        ${m.zone ? ArchCharts.zoneBadge(m.zone, m.activePain) : ''}
        ${m.inCycle ? '<span class="badge b-pain">В цикле</span>' : ''}
        ${node.language ? `<span class="badge b-muted">${ArchModel.esc(node.language)}</span>` : ''}
      </div>

      <div class="action-row">
        ${canExpand ? `<button id="btn-node-expand">${isExpanded ? 'Свернуть' : 'Раскрыть внутри'}</button>` : ''}
        <button id="btn-node-ego" class="${canExpand ? '' : 'span-2'}">Связи вокруг узла</button>
        <button id="btn-node-path-a" class="${appState.pathFromId && appState.pathFromId !== id ? '' : 'span-2'}">Найти путь отсюда</button>
        ${appState.pathFromId && appState.pathFromId !== id ? `<button id="btn-node-path-b" class="active">Путь &rarr; сюда</button>` : ''}
      </div>

      ${isFile ? `
        <div class="card-item" style="padding:10px 12px;margin:10px 0">
          <div style="font-size:12px;font-weight:700;color:var(--td-purple-color);margin-bottom:6px">Симуляция переноса файла в модуль</div>
          <div style="display:flex;gap:6px">
            <select id="whatif-move-select" style="flex:1;min-width:0">
              ${modIds.map(mid => `<option value="${ArchModel.esc(mid)}" ${mid === currentFileMod ? 'selected' : ''}>${ArchModel.esc(mid)}</option>`).join('')}
            </select>
            <button class="sm" id="btn-whatif-move">Перенести</button>
          </div>
        </div>
      ` : ''}

      <div class="stat-grid">
        <div class="stat-box" data-tip="Ca (Afferent): сколько внешних файлов ссылаются на этот компонент"><div class="k">Входящие</div><div class="v">${m.ca ?? incoming.length}</div></div>
        <div class="stat-box" data-tip="Ce (Efferent): от скольких внешних файлов зависит этот компонент"><div class="k">Исходящие</div><div class="v">${m.ce ?? outgoing.length}</div></div>
        <div class="stat-box" data-tip="Нестабильность I = Ce / (Ca + Ce). 0 — стабильное ядро, 1 — внешний слой"><div class="k">Нестаб. (I)</div><div class="v">${ArchModel.fmt(m.instability)}</div></div>
        <div class="stat-box" data-tip="Абстрактность A = доля трейтов и интерфейсов среди публичных сущностей"><div class="k">Абстр. (A)</div><div class="v">${ArchModel.fmt(m.abstractness)}</div></div>
        <div class="stat-box" data-tip="Отклонение от баланса D = |A + I − 1|. Чем ближе к 0, тем здоровее модуль"><div class="k">Отклон. (D)</div><div class="v">${ArchModel.fmt(m.distance)}</div></div>
        <div class="stat-box" data-tip="Индекс горячей точки (0..1): размер и связность × частота коммитов в Git"><div class="k">Риск (Git)</div><div class="v">${ArchModel.fmt(node.hotspot)}</div></div>
      </div>

      <div style="font-size:12.5px;color:var(--td-text-color-secondary);margin-bottom:8px">
        Файлов: <b>${node.files.length}</b> · Строк: <b>${node.loc.toLocaleString()}</b> ·
        Типов: <b>${node.abstractCount}</b> абстр. / <b>${node.concreteCount}</b> конкр. ·
        Коммитов: <b>${node.commits}</b>
      </div>

      <div class="section-hdr">Зависит от (${outgoing.length})</div>
      ${outgoing.length === 0 ? '<div style="color:var(--td-text-color-secondary);font-size:12.5px">Нет исходящих связей (ядро)</div>' : `
        <ul class="link-list">
          ${outgoing.map(e => `
            <li data-edge-id="${ArchModel.esc(e.id)}" data-tip="Нажмите, чтобы увидеть файлы, строки и символы этой связи">
              <span>${ArchModel.esc(cleanId(e.to))}</span>
              <span>
                ${e.isRuleViolation ? '<span class="badge b-rule" data-tip="Нарушение правила из archlens.toml">!</span> ' : ''}
                ${e.isCycleEdge ? '<span class="badge b-pain" data-tip="Ребро участвует в циклической зависимости">цикл</span> ' : ''}
                ${e.isSdpViolation ? '<span class="badge b-calm" data-tip="Нарушение стабильности: стабильный модуль зависит от менее стабильного">SDP</span> ' : ''}
                ${e.isDipOnly ? '<span class="badge b-dip" data-tip="Зависимость только через интерфейс / trait (инверсия зависимостей)">DIP</span> ' : ''}
                <b>${e.weight}</b>
              </span>
            </li>
          `).join('')}
        </ul>
      `}

      <div class="section-hdr">Используется в (${incoming.length})</div>
      ${incoming.length === 0 ? '<div style="color:var(--td-text-color-secondary);font-size:12.5px">Нет входящих связей (точка входа)</div>' : `
        <ul class="link-list">
          ${incoming.map(e => `
            <li data-edge-id="${ArchModel.esc(e.id)}" data-tip="Нажмите, чтобы увидеть файлы, строки и символы этой связи">
              <span>${ArchModel.esc(cleanId(e.from))}</span>
              <span>
                ${e.isCycleEdge ? '<span class="badge b-pain" data-tip="Ребро участвует в циклической зависимости">цикл</span> ' : ''}
                ${e.isDipOnly ? '<span class="badge b-dip" data-tip="Зависимость только через интерфейс / trait (инверсия зависимостей)">DIP</span> ' : ''}
                <b>${e.weight}</b>
              </span>
            </li>
          `).join('')}
        </ul>
      `}

      ${isFile && node.declarations.length > 0 ? `
        <div class="section-hdr">Объявления (${node.declarations.length})</div>
        <div class="sym-list">
          ${node.declarations.slice(0, 45).map(d => `
            <div class="sym-row">
              <span>${d.is_abstract ? '<span style="color:var(--td-dip-color)" data-tip="Абстракция (trait / interface / protocol)">◇</span>' : '<span style="color:var(--td-text-color-secondary)" data-tip="Конкретная реализация (struct / class / enum / fn)">▪</span>'} ${ArchModel.esc(d.name)}</span>
              <a class="code-link" data-tip="Открыть объявление в IDE на строке ${d.line}" href="${ArchModel.ideUri(report.root_path, node.fileNode.path, d.line)}">${ArchModel.esc(d.kind)}:${d.line}</a>
            </div>
          `).join('')}
        </div>
      ` : ''}

      ${!isFile && node.files.length > 0 ? `
        <div class="section-hdr">Файлы (${node.files.length})</div>
        <div class="file-list">
          ${node.files.slice(0, 70).map(fp => `
            <div class="file-row" data-open-file="${ArchModel.esc(fp)}">
              <span>${ArchModel.esc(fp)}</span>
              <a class="code-link" href="${ArchModel.ideUri(report.root_path, fp, 1)}" onclick="event.stopPropagation()">IDE</a>
            </div>
          `).join('')}
          ${node.files.length > 70 ? `<div style="padding:4px">… ещё ${node.files.length - 70}</div>` : ''}
        </div>
      ` : ''}
    `;

    document.getElementById('btn-node-expand')?.addEventListener('click', () => callbacks.onToggleExpand(id));
    document.getElementById('btn-node-ego')?.addEventListener('click', () => callbacks.onOpenEgo(id));
    document.getElementById('btn-node-path-a')?.addEventListener('click', () => callbacks.onSetPathStart(id));
    document.getElementById('btn-node-path-b')?.addEventListener('click', () => callbacks.onSetPathEnd(id));
    document.getElementById('btn-whatif-move')?.addEventListener('click', () => {
      const targetMod = document.getElementById('whatif-move-select').value;
      callbacks.onSimulateMove(node.fileNode.path, targetMod);
    });
    el.querySelectorAll('li[data-edge-id]').forEach(li => {
      li.addEventListener('click', () => {
        const eid = li.getAttribute('data-edge-id');
        const found = allLocalEdges.find(e => e.id === eid) || graphEdges.find(e => e.id === eid);
        if (found) callbacks.onSelectEdge(found);
      });
    });
    el.querySelectorAll('[data-open-file]').forEach(row => {
      row.addEventListener('click', () => callbacks.onSelectFile(row.getAttribute('data-open-file')));
    });
  }

  return { render };
})();
