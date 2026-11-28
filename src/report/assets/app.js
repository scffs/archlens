// Главный контроллер интерфейса archlens.

(() => {
  const REPORT = JSON.parse(document.getElementById('archlens-data').textContent);

  const VIEW_TITLES_RU = {
    components: 'Компоненты (из конфига)',
    modules: 'Модули',
    packages: 'Пакеты / крейты',
    'folders-d1': 'Папки (уровень 1)',
    'folders-d2': 'Папки (уровень 2)',
    'folders-d3': 'Папки (уровень 3)',
    files: 'Файлы',
  };

  function viewTitleRu(v) {
    return VIEW_TITLES_RU[v.id] || v.title;
  }

  function loadPref(key, fallback) {
    try {
      const v = localStorage.getItem(`archlens:${key}`);
      return v == null ? fallback : v;
    } catch (_) {
      return fallback;
    }
  }
  function savePref(key, val) {
    try {
      localStorage.setItem(`archlens:${key}`, String(val));
    } catch (_) {}
  }

  const state = {
    viewId: REPORT.default_view || (REPORT.views[0] && REPORT.views[0].id) || 'modules',
    tab: 'graph',
    lang: 'all',
    search: '',
    selectedNodeId: null,
    selectedEdgeId: null,
    selectedEdge: null,
    expandedIds: new Set(),
    dsmExpandedIds: new Set(),
    groupByPackage: loadPref('groupPkg', 'false') === 'true',
    hideIsolated: loadPref('showIso', 'false') !== 'true',
    graphLayout: 'sugiyama',
    graphOrient: loadPref('orient', 'TB') === 'LR' ? 'LR' : 'TB',
    colorMode: 'zone',
    edgeFilter: 'all',
    minWeight: 1,
    showTemporal: false,
    blastDepth: '1',
    blastDir: 'both',
    hiddenIsolatedCount: 0,
    pathFromId: null,
    pathToId: null,
    pathResults: [],
    sizeMode: 'loc',
    showLabels: true,
    jitter: true,
    treemapArea: 'loc',
    treemapColor: 'hotspot',
    sortKey: 'distance',
    sortAsc: false,
    whatIf: {
      cutEdges: new Set(),
      movedFiles: new Map(),
    },
  };

  let baselineHierarchy = ArchModel.buildHierarchy(REPORT, null);
  let hierarchy = baselineHierarchy;
  let graphDirty = true;

  for (const r of hierarchy.roots) {
    const node = hierarchy.nodes.get(r);
    if (node && node.kind === 'package') {
      state.dsmExpandedIds.add(r);
      if (state.groupByPackage) state.expandedIds.add(r);
    }
  }

  const $viewSel = document.getElementById('view-select');
  const $langSel = document.getElementById('lang-select');
  const $search = document.getElementById('search-input');
  const $tooltip = document.getElementById('tooltip');
  const $themeBtn = document.getElementById('theme-toggle');
  const $themeIcon = document.getElementById('theme-icon');
  const $aside = document.getElementById('t-aside');
  const $sidebarBtn = document.getElementById('sidebar-toggle');

  const ICON_MOON = '<path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>';
  const ICON_SUN = '<circle cx="12" cy="12" r="4"/><line x1="12" y1="2" x2="12" y2="5"/><line x1="12" y1="19" x2="12" y2="22"/><line x1="4.93" y1="4.93" x2="7.05" y2="7.05"/><line x1="16.95" y1="16.95" x2="19.07" y2="19.07"/><line x1="2" y1="12" x2="5" y2="12"/><line x1="19" y1="12" x2="22" y2="12"/><line x1="4.93" y1="19.07" x2="7.05" y2="16.95"/><line x1="16.95" y1="7.05" x2="19.07" y2="4.93"/>';

  if ($aside && loadPref('sidebar', 'expanded') === 'collapsed') {
    $aside.classList.add('collapsed');
  }
  if ($sidebarBtn && $aside) {
    $sidebarBtn.addEventListener('click', () => {
      const collapsed = $aside.classList.toggle('collapsed');
      savePref('sidebar', collapsed ? 'collapsed' : 'expanded');
      setTimeout(() => {
        if (state.tab === 'graph') ArchGraph.draw(state);
        else if (state.tab === 'treemap') renderActiveTab(true);
      }, 200);
    });
  }

  function syncThemeButton() {
    const cur = document.documentElement.getAttribute('data-theme') || 'light';
    if ($themeIcon) $themeIcon.innerHTML = cur === 'dark' ? ICON_SUN : ICON_MOON;
    $themeBtn.setAttribute('data-tip', cur === 'dark' ? 'Включить светлую тему' : 'Включить тёмную тему');
  }
  syncThemeButton();

  document.getElementById('chk-group-pkg').checked = state.groupByPackage;
  document.getElementById('chk-show-iso').checked = !state.hideIsolated;
  document.getElementById('orient-toggle').textContent = state.graphOrient === 'TB' ? 'Сверху ↓ Вниз' : 'Слева → Направо';

  // Скрываем пункт "Нарушения правил", если правила не настроены и нарушений нет
  if (!REPORT.rule_violations || REPORT.rule_violations.length === 0) {
    const opt = document.querySelector('#edge-filter option[value="rules"]');
    if (opt) opt.remove();
  }

  function showTip(evt, html) {
    $tooltip.innerHTML = html;
    $tooltip.style.display = 'block';
    const pad = 14;
    let x = evt.clientX + pad;
    let y = evt.clientY + pad;
    if (x + 390 > window.innerWidth) x = evt.clientX - 400;
    if (y + 180 > window.innerHeight) y = evt.clientY - 140;
    $tooltip.style.left = Math.max(8, x) + 'px';
    $tooltip.style.top = Math.max(8, y) + 'px';
  }
  function hideTip() { $tooltip.style.display = 'none'; }

  // Единый мгновенный тултип для всех элементов с [data-tip] (иконки, бейджи, KPI, колонки, легенда)
  let activeTipEl = null;
  document.addEventListener('mouseover', evt => {
    const el = evt.target && evt.target.closest ? evt.target.closest('[data-tip]') : null;
    if (!el) return;
    activeTipEl = el;
    const text = el.getAttribute('data-tip');
    if (text) showTip(evt, ArchModel.esc(text));
  });
  document.addEventListener('mousemove', evt => {
    if (!activeTipEl) return;
    const el = evt.target && evt.target.closest ? evt.target.closest('[data-tip]') : null;
    if (el !== activeTipEl) {
      activeTipEl = el;
      if (!el) { hideTip(); return; }
      const text = el.getAttribute('data-tip');
      if (text) showTip(evt, ArchModel.esc(text));
    } else if ($tooltip.style.display === 'block') {
      const pad = 14;
      let x = evt.clientX + pad;
      let y = evt.clientY + pad;
      if (x + 390 > window.innerWidth) x = evt.clientX - 400;
      if (y + 180 > window.innerHeight) y = evt.clientY - 140;
      $tooltip.style.left = Math.max(8, x) + 'px';
      $tooltip.style.top = Math.max(8, y) + 'px';
    }
  });
  document.addEventListener('mouseout', evt => {
    if (activeTipEl && (!evt.relatedTarget || !activeTipEl.contains(evt.relatedTarget))) {
      activeTipEl = null;
      hideTip();
    }
  });

  function currentView() {
    return REPORT.views.find(v => v.id === state.viewId) || REPORT.views[0];
  }

  function filteredViewNodes(view) {
    const q = state.search.trim().toLowerCase();
    return view.nodes.filter(n => {
      if (state.lang !== 'all' && n.primary_language.toLowerCase() !== state.lang) {
        const hasLang = n.languages && Object.keys(n.languages).some(k => k.toLowerCase() === state.lang);
        if (!hasLang) return false;
      }
      if (q && !n.id.toLowerCase().includes(q)) return false;
      return true;
    });
  }

  function hasWhatIfChanges() {
    return state.whatIf.cutEdges.size > 0 || state.whatIf.movedFiles.size > 0;
  }

  function rebuildHierarchy() {
    hierarchy = hasWhatIfChanges() ? ArchModel.buildHierarchy(REPORT, state.whatIf) : baselineHierarchy;
    graphDirty = true;
  }

  document.getElementById('project-title').textContent = `archlens · ${REPORT.project_name}`;
  document.getElementById('project-meta').textContent =
    `${REPORT.total_files} файлов · ${REPORT.total_loc.toLocaleString()} строк · ${REPORT.total_internal_edges} связей` +
    (REPORT.git ? ` · ${REPORT.git.commits_analyzed} коммитов (${REPORT.git.lookback_days} дн.)` : '');

  REPORT.views.forEach(v => {
    const opt = document.createElement('option');
    opt.value = v.id;
    opt.textContent = `${viewTitleRu(v)} (${v.summary.node_count})`;
    if (v.id === state.viewId) opt.selected = true;
    $viewSel.appendChild(opt);
  });

  const allLangs = Object.keys(REPORT.language_counts || {});
  if (allLangs.length <= 1) {
    $langSel.classList.add('hidden');
  } else {
    $langSel.innerHTML = `<option value="all">Все языки</option>` +
      allLangs.map(l => `<option value="${ArchModel.esc(l.toLowerCase())}">${ArchModel.esc(l)} (${REPORT.language_counts[l]})</option>`).join('');
  }
  const $asideFooter = document.getElementById('aside-footer');
  if ($asideFooter) {
    $asideFooter.textContent = allLangs.length > 0 ? `archlens · ${allLangs.join(', ')}` : 'archlens';
  }

  function renderBanners() {
    const wiEl = document.getElementById('whatif-banner');
    if (hasWhatIfChanges()) {
      const baseM = baselineHierarchy.moduleMetrics;
      const curM = hierarchy.moduleMetrics;
      wiEl.classList.remove('hidden');
      document.getElementById('whatif-stats').innerHTML =
        `<span>Изменений: <b>${state.whatIf.cutEdges.size}</b> разрыв(ов), <b>${state.whatIf.movedFiles.size}</b> перенос(ов)</span>` +
        `<span>Циклы: <b>${baseM.cycles.length} &rarr; ${curM.cycles.length}</b></span>` +
        `<span>Нарушения слоёв: <b>${baseM.sdpCount} &rarr; ${curM.sdpCount}</b></span>` +
        `<span>Связность: <b>${(baseM.propCost * 100).toFixed(1)}% &rarr; ${(curM.propCost * 100).toFixed(1)}%</b></span>`;
    } else {
      wiEl.classList.add('hidden');
    }

    const diffEl = document.getElementById('diff-banner');
    if (REPORT.baseline_diff) {
      const d = REPORT.baseline_diff;
      diffEl.classList.remove('hidden');
      document.getElementById('diff-stats').innerHTML =
        `<span>Срез: <b>${ArchModel.esc(d.baseline_view)}</b></span>` +
        `<span>Циклы: <b>${d.delta_cycles >= 0 ? '+' : ''}${d.delta_cycles}</b></span>` +
        `<span>Нарушения слоёв: <b>${d.delta_sdp_violations >= 0 ? '+' : ''}${d.delta_sdp_violations}</b></span>` +
        `<span>Связность: <b>${d.delta_propagation_cost >= 0 ? '+' : ''}${(d.delta_propagation_cost * 100).toFixed(1)}%</b></span>`;
    } else {
      diffEl.classList.add('hidden');
    }
  }

  function deltaPill(val, lowerIsBetter = true, fmtFn = v => (v > 0 ? `+${v}` : String(v))) {
    if (val == null || val === 0) return '';
    const good = lowerIsBetter ? val < 0 : val > 0;
    return `<span class="delta-pill ${good ? 'delta-good' : 'delta-bad'}">${fmtFn(val)}</span>`;
  }

  function renderKpis() {
    const v = currentView();
    const s = v.summary;
    const diff = REPORT.baseline_diff;
    const strip = document.getElementById('kpi-strip');
    const ruleCount = (REPORT.rule_violations || []).length;

    const items = [
      {
        label: 'Узлы и связи',
        val: `${s.node_count} / ${s.edge_count}`,
        sub: viewTitleRu(v),
        cls: '',
        tip: 'Число компонентов на выбранном уровне детализации и направленных зависимостей между ними',
      },
      {
        label: 'Циклы зависимостей',
        val: `${s.cycle_count} ${diff ? deltaPill(diff.delta_cycles) : ''}`,
        sub: s.nodes_in_cycles > 0 ? `затронуто узлов: ${s.nodes_in_cycles}` : 'зависимости ацикличны',
        cls: s.cycle_count > 0 ? 'bad' : 'good',
        tip: 'Сильно связные компоненты (SCC): группы модулей, которые прямо или косвенно ссылаются друг на друга',
      },
      {
        label: 'Охват изменений',
        val: `${(s.propagation_cost * 100).toFixed(1)}% ${diff ? deltaPill(Math.round(diff.delta_propagation_cost * 1000) / 10, true, x => `${x > 0 ? '+' : ''}${x}%`) : ''}`,
        sub: `ср. глубина ${s.acd.toFixed(1)} · индекс ${s.nccd.toFixed(2)}`,
        cls: s.nccd <= 1.2 ? 'good' : s.nccd <= 1.7 ? 'warn' : 'bad',
        tip: 'Propagation Cost: какая доля системы транзитивно связана со средним модулем. Индекс NCCD ≤ 1.2 — хорошая модульность, > 1.7 — сильная запутанность',
      },
      {
        label: 'Зоны риска',
        val: `${s.active_pain_count}`,
        sub: `часто меняются · всего в ядре: ${s.zone_of_pain_count}`,
        cls: s.active_pain_count > 0 ? 'bad' : 'good',
        tip: 'Модули в зоне риска: много входящих зависимостей, мало абстракций И высокая частота коммитов в Git',
      },
      {
        label: 'Нарушения слоёв',
        val: `${s.sdp_violation_count} ${diff ? deltaPill(diff.delta_sdp_violations) : ''}`,
        sub: 'стабильный зависит от нестабильного',
        cls: s.sdp_violation_count > 0 ? 'warn' : 'good',
        tip: 'Принцип стабильных зависимостей (SDP): более стабильный компонент (меньше I) не должен зависеть от менее стабильного (больше I)',
      },
    ];

    if (ruleCount > 0) {
      items.push({
        label: 'Правила границ',
        val: `${ruleCount}`,
        sub: 'нарушений по archlens.toml',
        cls: 'bad',
        tip: 'Нарушения правил слоёв, запрещённых импортов или публичных фасадов из archlens.toml',
      });
    }

    strip.innerHTML = items.map(it =>
      `<div class="kpi ${it.cls}" data-tip="${ArchModel.esc(it.tip)}"><div class="kpi-label">${it.label}</div><div class="kpi-val">${it.val}</div><div class="kpi-sub">${ArchModel.esc(it.sub)}</div></div>`
    ).join('');
  }

  function updateGraphHud() {
    const hud = document.getElementById('graph-hud-status');
    if (!hud) return;
    const nodes = ArchGraph.getNodes();
    const edges = ArchGraph.getEdges();
    const leaves = nodes.filter(n => !n.isExpanded).length;
    const containers = nodes.filter(n => n.isExpanded).length;
    hud.innerHTML =
      `<b>${leaves}</b> узлов${containers ? ` (раскрыто: ${containers})` : ''} · <b>${edges.length}</b> связей` +
      (state.hiddenIsolatedCount > 0 ? ` · скрыто изолированных: ${state.hiddenIsolatedCount}` : '') +
      (state.pathFromId && !state.pathToId ? ` · <span style="color:var(--td-info-color)">Точка A выбрана — Shift+клик по узлу B</span>` : '');
  }

  function renderCompoundGraph(preserveCamera = false) {
    const projected = ArchModel.projectCompoundGraph(hierarchy, {
      expandedIds: state.expandedIds,
      groupByPackage: state.groupByPackage,
      hideIsolated: state.hideIsolated,
      search: state.search,
      lang: state.lang,
      edgeFilter: state.edgeFilter,
      minWeight: state.minWeight,
      showTemporal: state.showTemporal,
      showCutEdges: true,
      report: REPORT,
    });
    state.hiddenIsolatedCount = projected.hiddenIsolatedCount || 0;

    if (state.pathFromId && state.pathToId) {
      state.pathResults = ArchModel.findPaths(projected.edges, state.pathFromId, state.pathToId);
    } else {
      state.pathResults = [];
    }

    ArchGraph.rebuild(projected, state, preserveCamera);
    graphDirty = false;
    updateGraphHud();
    if (state.selectedEdgeId) {
      const updated = projected.edges.find(e => e.id === state.selectedEdgeId);
      state.selectedEdge = updated || state.selectedEdge;
    }
    renderInspectorPanel();
  }

  function switchTab(tabName) {
    state.tab = tabName;
    document.querySelectorAll('.tab-btn').forEach(b => b.classList.toggle('active', b.dataset.tab === tabName));
    ['graph', 'dsm', 'mainseq', 'treemap', 'diagnostics'].forEach(t => {
      document.getElementById(`tab-${t}`)?.classList.toggle('hidden', t !== tabName);
    });
    renderActiveTab(true);
  }

  function ensureAncestorsExpanded(nodeId) {
    let cur = hierarchy.nodes.get(nodeId)?.parentId;
    while (cur) {
      const node = hierarchy.nodes.get(cur);
      if (node && node.kind === 'package' && !state.groupByPackage) break;
      state.expandedIds.add(cur);
      cur = node?.parentId;
    }
  }

  function resolveViewNodeToHierarchyId(viewNodeId) {
    for (const prefix of ['mod:', 'pkg:', 'file:']) {
      if (hierarchy.nodes.has(prefix + viewNodeId)) return prefix + viewNodeId;
    }
    return null;
  }

  const callbacks = {
    isActive: () => state.tab === 'graph',
    getState: () => state,
    showTip,
    hideTip,
    onSelectNode: id => {
      state.selectedNodeId = id;
      state.selectedEdgeId = null;
      state.selectedEdge = null;
      if (state.tab === 'graph') ArchGraph.draw(state);
      else renderActiveTab(true);
      renderInspectorPanel();
    },
    onSelectViewNode: viewNodeId => {
      const hid = resolveViewNodeToHierarchyId(viewNodeId);
      if (hid) {
        state.selectedNodeId = hid;
        state.selectedEdgeId = null;
        state.selectedEdge = null;
        renderActiveTab(true);
        renderInspectorPanel();
      }
    },
    onSelectFile: filePath => {
      const fid = `file:${filePath}`;
      if (hierarchy.nodes.has(fid)) {
        state.selectedNodeId = fid;
        state.selectedEdgeId = null;
        state.selectedEdge = null;
        ensureAncestorsExpanded(fid);
        graphDirty = true;
        renderActiveTab(true);
        renderInspectorPanel();
      }
    },
    onSelectEdge: edge => {
      state.selectedEdge = edge;
      state.selectedEdgeId = edge.id;
      if (state.tab === 'graph') ArchGraph.draw(state);
      renderInspectorPanel();
    },
    onClearSelection: () => {
      state.selectedNodeId = null;
      state.selectedEdgeId = null;
      state.selectedEdge = null;
      state.pathFromId = null;
      state.pathToId = null;
      state.pathResults = [];
      if (state.tab === 'graph') ArchGraph.draw(state);
      renderInspectorPanel();
    },
    onToggleExpand: id => {
      if (state.expandedIds.has(id)) state.expandedIds.delete(id);
      else state.expandedIds.add(id);
      ArchGraph.clearManualPositions();
      renderCompoundGraph(false);
    },
    onDsmToggle: (id, expand) => {
      if (expand) state.dsmExpandedIds.add(id);
      else state.dsmExpandedIds.delete(id);
      ArchDsm.render(hierarchy, state, callbacks);
    },
    onOpenEgo: id => {
      state.selectedNodeId = id;
      state.selectedEdgeId = null;
      state.selectedEdge = null;
      state.graphLayout = 'ego';
      updateLayoutButtons();
      ensureAncestorsExpanded(id);
      graphDirty = true;
      ArchGraph.clearManualPositions();
      switchTab('graph');
      renderCompoundGraph(false);
    },
    onFocusEdgeInGraph: edge => {
      ensureAncestorsExpanded(edge.from);
      ensureAncestorsExpanded(edge.to);
      state.selectedEdge = edge;
      state.selectedEdgeId = edge.id;
      graphDirty = true;
      switchTab('graph');
      renderCompoundGraph(false);
    },
    onSetPathStart: id => {
      state.pathFromId = id;
      state.pathToId = null;
      state.pathResults = [];
      updateGraphHud();
      renderInspectorPanel();
    },
    onSetPathEnd: id => {
      if (!state.pathFromId) state.pathFromId = state.selectedNodeId;
      state.pathToId = id;
      state.selectedEdge = null;
      state.selectedEdgeId = null;
      if (state.tab !== 'graph') switchTab('graph');
      renderCompoundGraph(true);
    },
    onShiftSelectNode: id => {
      if (!state.pathFromId) {
        state.pathFromId = state.selectedNodeId || id;
        if (state.pathFromId !== id) state.pathToId = id;
      } else if (state.pathFromId === id) {
        state.pathFromId = null;
        state.pathToId = null;
      } else {
        state.pathToId = id;
      }
      renderCompoundGraph(true);
    },
    onClearPath: () => {
      state.pathFromId = null;
      state.pathToId = null;
      state.pathResults = [];
      renderCompoundGraph(true);
    },
    onSwapPath: () => {
      const tmp = state.pathFromId;
      state.pathFromId = state.pathToId;
      state.pathToId = tmp;
      renderCompoundGraph(true);
    },
    onToggleCut: fileEdgeKey => {
      if (state.whatIf.cutEdges.has(fileEdgeKey)) state.whatIf.cutEdges.delete(fileEdgeKey);
      else state.whatIf.cutEdges.add(fileEdgeKey);
      rebuildHierarchy();
      renderBanners();
      renderActiveTab(true);
    },
    onToggleCutBatch: (keys, shouldCut) => {
      for (const k of keys) {
        if (shouldCut) state.whatIf.cutEdges.add(k);
        else state.whatIf.cutEdges.delete(k);
      }
      rebuildHierarchy();
      renderBanners();
      renderActiveTab(true);
    },
    onSimulateMove: (filePath, newModule) => {
      const orig = baselineHierarchy.fileMap.get(filePath)?.module_id;
      if (orig === newModule) state.whatIf.movedFiles.delete(filePath);
      else state.whatIf.movedFiles.set(filePath, newModule);
      rebuildHierarchy();
      renderBanners();
      ArchGraph.clearManualPositions();
      renderActiveTab(false);
    },
  };

  function renderInspectorPanel() {
    ArchInspector.render(REPORT, hierarchy, ArchGraph.getEdges(), state, callbacks);
  }

  function renderActiveTab(preserveCamera = false) {
    const view = currentView();
    const fNodes = filteredViewNodes(view);
    if (state.tab === 'graph') {
      if (graphDirty || !preserveCamera) renderCompoundGraph(preserveCamera);
      else ArchGraph.draw(state);
    } else if (state.tab === 'dsm') {
      ArchDsm.render(hierarchy, state, callbacks);
    } else if (state.tab === 'mainseq') {
      ArchCharts.renderMainSeq(REPORT, view, fNodes, state, callbacks);
    } else if (state.tab === 'treemap') {
      ArchCharts.renderTreemap(REPORT, hierarchy, state, callbacks);
    } else if (state.tab === 'diagnostics') {
      ArchCharts.renderDiagnostics(REPORT, view, fNodes, state, callbacks);
    }
    renderInspectorPanel();
  }

  function renderAll(preserveCamera = false) {
    renderBanners();
    renderKpis();
    renderActiveTab(preserveCamera);
  }

  function updateLayoutButtons() {
    document.getElementById('layout-sugiyama')?.classList.toggle('active', state.graphLayout === 'sugiyama');
    document.getElementById('layout-ego')?.classList.toggle('active', state.graphLayout === 'ego');
    document.getElementById('layout-force')?.classList.toggle('active', state.graphLayout === 'force');
  }

  $viewSel.addEventListener('change', () => { state.viewId = $viewSel.value; renderAll(true); });
  $langSel.addEventListener('change', () => { state.lang = $langSel.value; graphDirty = true; renderAll(false); });
  let searchTimer = null;
  $search.addEventListener('input', () => {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => {
      state.search = $search.value;
      graphDirty = true;
      renderAll(false);
    }, 80);
  });

  function applyTheme(next) {
    document.documentElement.setAttribute('data-theme', next);
    savePref('theme', next);
    syncThemeButton();
    ArchGraph.refreshTheme(state);
    if (state.tab !== 'graph') renderActiveTab(true);
  }

  $themeBtn.addEventListener('click', () => {
    const cur = document.documentElement.getAttribute('data-theme');
    const next = cur === 'dark' ? 'light' : 'dark';
    const reduceMotion = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (!reduceMotion && typeof document.startViewTransition === 'function') {
      document.startViewTransition(() => applyTheme(next));
    } else {
      applyTheme(next);
    }
  });

  document.querySelectorAll('.tab-btn').forEach(btn => {
    btn.addEventListener('click', () => switchTab(btn.dataset.tab));
  });

  // Контролы графа
  document.getElementById('layout-sugiyama').addEventListener('click', () => {
    state.graphLayout = 'sugiyama';
    updateLayoutButtons();
    ArchGraph.clearManualPositions();
    renderCompoundGraph(false);
  });
  document.getElementById('layout-ego').addEventListener('click', () => {
    state.graphLayout = 'ego';
    updateLayoutButtons();
    ArchGraph.clearManualPositions();
    renderCompoundGraph(false);
  });
  document.getElementById('layout-force').addEventListener('click', () => {
    state.graphLayout = 'force';
    updateLayoutButtons();
    ArchGraph.clearManualPositions();
    renderCompoundGraph(false);
  });
  document.getElementById('orient-toggle').addEventListener('click', e => {
    state.graphOrient = state.graphOrient === 'TB' ? 'LR' : 'TB';
    savePref('orient', state.graphOrient);
    e.target.textContent = state.graphOrient === 'TB' ? 'Сверху ↓ Вниз' : 'Слева → Направо';
    ArchGraph.clearManualPositions();
    renderCompoundGraph(false);
  });
  document.getElementById('color-mode').addEventListener('change', e => {
    state.colorMode = e.target.value;
    ArchGraph.draw(state);
  });
  document.getElementById('edge-filter').addEventListener('change', e => {
    state.edgeFilter = e.target.value;
    renderCompoundGraph(false);
  });
  document.getElementById('chk-group-pkg').addEventListener('change', e => {
    state.groupByPackage = e.target.checked;
    savePref('groupPkg', state.groupByPackage);
    state.expandedIds.clear();
    if (state.groupByPackage) {
      for (const r of hierarchy.roots) {
        if (hierarchy.nodes.get(r)?.kind === 'package') state.expandedIds.add(r);
      }
    }
    ArchGraph.clearManualPositions();
    renderCompoundGraph(false);
  });
  document.getElementById('chk-show-iso').addEventListener('change', e => {
    state.hideIsolated = !e.target.checked;
    savePref('showIso', e.target.checked);
    ArchGraph.clearManualPositions();
    renderCompoundGraph(false);
  });
  document.getElementById('chk-temporal').addEventListener('change', e => {
    state.showTemporal = e.target.checked;
    renderCompoundGraph(true);
  });
  document.getElementById('graph-expand-cycles').addEventListener('click', () => {
    const modView = REPORT.views.find(v => v.id === 'modules');
    if (modView) {
      for (const cyc of modView.cycles) {
        for (const m of cyc.nodes) {
          const mid = `mod:${m}`;
          if (hierarchy.nodes.has(mid)) {
            ensureAncestorsExpanded(mid);
            state.expandedIds.add(mid);
          }
        }
      }
    }
    ArchGraph.clearManualPositions();
    renderCompoundGraph(false);
  });
  document.getElementById('graph-collapse-all').addEventListener('click', () => {
    state.expandedIds.clear();
    if (state.groupByPackage) {
      for (const r of hierarchy.roots) {
        if (hierarchy.nodes.get(r)?.kind === 'package') state.expandedIds.add(r);
      }
    }
    ArchGraph.clearManualPositions();
    renderCompoundGraph(false);
  });
  document.getElementById('graph-fit').addEventListener('click', () => ArchGraph.fitView(state));
  document.getElementById('zoom-in').addEventListener('click', () => ArchGraph.zoomBy(1.22, state));
  document.getElementById('zoom-out').addEventListener('click', () => ArchGraph.zoomBy(0.82, state));

  // Симуляция рефакторинга
  document.getElementById('btn-whatif-reset').addEventListener('click', () => {
    state.whatIf.cutEdges.clear();
    state.whatIf.movedFiles.clear();
    rebuildHierarchy();
    renderAll(true);
  });
  document.getElementById('btn-whatif-export').addEventListener('click', e => {
    const lines = ['# План рефакторинга (archlens)', ''];
    const bm = baselineHierarchy.moduleMetrics;
    const cm = hierarchy.moduleMetrics;
    lines.push(`- Циклы: ${bm.cycles.length} -> ${cm.cycles.length}`);
    lines.push(`- Нарушения стабильности: ${bm.sdpCount} -> ${cm.sdpCount}`);
    lines.push(`- Охват изменений: ${(bm.propCost * 100).toFixed(1)}% -> ${(cm.propCost * 100).toFixed(1)}%`, '');
    if (state.whatIf.cutEdges.size > 0) {
      lines.push('## Разорвать зависимости');
      for (const key of state.whatIf.cutEdges) {
        const [from, to] = JSON.parse(key);
        lines.push(`- Убрать импорт: \`${from}→${to}\``);
      }
      lines.push('');
    }
    if (state.whatIf.movedFiles.size > 0) {
      lines.push('## Перенести файлы');
      for (const [f, m] of state.whatIf.movedFiles.entries()) lines.push(`- Перенести \`${f}\` -> \`${m}\``);
    }
    const text = lines.join('\n');
    const btn = e.currentTarget;
    if (navigator.clipboard) navigator.clipboard.writeText(text).catch(() => {});
    const orig = btn.textContent;
    btn.textContent = 'Скопировано ✓';
    setTimeout(() => { btn.textContent = orig; }, 1800);
  });

  document.getElementById('chk-labels').addEventListener('change', e => { state.showLabels = e.target.checked; renderActiveTab(true); });
  document.getElementById('size-mode').addEventListener('change', e => { state.sizeMode = e.target.value; renderActiveTab(true); });
  document.getElementById('treemap-area').addEventListener('change', e => { state.treemapArea = e.target.value; renderActiveTab(true); });
  document.getElementById('treemap-color').addEventListener('change', e => { state.treemapColor = e.target.value; renderActiveTab(true); });

  document.getElementById('dsm-collapse-all').addEventListener('click', () => {
    state.dsmExpandedIds.clear();
    for (const r of hierarchy.roots) {
      if (hierarchy.nodes.get(r)?.kind === 'package') state.dsmExpandedIds.add(r);
    }
    ArchDsm.render(hierarchy, state, callbacks);
  });

  document.querySelectorAll('#metrics-table th[data-sort]').forEach(th => {
    th.addEventListener('click', () => {
      const k = th.dataset.sort;
      if (state.sortKey === k) state.sortAsc = !state.sortAsc;
      else { state.sortKey = k; state.sortAsc = k === 'id'; }
      renderActiveTab(true);
    });
  });

  ArchGraph.wireInteractions(callbacks);
  renderAll(false);
})();
