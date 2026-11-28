// Recursive Compound Sugiyama Layered Layout + Ego Blast-Radius Layout + Cluster Force Layout.

const ArchLayout = (() => {
  const CARD_H = 56;
  const SYM_H = 34;
  const HEADER_H = 36;
  const PAD_X = 24;
  const PAD_Y = 20;
  const GAP_X = 34;
  const GAP_Y = 82;
  const GAP_LR_X = 110;
  const GAP_LR_Y = 24;

  function measureLeaf(vn) {
    const isSym = vn.kind === 'symbol';
    const labelLen = Math.min(24, (vn.label || vn.id).length);
    const w = isSym
      ? Math.max(120, Math.min(210, 32 + labelLen * 7.8))
      : Math.max(196, Math.min(256, 56 + labelLen * 8.2));
    const h = isSym ? SYM_H : CARD_H;
    return { w, h };
  }

  // Sugiyama layered layout on a flat list of sibling boxes + internal edges between them
  function layoutSiblingsSugiyama(items, edges, orientation = 'TB') {
    if (items.length === 0) return { w: 120, h: 80, layerBands: [] };
    const idSet = new Set(items.map(it => it.id));
    const itemMap = new Map(items.map(it => [it.id, it]));

    const out = new Map(items.map(it => [it.id, new Set()]));
    const inc = new Map(items.map(it => [it.id, new Set()]));
    for (const e of edges) {
      if (idSet.has(e.from) && idSet.has(e.to) && e.from !== e.to && !e.isTemporal && !e.isAllCut) {
        out.get(e.from).add(e.to);
        inc.get(e.to).add(e.from);
      }
    }

    // 1. Cycle-breaking for rank assignment: keep high-instability / caller nodes above stable core
    const dagOut = new Map(items.map(it => [it.id, []]));
    const indeg = new Map(items.map(it => [it.id, 0]));
    const instScore = id => {
      const m = itemMap.get(id).vn.metrics;
      if (m && m.instability != null) return m.instability;
      const o = out.get(id).size;
      const i = inc.get(id).size;
      return o + i === 0 ? 0.5 : o / (o + i);
    };

    const order = [...items].sort((a, b) => {
      const diff = instScore(b.id) - instScore(a.id);
      if (Math.abs(diff) > 0.001) return diff;
      return out.get(b.id).size - out.get(a.id).size || a.id.localeCompare(b.id);
    });
    const posRank = new Map(order.map((it, idx) => [it.id, idx]));

    const flatEdges = [];
    for (const [u, nbrs] of out.entries()) {
      for (const v of nbrs) flatEdges.push({ from: u, to: v });
    }
    const { nodeScc, inCycle } = ArchModel.computeTarjanScc([...idSet], flatEdges);

    for (const e of flatEdges) {
      const sameCycle = inCycle.has(e.from) && nodeScc.get(e.from) === nodeScc.get(e.to);
      if (sameCycle && posRank.get(e.from) > posRank.get(e.to)) {
        dagOut.get(e.to).push(e.from);
        indeg.set(e.from, indeg.get(e.from) + 1);
      } else {
        dagOut.get(e.from).push(e.to);
        indeg.set(e.to, indeg.get(e.to) + 1);
      }
    }

    // 2. Longest-path rank assignment (top = callers, bottom = foundational core)
    const rank = new Map(items.map(it => [it.id, 0]));
    const q = [];
    for (const [id, d] of indeg.entries()) {
      if (d === 0) q.push(id);
    }
    const visited = new Set();
    while (q.length > 0) {
      const u = q.shift();
      visited.add(u);
      const ru = rank.get(u);
      for (const v of dagOut.get(u) || []) {
        rank.set(v, Math.max(rank.get(v), ru + 1));
        indeg.set(v, indeg.get(v) - 1);
        if (indeg.get(v) === 0) q.push(v);
      }
    }
    for (const it of items) {
      if (!visited.has(it.id)) rank.set(it.id, 0);
    }

    // Pull sink/core nodes (0 outgoing, >0 incoming) down to the bottom layer so stable cores align cleanly
    let maxConnectedRank = 0;
    let hasConnected = false;
    for (const it of items) {
      const deg = out.get(it.id).size + inc.get(it.id).size;
      if (deg > 0) {
        hasConnected = true;
        maxConnectedRank = Math.max(maxConnectedRank, rank.get(it.id));
      }
    }
    if (hasConnected && maxConnectedRank > 0) {
      for (const it of items) {
        const o = out.get(it.id).size;
        const i = inc.get(it.id).size;
        if (o === 0 && i > 0) rank.set(it.id, maxConnectedRank);
        else if (o === 0 && i === 0) rank.set(it.id, maxConnectedRank + 1);
      }
    }

    const layersMap = new Map();
    for (const it of items) {
      const r = rank.get(it.id);
      if (!layersMap.has(r)) layersMap.set(r, []);
      layersMap.get(r).push(it);
    }
    const layerKeys = [...layersMap.keys()].sort((a, b) => a - b);
    const layers = layerKeys.map(k => layersMap.get(k));

    // Wrap huge flat layers neatly
    const MAX_PER_ROW = orientation === 'LR' ? 8 : 6;
    const expandedLayers = [];
    for (const layer of layers) {
      layer.sort(
        (a, b) =>
          (a.vn.pkgName || '').localeCompare(b.vn.pkgName || '') ||
          (out.get(b.id).size + inc.get(b.id).size) - (out.get(a.id).size + inc.get(a.id).size) ||
          a.id.localeCompare(b.id)
      );
      if (layer.length <= MAX_PER_ROW) {
        expandedLayers.push(layer);
      } else {
        for (let i = 0; i < layer.length; i += MAX_PER_ROW) {
          expandedLayers.push(layer.slice(i, i + MAX_PER_ROW));
        }
      }
    }

    // 3. 6-sweep Barycenter crossing minimization (uses full graph neighbors across layers)
    const layerOf = new Map();
    const posInLayer = () => {
      const map = new Map();
      expandedLayers.forEach((l, li) => {
        l.forEach((it, idx) => {
          map.set(it.id, idx);
          layerOf.set(it.id, li);
        });
      });
      return map;
    };

    for (let sweep = 0; sweep < 6; sweep++) {
      const down = sweep % 2 === 0;
      const start = down ? 1 : expandedLayers.length - 2;
      const end = down ? expandedLayers.length : -1;
      const step = down ? 1 : -1;
      for (let li = start; li !== end; li += step) {
        const pos = posInLayer();
        const bary = new Map();
        expandedLayers[li].forEach((it, idx) => {
          const neighbors = down ? [...inc.get(it.id)] : [...out.get(it.id)];
          const valid = neighbors.filter(nid => {
            const nl = layerOf.get(nid);
            return nl != null && (down ? nl < li : nl > li);
          });
          if (valid.length === 0) bary.set(it.id, idx + 0.01);
          else bary.set(it.id, valid.reduce((s, nid) => s + pos.get(nid), 0) / valid.length);
        });
        expandedLayers[li].sort((a, b) => bary.get(a.id) - bary.get(b.id) || a.id.localeCompare(b.id));
      }
    }

    // 4. Coordinate assignment + layer band metadata
    let totalW = 0;
    let totalH = 0;
    const layerBands = [];

    if (orientation === 'TB') {
      const rowWidths = expandedLayers.map(
        l => l.reduce((s, it) => s + it.w, 0) + Math.max(0, l.length - 1) * GAP_X
      );
      totalW = Math.max(200, ...rowWidths);
      let curY = 0;
      expandedLayers.forEach((l, li) => {
        const rowH = Math.max(...l.map(it => it.h));
        let curX = (totalW - rowWidths[li]) / 2;
        for (const it of l) {
          it.lx = curX;
          it.ly = curY + (rowH - it.h) / 2;
          it.layerIndex = li;
          curX += it.w + GAP_X;
        }
        layerBands.push({ index: li, y: curY, h: rowH });
        curY += rowH + GAP_Y;
      });
      totalH = Math.max(80, curY - GAP_Y);
    } else {
      const colHeights = expandedLayers.map(
        l => l.reduce((s, it) => s + it.h, 0) + Math.max(0, l.length - 1) * GAP_LR_Y
      );
      totalH = Math.max(160, ...colHeights);
      let curX = 0;
      expandedLayers.forEach((l, li) => {
        const colW = Math.max(...l.map(it => it.w));
        let curY = (totalH - colHeights[li]) / 2;
        for (const it of l) {
          it.lx = curX + (colW - it.w) / 2;
          it.ly = curY;
          it.layerIndex = li;
          curY += it.h + GAP_LR_Y;
        }
        layerBands.push({ index: li, x: curX, w: colW });
        curX += colW + GAP_LR_X;
      });
      totalW = Math.max(200, curX - GAP_LR_X);
    }

    return { w: totalW, h: totalH, layerBands };
  }

  function liftEdgesToContainer(visibleNodes, edges, parentId) {
    const parentOf = new Map(visibleNodes.map(n => [n.id, n.parentId]));
    const lift = id => {
      let cur = id;
      while (cur) {
        const p = parentOf.get(cur) || null;
        if (p === parentId) return cur;
        cur = p;
      }
      return null;
    };
    const lifted = [];
    const seen = new Set();
    for (const e of edges) {
      if (e.isTemporal || e.isAllCut) continue;
      const a = lift(e.from);
      const b = lift(e.to);
      if (a && b && a !== b) {
        const k = ArchModel.edgeKey(a, b);
        if (!seen.has(k)) {
          seen.add(k);
          lifted.push({ from: a, to: b });
        }
      }
    }
    return lifted;
  }

  function computeCompoundSugiyama(visibleNodes, edges, orientation = 'TB') {
    const childrenByParent = new Map();
    for (const vn of visibleNodes) {
      const key = vn.parentId || '__ROOT__';
      if (!childrenByParent.has(key)) childrenByParent.set(key, []);
      childrenByParent.get(key).push(vn);
    }

    const boxMap = new Map();

    function layoutSubtree(parentId) {
      const kids = childrenByParent.get(parentId || '__ROOT__') || [];
      const items = kids.map(vn => {
        if (vn.isExpanded && (childrenByParent.get(vn.id) || []).length > 0) {
          const inner = layoutSubtree(vn.id);
          const minTitleW = Math.max(210, 56 + (vn.label || vn.id).length * 8.4);
          return {
            id: vn.id,
            vn,
            isContainer: true,
            w: Math.max(minTitleW, inner.w + PAD_X * 2),
            h: inner.h + HEADER_H + PAD_Y * 2,
            lx: 0,
            ly: 0,
            layerIndex: 0,
            childItems: inner.items,
          };
        }
        const sz = measureLeaf(vn);
        return {
          id: vn.id,
          vn,
          isContainer: false,
          w: sz.w,
          h: sz.h,
          lx: 0,
          ly: 0,
          layerIndex: 0,
          childItems: [],
        };
      });

      const liftedEdges = liftEdgesToContainer(visibleNodes, edges, parentId);
      const dims = layoutSiblingsSugiyama(items, liftedEdges, orientation);
      return { w: dims.w, h: dims.h, items, layerBands: dims.layerBands };
    }

    const rootTree = layoutSubtree(null);

    function commitCoords(items, offsetX, offsetY) {
      for (const it of items) {
        const x = offsetX + it.lx;
        const y = offsetY + it.ly;
        boxMap.set(it.id, {
          id: it.id,
          x,
          y,
          w: it.w,
          h: it.h,
          cx: x + it.w / 2,
          cy: y + it.h / 2,
          isContainer: it.isContainer,
          layerIndex: it.layerIndex,
        });
        if (it.isContainer && it.childItems.length > 0) {
          commitCoords(it.childItems, x + PAD_X, y + HEADER_H + PAD_Y);
        }
      }
    }

    commitCoords(rootTree.items, 48, 48);
    return boxMap;
  }

  function computeEgoLayout(visibleNodes, edges, focusId) {
    const leaves = visibleNodes.filter(n => !n.isExpanded);
    if (leaves.length === 0) return new Map();

    let activeFocus = focusId && leaves.some(n => n.id === focusId) ? focusId : null;
    if (!activeFocus) {
      const top = [...leaves].sort(
        (a, b) =>
          (b.metrics?.ca || 0) + (b.metrics?.ce || 0) - ((a.metrics?.ca || 0) + (a.metrics?.ce || 0))
      )[0];
      activeFocus = top ? top.id : leaves[0].id;
    }

    const blast = ArchModel.computeBlastRadius(edges, activeFocus, Infinity, 'both');
    const cols = new Map();
    const unreached = [];

    for (const vn of leaves) {
      const info = blast.get(vn.id);
      if (!info) {
        unreached.push(vn);
        continue;
      }
      const col =
        info.dir === 'self'
          ? 0
          : info.dir === 'upstream'
          ? -Math.min(4, info.hop)
          : Math.min(4, info.hop);
      if (!cols.has(col)) cols.set(col, []);
      cols.get(col).push(vn);
    }

    const boxMap = new Map();
    const sortedCols = [...cols.keys()].sort((a, b) => a - b);
    let curX = 60;
    let maxH = 440;

    const colDims = sortedCols.map(c => {
      const list = cols.get(c);
      list.sort(
        (a, b) =>
          (b.metrics?.ca || 0) + (b.metrics?.ce || 0) - ((a.metrics?.ca || 0) + (a.metrics?.ce || 0))
      );
      const sizes = list.map(vn => measureLeaf(vn));
      const colW = Math.max(176, ...sizes.map(s => s.w));
      const colH = sizes.reduce((s, sz) => s + sz.h, 0) + Math.max(0, list.length - 1) * 24;
      maxH = Math.max(maxH, colH + 80);
      return { c, list, sizes, colW, colH };
    });

    for (const cd of colDims) {
      let curY = (maxH - cd.colH) / 2;
      cd.list.forEach((vn, idx) => {
        const sz = cd.sizes[idx];
        const x = curX + (cd.colW - sz.w) / 2;
        boxMap.set(vn.id, {
          id: vn.id,
          x,
          y: curY,
          w: sz.w,
          h: sz.h,
          cx: x + sz.w / 2,
          cy: curY + sz.h / 2,
          isContainer: false,
          layerIndex: cd.c,
        });
        curY += sz.h + 24;
      });
      curX += cd.colW + 115;
    }

    if (unreached.length > 0) {
      let ux = 60;
      let uy = maxH + 46;
      for (const vn of unreached) {
        const sz = measureLeaf(vn);
        if (ux + sz.w > Math.max(980, curX)) {
          ux = 60;
          uy += 72;
        }
        boxMap.set(vn.id, {
          id: vn.id,
          x: ux,
          y: uy,
          w: sz.w,
          h: sz.h,
          cx: ux + sz.w / 2,
          cy: uy + sz.h / 2,
          isContainer: false,
          layerIndex: 99,
        });
        ux += sz.w + 22;
      }
    }

    wrapExpandedContainers(visibleNodes, boxMap);
    return boxMap;
  }

  function computeClusterForce(visibleNodes, edges) {
    const leaves = visibleNodes.filter(n => !n.isExpanded);
    const n = leaves.length;
    const boxMap = new Map();
    if (n === 0) return boxMap;

    const items = leaves.map((vn, i) => {
      const sz = measureLeaf(vn);
      const angle = (2 * Math.PI * i) / Math.max(1, n);
      const r = Math.max(210, Math.sqrt(n) * 115);
      return {
        id: vn.id,
        vn,
        w: sz.w,
        h: sz.h,
        x: 560 + Math.cos(angle) * r,
        y: 380 + Math.sin(angle) * r,
        vx: 0,
        vy: 0,
      };
    });
    const byId = new Map(items.map(it => [it.id, it]));
    const k = Math.max(185, Math.sqrt(n) * 52);
    const iters = n > 120 ? 80 : 130;

    for (let step = 0; step < iters; step++) {
      const temp = (1 - step / iters) * 40 + 1;
      for (const it of items) { it.vx = 0; it.vy = 0; }

      for (let i = 0; i < n; i++) {
        for (let j = i + 1; j < n; j++) {
          const a = items[i], b = items[j];
          const dx = a.x - b.x || 0.1;
          const dy = a.y - b.y || 0.1;
          const dist = Math.hypot(dx, dy) || 0.1;
          const minSep = (a.w + b.w) * 0.62 + 24;
          let rep = (k * k) / dist;
          if (dist < minSep) rep *= 2.8;
          const fx = (dx / dist) * rep;
          const fy = (dy / dist) * rep;
          a.vx += fx; a.vy += fy;
          b.vx -= fx; b.vy -= fy;

          if (a.vn.pkgName && a.vn.pkgName === b.vn.pkgName) {
            const pull = dist * 0.035;
            a.vx -= (dx / dist) * pull;
            a.vy -= (dy / dist) * pull;
            b.vx += (dx / dist) * pull;
            b.vy += (dy / dist) * pull;
          }
        }
      }

      for (const e of edges) {
        if (e.isAllCut) continue;
        const a = byId.get(e.from);
        const b = byId.get(e.to);
        if (!a || !b) continue;
        const dx = b.x - a.x;
        const dy = b.y - a.y;
        const dist = Math.hypot(dx, dy) || 0.1;
        const attr = (dist * dist) / (k * (e.isTemporal ? 2.4 : 1.2));
        const fx = (dx / dist) * attr;
        const fy = (dy / dist) * attr;
        a.vx += fx; a.vy += fy;
        b.vx -= fx; b.vy -= fy;
      }

      for (const it of items) {
        it.vx += (560 - it.x) * 0.025;
        it.vy += (380 - it.y) * 0.025;
        const mag = Math.hypot(it.vx, it.vy) || 1;
        const move = Math.min(mag, temp);
        it.x += (it.vx / mag) * move;
        it.y += (it.vy / mag) * move;
      }
    }

    for (const it of items) {
      boxMap.set(it.id, {
        id: it.id,
        x: it.x - it.w / 2,
        y: it.y - it.h / 2,
        w: it.w,
        h: it.h,
        cx: it.x,
        cy: it.y,
        isContainer: false,
        layerIndex: 0,
      });
    }

    wrapExpandedContainers(visibleNodes, boxMap);
    return boxMap;
  }

  function wrapExpandedContainers(visibleNodes, boxMap) {
    const containers = visibleNodes
      .filter(n => n.isExpanded)
      .sort((a, b) => b.depth - a.depth);
    for (const c of containers) {
      const kids = visibleNodes.filter(n => n.parentId === c.id && boxMap.has(n.id));
      if (kids.length === 0) continue;
      let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
      for (const k of kids) {
        const b = boxMap.get(k.id);
        minX = Math.min(minX, b.x);
        maxX = Math.max(maxX, b.x + b.w);
        minY = Math.min(minY, b.y);
        maxY = Math.max(maxY, b.y + b.h);
      }
      const x = minX - PAD_X;
      const y = minY - HEADER_H - PAD_Y;
      const w = Math.max(200, maxX - minX + PAD_X * 2);
      const h = maxY - minY + HEADER_H + PAD_Y * 2;
      boxMap.set(c.id, { id: c.id, x, y, w, h, cx: x + w / 2, cy: y + h / 2, isContainer: true, layerIndex: 0 });
    }
  }

  return {
    HEADER_H,
    computeCompoundSugiyama,
    computeEgoLayout,
    computeClusterForce,
    wrapExpandedContainers,
  };
})();
