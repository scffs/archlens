// High-performance 60fps HiDPI Compound Graph Canvas with port-distributed cubic S-curves.

const ArchGraph = (() => {
  const PALETTE = [
    '#5f5f5f', '#2ba471', '#e37318', '#366ef4', '#6d28d9',
    '#d54941', '#0d9488', '#8b8b8b', '#a75d2b', '#4582e6',
  ];

  const g = {
    nodes: [],
    edges: [],
    nodeById: new Map(),
    boxMap: new Map(),
    edgeGeoms: [],
    manualPos: new Map(),
    cam: { x: 0, y: 0, scale: 1 },
    hoverNodeId: null,
    hoverEdgeId: null,
    draggingId: null,
    dragLastWorld: { x: 0, y: 0 },
    panning: false,
    panStart: { x: 0, y: 0 },
    hasCameraInit: false,
    // Performance caches (zero getComputedStyle or getBoundingClientRect per frame!)
    theme: null,
    pkgColorMap: new Map(),
    rect: { left: 0, top: 0, width: 900, height: 640 },
    rafPending: false,
    lastAppState: null,
    blastCacheKey: '',
    blastCacheSet: null,
  };

  function readTheme() {
    const cs = getComputedStyle(document.documentElement);
    const v = name => cs.getPropertyValue(name).trim();
    const isDark = document.documentElement.getAttribute('data-theme') === 'dark';
    g.theme = {
      isDark,
      border: v('--td-component-stroke'),
      borderStrong: v('--td-component-border'),
      surface: v('--td-bg-color-container'),
      subtle: v('--td-bg-color-secondarycontainer'),
      text: v('--td-text-color-primary'),
      muted: v('--td-text-color-secondary'),
      red: v('--td-error-color'),
      redBg: v('--td-error-color-1'),
      amber: v('--td-warning-color'),
      teal: v('--td-dip-color'),
      green: v('--td-success-color'),
      magenta: v('--td-error-color'),
      purple: v('--td-purple-color'),
      cyan: v('--td-info-color'),
      accent: v('--td-brand-color'),
      slate: isDark ? '#777777' : '#8b8b8b',
      shadow: isDark ? 'rgba(0,0,0,0.32)' : 'rgba(0,0,0,0.06)',
      containerBg: isDark ? 'rgba(36,36,36,0.78)' : 'rgba(255,255,255,0.82)',
    };
  }

  function refreshTheme(appState) {
    readTheme();
    if (appState) scheduleDraw(appState);
  }

  function updateCanvasRect() {
    const canvas = document.getElementById('graph-canvas');
    if (!canvas) return;
    const r = canvas.getBoundingClientRect();
    if (r.width > 0 && r.height > 0) {
      g.rect = { left: r.left, top: r.top, width: r.width, height: r.height };
    }
  }

  function pkgColor(pkgName) {
    if (!pkgName) return g.theme.slate;
    if (!g.pkgColorMap.has(pkgName)) {
      g.pkgColorMap.set(pkgName, PALETTE[g.pkgColorMap.size % PALETTE.length]);
    }
    return g.pkgColorMap.get(pkgName);
  }

  function colorForNode(vn, colorMode) {
    const t = g.theme;
    const m = vn.metrics || {};
    const d = vn.data || {};
    if (colorMode === 'hotspot') {
      const h = d.hotspot || 0;
      if (h >= 0.65) return t.red;
      if (h >= 0.35) return t.amber;
      if (h >= 0.15) return t.cyan;
      return t.green;
    }
    if (colorMode === 'louvain') return PALETTE[(d.louvainCluster || 0) % PALETTE.length];
    if (colorMode === 'package') return pkgColor(vn.pkgName || d.pkgName);
    if (colorMode === 'language') {
      const lang = (d.language || '').toLowerCase();
      const map = { rust: '#e37318', typescript: '#0052d9', vue: '#008858', javascript: '#ca8a04', python: '#6d28d9', go: '#0891b2' };
      return map[lang] || t.slate;
    }
    if (m.inCycle || m.activePain) return t.red;
    if (m.zone === 'zone_of_pain') return t.amber;
    if (m.zone === 'zone_of_uselessness') return t.purple;
    if (m.zone === 'main_sequence') return t.green;
    if (vn.kind === 'symbol') return d.symbolDecl?.is_abstract ? t.teal : t.accent;
    return t.slate;
  }

  // Precompute clean distributed port positions and smooth cubic Bézier geometry for every edge
  function buildEdgeGeometries(appState) {
    const isLR = appState.graphLayout === 'ego' || (appState.graphLayout === 'sugiyama' && appState.graphOrient === 'LR');
    const isForce = appState.graphLayout === 'force';

    // Group edges per node port side so exit/entry ports fan out smoothly sorted by neighbor position
    const outPorts = new Map();
    const inPorts = new Map();

    for (const e of g.edges) {
      const ba = g.boxMap.get(e.from);
      const bb = g.boxMap.get(e.to);
      if (!ba || !bb) continue;
      if (!outPorts.has(e.from)) outPorts.set(e.from, []);
      if (!inPorts.has(e.to)) inPorts.set(e.to, []);
      outPorts.get(e.from).push({ edge: e, other: bb });
      inPorts.get(e.to).push({ edge: e, other: ba });
    }

    for (const list of outPorts.values()) {
      list.sort((a, b) => (isLR ? a.other.cy - b.other.cy : a.other.cx - b.other.cx));
    }
    for (const list of inPorts.values()) {
      list.sort((a, b) => (isLR ? a.other.cy - b.other.cy : a.other.cx - b.other.cx));
    }

    const outIdx = new Map();
    const inIdx = new Map();
    for (const [nid, list] of outPorts.entries()) {
      list.forEach((it, idx) => outIdx.set(`${nid}|${it.edge.id}`, { idx, total: list.length }));
    }
    for (const [nid, list] of inPorts.entries()) {
      list.forEach((it, idx) => inIdx.set(`${nid}|${it.edge.id}`, { idx, total: list.length }));
    }

    const portFrac = info => {
      if (!info || info.total <= 1) return 0.5;
      return 0.16 + (0.68 * info.idx) / (info.total - 1);
    };

    const geoms = [];
    for (const e of g.edges) {
      const ba = g.boxMap.get(e.from);
      const bb = g.boxMap.get(e.to);
      if (!ba || !bb) continue;

      let x1, y1, cx1, cy1, cx2, cy2, x2, y2;

      if (isForce) {
        const dx = bb.cx - ba.cx;
        const dy = bb.cy - ba.cy;
        const dist = Math.hypot(dx, dy) || 1;
        const ux = dx / dist, uy = dy / dist;
        x1 = ba.cx + ux * (ba.w * 0.42);
        y1 = ba.cy + uy * (ba.h * 0.42);
        x2 = bb.cx - ux * (bb.w * 0.48);
        y2 = bb.cy - uy * (bb.h * 0.48);
        const bend = e.isCycleEdge ? 38 : 14;
        const mx = (x1 + x2) / 2 - uy * bend;
        const my = (y1 + y2) / 2 + ux * bend;
        cx1 = mx; cy1 = my; cx2 = mx; cy2 = my;
      } else if (!isLR) {
        // Top -> Down architectural layered routing
        const pfOut = portFrac(outIdx.get(`${e.from}|${e.id}`));
        const pfIn = portFrac(inIdx.get(`${e.to}|${e.id}`));
        const goingUp = bb.cy < ba.cy - 12;
        const sameRow = Math.abs(bb.cy - ba.cy) <= 12;

        if (goingUp) {
          // Upward feedback / cycle edge: exit & enter from the outer right/left side cleanly!
          const goRight = ba.cx >= bb.cx;
          x1 = goRight ? ba.x + ba.w : ba.x;
          y1 = ba.cy;
          x2 = goRight ? bb.x + bb.w : bb.x;
          y2 = bb.cy;
          const span = Math.max(64, Math.min(140, Math.abs(ba.cy - bb.cy) * 0.45));
          const sideX = goRight ? Math.max(x1, x2) + span : Math.min(x1, x2) - span;
          cx1 = sideX; cy1 = y1;
          cx2 = sideX; cy2 = y2;
        } else if (sameRow) {
          // Same-layer edge: gentle arc cleanly over top
          x1 = ba.x + ba.w * pfOut;
          y1 = ba.y;
          x2 = bb.x + bb.w * pfIn;
          y2 = bb.y;
          const lift = Math.min(62, 26 + Math.abs(x2 - x1) * 0.14);
          cx1 = x1; cy1 = y1 - lift;
          cx2 = x2; cy2 = y2 - lift;
        } else {
          // Normal downward S-curve from bottom port to top port
          x1 = ba.x + ba.w * pfOut;
          y1 = ba.y + ba.h;
          x2 = bb.x + bb.w * pfIn;
          y2 = bb.y;
          const dy = Math.max(36, y2 - y1);
          const ctrl = Math.min(130, dy * 0.48);
          cx1 = x1; cy1 = y1 + ctrl;
          cx2 = x2; cy2 = y2 - ctrl;
        }
      } else {
        // Left -> Right layered / Ego routing
        const pfOut = portFrac(outIdx.get(`${e.from}|${e.id}`));
        const pfIn = portFrac(inIdx.get(`${e.to}|${e.id}`));
        const goingLeft = bb.cx < ba.cx - 12;

        if (goingLeft) {
          x1 = ba.cx; y1 = ba.y;
          x2 = bb.cx; y2 = bb.y;
          const lift = Math.max(54, Math.min(125, Math.abs(x1 - x2) * 0.25));
          const topY = Math.min(y1, y2) - lift;
          cx1 = x1; cy1 = topY;
          cx2 = x2; cy2 = topY;
        } else {
          x1 = ba.x + ba.w;
          y1 = ba.y + ba.h * pfOut;
          x2 = bb.x;
          y2 = bb.y + bb.h * pfIn;
          const dx = Math.max(40, x2 - x1);
          const ctrl = Math.min(140, dx * 0.48);
          cx1 = x1 + ctrl; cy1 = y1;
          cx2 = x2 - ctrl; cy2 = y2;
        }
      }

      const minX = Math.min(x1, cx1, cx2, x2) - 10;
      const maxX = Math.max(x1, cx1, cx2, x2) + 10;
      const minY = Math.min(y1, cy1, cy2, y2) - 10;
      const maxY = Math.max(y1, cy1, cy2, y2) + 10;
      // Midpoint of cubic Bézier at t = 0.5
      const midX = 0.125 * x1 + 0.375 * cx1 + 0.375 * cx2 + 0.125 * x2;
      const midY = 0.125 * y1 + 0.375 * cy1 + 0.375 * cy2 + 0.125 * y2;

      geoms.push({ edge: e, x1, y1, cx1, cy1, cx2, cy2, x2, y2, midX, midY, minX, maxX, minY, maxY });
    }
    g.edgeGeoms = geoms;
  }

  function rebuild(projected, appState, preserveCamera = false) {
    if (!g.theme) readTheme();
    updateCanvasRect();

    g.nodes = projected.nodes;
    g.edges = projected.edges;
    g.nodeById = new Map(g.nodes.map(n => [n.id, n]));
    g.blastCacheKey = '';
    g.blastCacheSet = null;

    let computed;
    if (appState.graphLayout === 'ego') {
      computed = ArchLayout.computeEgoLayout(g.nodes, g.edges, appState.selectedNodeId);
    } else if (appState.graphLayout === 'force') {
      computed = ArchLayout.computeClusterForce(g.nodes, g.edges);
    } else {
      computed = ArchLayout.computeCompoundSugiyama(g.nodes, g.edges, appState.graphOrient);
    }

    if (preserveCamera && g.manualPos.size > 0) {
      for (const [id, pos] of g.manualPos.entries()) {
        const b = computed.get(id);
        if (b && !b.isContainer) {
          b.x = pos.x;
          b.y = pos.y;
          b.cx = b.x + b.w / 2;
          b.cy = b.y + b.h / 2;
        }
      }
      ArchLayout.wrapExpandedContainers(g.nodes, computed);
    }

    g.boxMap = computed;
    buildEdgeGeometries(appState);

    if (!preserveCamera || !g.hasCameraInit) {
      fitView(appState);
      g.hasCameraInit = true;
    } else {
      scheduleDraw(appState);
    }
  }

  function clearManualPositions() {
    g.manualPos.clear();
  }

  function zoomBy(factor, appState) {
    const cx = g.rect.width / 2;
    const cy = g.rect.height / 2;
    const nextScale = Math.max(0.22, Math.min(3.0, g.cam.scale * factor));
    g.cam.x = cx - ((cx - g.cam.x) / g.cam.scale) * nextScale;
    g.cam.y = cy - ((cy - g.cam.y) / g.cam.scale) * nextScale;
    g.cam.scale = nextScale;
    scheduleDraw(appState);
  }

  function fitView(appState) {
    updateCanvasRect();
    const { width, height } = g.rect;
    if (!width || g.boxMap.size === 0) return;

    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    for (const b of g.boxMap.values()) {
      minX = Math.min(minX, b.x);
      maxX = Math.max(maxX, b.x + b.w);
      minY = Math.min(minY, b.y);
      maxY = Math.max(maxY, b.y + b.h);
    }
    const bw = Math.max(160, maxX - minX + 70);
    const bh = Math.max(140, maxY - minY + 70);
    const scale = Math.max(0.28, Math.min(1.18, Math.min((width - 40) / bw, (height - 48) / bh)));
    g.cam.scale = scale;
    g.cam.x = width / 2 - ((minX + maxX) / 2) * scale;
    g.cam.y = height / 2 - ((minY + maxY) / 2) * scale;
    if (appState) scheduleDraw(appState);
  }

  function scheduleDraw(appState) {
    g.lastAppState = appState;
    if (g.rafPending) return;
    g.rafPending = true;
    requestAnimationFrame(() => {
      g.rafPending = false;
      if (g.lastAppState) drawNow(g.lastAppState);
    });
  }

  function drawRoundedRect(ctx, x, y, w, h, r) {
    const rad = Math.min(r, w / 2, h / 2);
    ctx.beginPath();
    ctx.moveTo(x + rad, y);
    ctx.arcTo(x + w, y, x + w, y + h, rad);
    ctx.arcTo(x + w, y + h, x, y + h, rad);
    ctx.arcTo(x, y + h, x, y, rad);
    ctx.arcTo(x, y, x + w, y, rad);
    ctx.closePath();
  }

  function getActiveFocusSet(appState) {
    if (appState.pathResults && appState.pathResults.length > 0) {
      const nodeSet = new Set();
      const pathEdgeKeys = new Set();
      for (const p of appState.pathResults) {
        for (let i = 0; i < p.length; i++) {
          nodeSet.add(p[i]);
          if (i + 1 < p.length) pathEdgeKeys.add(`${p[i]}→${p[i + 1]}`);
        }
      }
      return { hasHighlight: true, nodeSet, pathEdgeKeys, focusId: null };
    }
    if (appState.selectedEdgeId) {
      const selE = g.edges.find(e => e.id === appState.selectedEdgeId);
      if (selE) {
        return { hasHighlight: true, nodeSet: new Set([selE.from, selE.to]), pathEdgeKeys: new Set(), focusId: null };
      }
    }
    const focusId = appState.selectedNodeId || g.hoverNodeId;
    if (focusId && g.boxMap.has(focusId)) {
      const key = `${focusId}|${appState.blastDepth}|${appState.blastDir}|${g.edges.length}`;
      if (g.blastCacheKey !== key) {
        const depth = appState.blastDepth === 'inf' ? Infinity : Number(appState.blastDepth || 1);
        const blast = ArchModel.computeBlastRadius(g.edges, focusId, depth, appState.blastDir || 'both');
        const set = new Set(blast.keys());
        for (const id of [...set]) {
          let p = g.nodeById.get(id)?.parentId;
          while (p) { set.add(p); p = g.nodeById.get(p)?.parentId; }
        }
        g.blastCacheKey = key;
        g.blastCacheSet = set;
      }
      return { hasHighlight: true, nodeSet: g.blastCacheSet, pathEdgeKeys: new Set(), focusId };
    }
    return { hasHighlight: false, nodeSet: null, pathEdgeKeys: new Set(), focusId: null };
  }

  function drawNow(appState) {
    const canvas = document.getElementById('graph-canvas');
    if (!canvas) return;
    if (!g.theme) readTheme();
    const { width, height } = g.rect;
    if (!width || !height) return;

    const dpr = Math.min(2, window.devicePixelRatio || 1);
    const targetW = Math.round(width * dpr);
    const targetH = Math.round(height * dpr);
    if (canvas.width !== targetW || canvas.height !== targetH) {
      canvas.width = targetW;
      canvas.height = targetH;
    }

    const ctx = canvas.getContext('2d');
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);

    const t = g.theme;
    const { hasHighlight, nodeSet: activeNodeSet, pathEdgeKeys, focusId } = getActiveFocusSet(appState);

    ctx.save();
    ctx.translate(g.cam.x, g.cam.y);
    ctx.scale(g.cam.scale, g.cam.scale);

    // 1. Expanded compound containers
    const containers = g.nodes.filter(n => n.isExpanded && g.boxMap.has(n.id)).sort((a, b) => a.depth - b.depth);
    for (const vn of containers) {
      const b = g.boxMap.get(vn.id);
      const dimmed = hasHighlight && !activeNodeSet.has(vn.id);
      const isSel = vn.id === appState.selectedNodeId;
      ctx.globalAlpha = dimmed ? 0.25 : 1;

      drawRoundedRect(ctx, b.x, b.y, b.w, b.h, 10);
      ctx.fillStyle = t.containerBg;
      ctx.fill();
      ctx.lineWidth = isSel ? 2.2 : 1.2;
      ctx.strokeStyle = isSel ? t.accent : t.border;
      ctx.stroke();

      // Header band
      ctx.fillStyle = t.subtle;
      ctx.fillRect(b.x + 1, b.y + 1, b.w - 2, ArchLayout.HEADER_H - 1);
      ctx.beginPath();
      ctx.moveTo(b.x, b.y + ArchLayout.HEADER_H);
      ctx.lineTo(b.x + b.w, b.y + ArchLayout.HEADER_H);
      ctx.strokeStyle = t.border;
      ctx.lineWidth = 1;
      ctx.stroke();

      ctx.fillStyle = t.accent;
      ctx.font = '700 13px -apple-system, BlinkMacSystemFont, sans-serif';
      ctx.textAlign = 'left';
      ctx.textBaseline = 'middle';
      ctx.fillText('▾', b.x + 12, b.y + ArchLayout.HEADER_H / 2);

      ctx.fillStyle = t.text;
      ctx.font = '600 13px -apple-system, BlinkMacSystemFont, sans-serif';
      ctx.fillText(`${vn.label} (${vn.data.files.length})`, b.x + 28, b.y + ArchLayout.HEADER_H / 2);
    }

    // 2. Edges (calm background lines first, highlighted / violation lines on top)
    const drawEdgePass = highlightPass => {
      for (const eg of g.edgeGeoms) {
        const e = eg.edge;
        const isPath = pathEdgeKeys.has(`${e.from}→${e.to}`);
        const isSelectedEdge = e.id === appState.selectedEdgeId;
        const isHoveredEdge = e.id === g.hoverEdgeId;
        const isIncident = focusId && (e.from === focusId || e.to === focusId);
        const isImportant = isPath || isSelectedEdge || isHoveredEdge || isIncident || e.isCycleEdge || e.isRuleViolation;
        if (highlightPass !== isImportant) continue;

        const isFocus =
          !hasHighlight ||
          isPath ||
          isSelectedEdge ||
          isHoveredEdge ||
          (activeNodeSet && activeNodeSet.has(e.from) && activeNodeSet.has(e.to));

        let stroke = t.slate;
        let dash = null;
        let lw = Math.min(3.5, 1.3 + Math.log2(Math.max(1, e.weight)) * 0.65);

        if (e.isAllCut) {
          stroke = t.muted; dash = [5, 5]; lw = 1.2;
        } else if (isPath) {
          stroke = t.cyan; lw = 3.2;
        } else if (e.isTemporal) {
          stroke = t.purple; dash = [4, 5]; lw = 1.7;
        } else if (e.isRuleViolation) {
          stroke = t.magenta; dash = [6, 3]; lw = Math.max(2.4, lw);
        } else if (e.isCycleEdge) {
          stroke = t.red; lw = Math.max(2.3, lw);
        } else if (e.isSdpViolation) {
          stroke = t.amber; lw = Math.max(2.0, lw);
        } else if (e.isDipOnly) {
          stroke = t.teal; dash = [5, 3];
        } else if (focusId && e.to === focusId) {
          stroke = t.green; lw = Math.max(2.0, lw);
        } else if (focusId && e.from === focusId) {
          stroke = t.accent; lw = Math.max(2.0, lw);
        }

        if (isSelectedEdge || isHoveredEdge) lw = Math.max(lw + 1.3, 3.2);

        const alpha = e.isAllCut
          ? 0.25
          : isPath || isSelectedEdge || isHoveredEdge
          ? 0.98
          : isFocus
          ? isIncident || e.isCycleEdge || e.isRuleViolation || e.isSdpViolation
            ? 0.9
            : hasHighlight
            ? 0.55
            : 0.34
          : 0.06;

        ctx.globalAlpha = alpha;
        ctx.strokeStyle = stroke;
        ctx.lineWidth = lw;
        ctx.setLineDash(dash || []);

        ctx.beginPath();
        ctx.moveTo(eg.x1, eg.y1);
        ctx.bezierCurveTo(eg.cx1, eg.cy1, eg.cx2, eg.cy2, eg.x2, eg.y2);
        ctx.stroke();

        // Crisp arrowhead aligned to end tangent (x2 - cx2, y2 - cy2)
        if (!e.isTemporal) {
          const ang = Math.atan2(eg.y2 - eg.cy2, eg.x2 - eg.cx2);
          const head = 7.5 + Math.min(2.5, lw * 0.45);
          ctx.setLineDash([]);
          ctx.fillStyle = stroke;
          ctx.beginPath();
          ctx.moveTo(eg.x2, eg.y2);
          ctx.lineTo(eg.x2 - head * Math.cos(ang - 0.36), eg.y2 - head * Math.sin(ang - 0.36));
          ctx.lineTo(eg.x2 - head * Math.cos(ang + 0.36), eg.y2 - head * Math.sin(ang + 0.36));
          ctx.closePath();
          ctx.fill();
        }

        // Show weight pill ONLY when hovered, selected, path, incident to focus, or violation (zero clutter!)
        if (
          (isSelectedEdge || isHoveredEdge || isPath || isIncident || e.isCycleEdge || e.isRuleViolation) &&
          isFocus &&
          g.cam.scale >= 0.48
        ) {
          const badgeText = e.isAllCut
            ? 'CUT'
            : e.isTemporal
            ? `git:${e.weight}`
            : e.isRuleViolation
            ? `! ${e.weight}`
            : String(e.weight);
          ctx.font = '700 11px ui-monospace, SFMono-Regular, Menlo, monospace';
          const tw = badgeText.length * 7 + 12;
          ctx.setLineDash([]);
          drawRoundedRect(ctx, eg.midX - tw / 2, eg.midY - 9.5, tw, 19, 5);
          ctx.fillStyle = t.surface;
          ctx.fill();
          ctx.lineWidth = 1.2;
          ctx.strokeStyle = stroke;
          ctx.stroke();
          ctx.fillStyle = stroke;
          ctx.textAlign = 'center';
          ctx.textBaseline = 'middle';
          ctx.fillText(badgeText, eg.midX, eg.midY + 0.5);
        }
      }
    };
    drawEdgePass(false);
    drawEdgePass(true);
    ctx.setLineDash([]);

    // 3. Leaf node cards (clean, fast, zero ctx.clip() overhead!)
    const leaves = g.nodes.filter(n => !n.isExpanded && g.boxMap.has(n.id));
    for (const vn of leaves) {
      const b = g.boxMap.get(vn.id);
      const accentCol = colorForNode(vn, appState.colorMode);
      const isSel = vn.id === appState.selectedNodeId;
      const isPathEnd = vn.id === appState.pathFromId || vn.id === appState.pathToId;
      const isHov = vn.id === g.hoverNodeId;
      const dimmed = hasHighlight && !activeNodeSet.has(vn.id);

      ctx.globalAlpha = dimmed ? 0.18 : 1;

      // Soft elevation shadow (cheap rect offset)
      if (!dimmed) {
        drawRoundedRect(ctx, b.x, b.y + 2, b.w, b.h, vn.kind === 'symbol' ? 16 : 8);
        ctx.fillStyle = t.shadow;
        ctx.fill();
      }

      drawRoundedRect(ctx, b.x, b.y, b.w, b.h, vn.kind === 'symbol' ? 16 : 8);
      ctx.fillStyle = t.surface;
      ctx.fill();
      ctx.lineWidth = isPathEnd ? 2.6 : isSel ? 2.4 : isHov ? 1.8 : 1.1;
      ctx.strokeStyle = isPathEnd ? t.cyan : isSel ? t.accent : isHov ? accentCol : t.border;
      ctx.stroke();

      // Left status stripe pill (no clip needed!)
      if (vn.kind !== 'symbol') {
        drawRoundedRect(ctx, b.x + 5, b.y + 7, 4, b.h - 14, 2);
        ctx.fillStyle = accentCol;
        ctx.fill();
      }

      ctx.textAlign = 'left';
      ctx.textBaseline = 'middle';

      if (vn.kind === 'symbol') {
        const tag = vn.data.symbolDecl?.is_abstract ? '◇ ' : '▪ ';
        const maxC = Math.max(8, Math.floor((b.w - 30) / 7.8));
        const raw = vn.label || vn.id;
        const lbl = raw.length > maxC ? raw.slice(0, maxC - 1) + '…' : raw;
        ctx.font = '600 12.5px ui-monospace, SFMono-Regular, Menlo, monospace';
        ctx.fillStyle = accentCol;
        ctx.fillText(tag, b.x + 11, b.y + b.h / 2);
        ctx.fillStyle = t.text;
        ctx.fillText(lbl, b.x + 25, b.y + b.h / 2);
      } else {
        const d = vn.data || {};
        const hasHot = d.hotspot >= 0.35;
        const rightReserve = (vn.canExpand ? 32 : 12) + (hasHot ? 26 : 0);
        const maxC = Math.max(8, Math.floor((b.w - 16 - rightReserve) / 7.8));
        const raw = vn.label || vn.id;
        const lbl = raw.length > maxC ? '…' + raw.slice(-(maxC - 1)) : raw;

        ctx.font = `${isSel ? '700 ' : '600 '}13px -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif`;
        ctx.fillStyle = t.text;
        ctx.fillText(lbl, b.x + 16, b.y + 19);

        // Crisp TDesign mini risk tag (no emojis!)
        if (hasHot) {
          const hx = b.x + b.w - (vn.canExpand ? 50 : 28);
          drawRoundedRect(ctx, hx, b.y + 10, 20, 18, 4);
          ctx.fillStyle = t.redBg;
          ctx.fill();
          ctx.fillStyle = t.red;
          ctx.font = '700 10.5px -apple-system, BlinkMacSystemFont, sans-serif';
          ctx.textAlign = 'center';
          ctx.fillText('H', hx + 10, b.y + 19.5);
          ctx.textAlign = 'left';
        }

        // Second row: package dot + compact I / D metrics strictly inside box
        const m = vn.metrics || {};
        let curX = b.x + 16;
        if (!appState.groupByPackage && vn.pkgName && vn.pkgName !== vn.label) {
          const pCol = pkgColor(vn.pkgName);
          ctx.beginPath();
          ctx.arc(curX + 3.5, b.y + 39, 3.2, 0, Math.PI * 2);
          ctx.fillStyle = pCol;
          ctx.fill();
          curX += 11;
        }
        const meta = [];
        if (m.instability != null) meta.push(`I:${m.instability.toFixed(2)}`);
        if (m.distance != null) meta.push(`D:${m.distance.toFixed(2)}`);
        const baseText = meta.join(' · ');
        const filesSuffix = d.files && d.files.length > 1 ? ` · ${d.files.length}ф` : '';
        const availChars = Math.floor((b.x + b.w - 12 - curX) / 6.9);
        const metaStr = (baseText + filesSuffix).length <= availChars ? baseText + filesSuffix : baseText;
        ctx.font = '500 11px ui-monospace, SFMono-Regular, Menlo, monospace';
        ctx.fillStyle = t.muted;
        ctx.fillText(metaStr, curX, b.y + 39);

        // Expand (+) chip
        if (vn.canExpand) {
          drawRoundedRect(ctx, b.x + b.w - 26, b.y + 10, 18, 18, 4);
          ctx.fillStyle = t.subtle;
          ctx.fill();
          ctx.fillStyle = t.text;
          ctx.font = '600 13px -apple-system, BlinkMacSystemFont, sans-serif';
          ctx.textAlign = 'center';
          ctx.fillText('+', b.x + b.w - 17, b.y + 19.5);
        }
      }
    }

    ctx.restore();
    ctx.globalAlpha = 1;
    drawMinimap(ctx, width, height, t);
  }

  function drawMinimap(ctx, cw, ch, t) {
    if (g.boxMap.size < 6) return;
    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    for (const b of g.boxMap.values()) {
      minX = Math.min(minX, b.x);
      maxX = Math.max(maxX, b.x + b.w);
      minY = Math.min(minY, b.y);
      maxY = Math.max(maxY, b.y + b.h);
    }
    const mw = 152, mh = 94;
    const mx = cw - mw - 12, my = ch - mh - 12;
    const bw = Math.max(100, maxX - minX);
    const bh = Math.max(100, maxY - minY);
    const s = Math.min((mw - 12) / bw, (mh - 12) / bh);
    const ox = mx + 6 - minX * s;
    const oy = my + 6 - minY * s;

    ctx.save();
    ctx.globalAlpha = 0.92;
    drawRoundedRect(ctx, mx, my, mw, mh, 7);
    ctx.fillStyle = t.surface;
    ctx.fill();
    ctx.lineWidth = 1;
    ctx.strokeStyle = t.border;
    ctx.stroke();

    ctx.fillStyle = t.slate;
    ctx.globalAlpha = 0.6;
    for (const b of g.boxMap.values()) {
      if (b.isContainer) continue;
      ctx.fillRect(ox + b.x * s, oy + b.y * s, Math.max(3, b.w * s), Math.max(2, b.h * s));
    }

    const vx = -g.cam.x / g.cam.scale;
    const vy = -g.cam.y / g.cam.scale;
    const vw = cw / g.cam.scale;
    const vh = ch / g.cam.scale;
    ctx.globalAlpha = 1;
    ctx.strokeStyle = t.accent;
    ctx.lineWidth = 1.4;
    ctx.strokeRect(
      Math.max(mx, ox + vx * s),
      Math.max(my, oy + vy * s),
      Math.min(mw, vw * s),
      Math.min(mh, vh * s)
    );
    ctx.restore();
  }

  function toWorld(evt) {
    return {
      x: (evt.clientX - g.rect.left - g.cam.x) / g.cam.scale,
      y: (evt.clientY - g.rect.top - g.cam.y) / g.cam.scale,
    };
  }

  function hitNode(wx, wy) {
    const leaves = g.nodes;
    for (let i = leaves.length - 1; i >= 0; i--) {
      const vn = leaves[i];
      if (vn.isExpanded) continue;
      const b = g.boxMap.get(vn.id);
      if (!b) continue;
      if (wx >= b.x && wx <= b.x + b.w && wy >= b.y && wy <= b.y + b.h) {
        const hitToggle = vn.canExpand && wx >= b.x + b.w - 30 && wy <= b.y + 32;
        return { vn, b, hitToggle };
      }
    }
    const containers = g.nodes.filter(n => n.isExpanded && g.boxMap.has(n.id)).sort((a, b) => b.depth - a.depth);
    for (const vn of containers) {
      const b = g.boxMap.get(vn.id);
      if (wx >= b.x && wx <= b.x + b.w && wy >= b.y && wy <= b.y + ArchLayout.HEADER_H) {
        return { vn, b, hitToggle: wx <= b.x + 32 };
      }
    }
    return null;
  }

  function hitEdge(wx, wy) {
    const thresh = Math.max(7, 8 / g.cam.scale);
    let best = null;
    let bestD = thresh;
    for (const eg of g.edgeGeoms) {
      // Fast AABB rejection (eliminates 95%+ of edges instantly)
      if (wx < eg.minX - thresh || wx > eg.maxX + thresh || wy < eg.minY - thresh || wy > eg.maxY + thresh) {
        continue;
      }
      for (let i = 0; i <= 14; i++) {
        const t = i / 14;
        const omt = 1 - t;
        const omt2 = omt * omt;
        const t2 = t * t;
        const x = omt2 * omt * eg.x1 + 3 * omt2 * t * eg.cx1 + 3 * omt * t2 * eg.cx2 + t2 * t * eg.x2;
        const y = omt2 * omt * eg.y1 + 3 * omt2 * t * eg.cy1 + 3 * omt * t2 * eg.cy2 + t2 * t * eg.y2;
        const d = Math.hypot(wx - x, wy - y);
        if (d < bestD) {
          bestD = d;
          best = eg.edge;
        }
      }
    }
    return best;
  }

  function shiftSubtree(rootId, dx, dy) {
    const b = g.boxMap.get(rootId);
    if (!b) return;
    b.x += dx; b.y += dy; b.cx += dx; b.cy += dy;
    if (!b.isContainer) g.manualPos.set(rootId, { x: b.x, y: b.y });
    for (const vn of g.nodes) {
      if (vn.parentId === rootId) shiftSubtree(vn.id, dx, dy);
    }
  }

  function wireInteractions(callbacks) {
    const canvas = document.getElementById('graph-canvas');
    if (!canvas) return;
    readTheme();
    updateCanvasRect();

    if (typeof ResizeObserver !== 'undefined') {
      new ResizeObserver(() => {
        updateCanvasRect();
        if (callbacks.isActive()) scheduleDraw(callbacks.getState());
      }).observe(canvas);
    }
    window.addEventListener('scroll', updateCanvasRect, { passive: true });
    window.addEventListener('resize', updateCanvasRect, { passive: true });

    let moved = false;
    let hoverTimer = null;

    canvas.addEventListener('mousedown', evt => {
      updateCanvasRect();
      moved = false;
      const w = toWorld(evt);
      const hit = hitNode(w.x, w.y);
      if (hit) {
        g.draggingId = hit.vn.id;
        g.dragLastWorld = w;
      } else {
        g.panning = true;
        g.panStart = { x: evt.clientX - g.cam.x, y: evt.clientY - g.cam.y };
      }
    });

    window.addEventListener('mousemove', evt => {
      if (!callbacks.isActive()) return;
      if (g.draggingId) {
        const w = toWorld(evt);
        const dx = w.x - g.dragLastWorld.x;
        const dy = w.y - g.dragLastWorld.y;
        if (Math.hypot(dx, dy) > 1.5) moved = true;
        g.dragLastWorld = w;
        shiftSubtree(g.draggingId, dx, dy);
        ArchLayout.wrapExpandedContainers(g.nodes, g.boxMap);
        buildEdgeGeometries(callbacks.getState());
        scheduleDraw(callbacks.getState());
      } else if (g.panning) {
        moved = true;
        g.cam.x = evt.clientX - g.panStart.x;
        g.cam.y = evt.clientY - g.panStart.y;
        scheduleDraw(callbacks.getState());
      }
    });

    canvas.addEventListener('mousemove', evt => {
      if (g.draggingId || g.panning) return;
      const w = toWorld(evt);
      const nhit = hitNode(w.x, w.y);
      const ehit = nhit ? null : hitEdge(w.x, w.y);
      const nextNode = nhit ? nhit.vn.id : null;
      const nextEdge = ehit ? ehit.id : null;

      if (nextNode !== g.hoverNodeId || nextEdge !== g.hoverEdgeId) {
        g.hoverNodeId = nextNode;
        g.hoverEdgeId = nextEdge;
        canvas.style.cursor = nhit || ehit ? 'pointer' : 'grab';
        scheduleDraw(callbacks.getState());
      }

      if (nhit) {
        const vn = nhit.vn;
        const m = vn.metrics || {};
        const d = vn.data || {};
        const tipExtras = [];
        if (vn.pkgName && vn.pkgName !== vn.label) tipExtras.push(`Пакет: <b>${ArchModel.esc(vn.pkgName)}</b>`);
        if (d.hotspot >= 0.35) tipExtras.push(`Горячая точка (H): <b>${ArchModel.fmt(d.hotspot)}</b>`);
        if (nhit.hitToggle) {
          tipExtras.push(vn.isExpanded ? 'Нажмите «▾», чтобы свернуть содержимое' : 'Нажмите «+», чтобы раскрыть файлы/символы внутри');
        }
        callbacks.showTip(
          evt,
          `<div class="mono" style="font-weight:700">${ArchModel.esc(vn.id.replace(/^(pkg|mod|file|sym):/, ''))}</div>` +
          `<div>Входящих: <b>${m.ca ?? '—'}</b> · Исходящих: <b>${m.ce ?? '—'}</b> · Файлов: ${d.files?.length || 0} · Строк: ${(d.loc || 0).toLocaleString()}</div>` +
          `<div>I (нестаб.): <b>${ArchModel.fmt(m.instability)}</b> · A (абстр.): <b>${ArchModel.fmt(m.abstractness)}</b> · D (откл.): <b>${ArchModel.fmt(m.distance)}</b></div>` +
          (tipExtras.length ? `<div style="margin-top:4px;color:var(--td-text-color-secondary)">${tipExtras.join(' · ')}</div>` : '')
        );
      } else if (ehit) {
        const syms = [...new Set((ehit.fileEdges || []).flatMap(fe => fe.symbols || []))].slice(0, 6);
        callbacks.showTip(
          evt,
          `<div class="mono" style="font-weight:700">${ArchModel.esc(ehit.from.replace(/^(pkg|mod|file|sym):/, ''))} &rarr; ${ArchModel.esc(ehit.to.replace(/^(pkg|mod|file|sym):/, ''))}</div>` +
          (ehit.isTemporal
            ? `<div><span class="badge b-useless">Совместные коммиты (Git)</span> ${ehit.weight} комм. (${Math.round((ehit.temporalStrength || 0) * 100)}%)</div>`
            : `<div><b>${ehit.weight}</b> связей файлов · DIP: ${ehit.dipWeight} ${ehit.isCycleEdge ? '<span class="badge b-pain">Цикл</span>' : ''} ${ehit.isSdpViolation ? '<span class="badge b-calm">SDP</span>' : ''} ${ehit.isRuleViolation ? '<span class="badge b-rule">Правило</span>' : ''}</div>` +
              (syms.length ? `<div class="mono" style="margin-top:4px;color:var(--td-text-color-secondary)">Символы: ${ArchModel.esc(syms.join(', '))}</div>` : ''))
        );
      } else {
        clearTimeout(hoverTimer);
        callbacks.hideTip();
      }
    });

    canvas.addEventListener('mouseleave', () => {
      g.hoverNodeId = null;
      g.hoverEdgeId = null;
      callbacks.hideTip();
      scheduleDraw(callbacks.getState());
    });

    window.addEventListener('mouseup', evt => {
      if (g.draggingId && !moved) {
        const w = toWorld(evt);
        const hit = hitNode(w.x, w.y);
        if (hit) {
          if (hit.hitToggle) callbacks.onToggleExpand(hit.vn.id);
          else if (evt.shiftKey) callbacks.onShiftSelectNode(hit.vn.id);
          else callbacks.onSelectNode(hit.vn.id);
        }
      } else if (g.panning && !moved) {
        const w = toWorld(evt);
        const ehit = hitEdge(w.x, w.y);
        if (ehit) callbacks.onSelectEdge(ehit);
        else callbacks.onClearSelection();
      }
      g.draggingId = null;
      g.panning = false;
    });

    canvas.addEventListener('dblclick', evt => {
      const w = toWorld(evt);
      const hit = hitNode(w.x, w.y);
      if (hit && hit.vn.canExpand) callbacks.onToggleExpand(hit.vn.id);
    });

    canvas.addEventListener(
      'wheel',
      evt => {
        evt.preventDefault();
        const mx = evt.clientX - g.rect.left;
        const my = evt.clientY - g.rect.top;
        const factor = evt.deltaY < 0 ? 1.12 : 0.89;
        const nextScale = Math.max(0.18, Math.min(3.2, g.cam.scale * factor));
        g.cam.x = mx - ((mx - g.cam.x) / g.cam.scale) * nextScale;
        g.cam.y = my - ((my - g.cam.y) / g.cam.scale) * nextScale;
        g.cam.scale = nextScale;
        scheduleDraw(callbacks.getState());
      },
      { passive: false }
    );
  }

  return {
    rebuild,
    draw: scheduleDraw,
    fitView,
    zoomBy,
    refreshTheme,
    clearManualPositions,
    wireInteractions,
    getEdges: () => g.edges,
    getNodes: () => g.nodes,
  };
})();
