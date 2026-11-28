// Core graph data engine: compound hierarchy, Tarjan SCC, What-If simulator, Blast Radius, Path Finder.

const ArchModel = (() => {
  const esc = str =>
    String(str).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

  const fmt = (v, d = 2) => (v == null || Number.isNaN(v) ? '—' : Number(v).toFixed(d));

  const edgeKey = (from, to) => JSON.stringify([from, to]);

  function ideUri(rootPath, relPath, line = 1) {
    const cleanRoot = (rootPath || '').replace(/\/+$/, '');
    const abs = cleanRoot ? `${cleanRoot}/${relPath}` : relPath;
    return `vscode://file/${encodeURI(abs)}:${line || 1}`;
  }

  function directedAdjacency(ids, edges) {
    const adj = new Map(ids.map(id => [id, []]));
    for (const e of edges) {
      if (adj.has(e.from) && adj.has(e.to) && e.from !== e.to) {
        adj.get(e.from).push(e.to);
      }
    }
    return adj;
  }

  // Tarjan SCC on directed graph (ids: string[], edges: {from, to}[])
  function computeTarjanScc(ids, edges) {
    const adj = directedAdjacency(ids, edges);
    let idx = 0;
    const indexMap = new Map();
    const lowlink = new Map();
    const onStack = new Set();
    const stack = [];
    const sccs = [];

    function strongconnect(v) {
      indexMap.set(v, idx);
      lowlink.set(v, idx);
      idx++;
      stack.push(v);
      onStack.add(v);

      for (const w of adj.get(v) || []) {
        if (!indexMap.has(w)) {
          strongconnect(w);
          lowlink.set(v, Math.min(lowlink.get(v), lowlink.get(w)));
        } else if (onStack.has(w)) {
          lowlink.set(v, Math.min(lowlink.get(v), indexMap.get(w)));
        }
      }

      if (lowlink.get(v) === indexMap.get(v)) {
        const comp = [];
        while (true) {
          const w = stack.pop();
          onStack.delete(w);
          comp.push(w);
          if (w === v) break;
        }
        sccs.push(comp);
      }
    }

    for (const id of ids) {
      if (!indexMap.has(id)) strongconnect(id);
    }

    const nodeScc = new Map();
    const inCycle = new Set();
    const cycles = [];
    sccs.forEach((comp, cid) => {
      for (const id of comp) nodeScc.set(id, cid);
      if (comp.length >= 2) {
        cycles.push([...comp].sort());
        for (const id of comp) inCycle.add(id);
      }
    });
    return { sccs, nodeScc, inCycle, cycles };
  }

  function computePropagationCost(ids, edges) {
    const n = ids.length;
    if (n === 0) return 0;
    const adj = directedAdjacency(ids, edges);
    let ccd = 0;
    for (const start of ids) {
      const seen = new Set([start]);
      const q = [start];
      for (let index = 0; index < q.length; index++) {
        const cur = q[index];
        for (const nxt of adj.get(cur) || []) {
          if (!seen.has(nxt)) {
            seen.add(nxt);
            q.push(nxt);
          }
        }
      }
      ccd += seen.size;
    }
    return ccd / (n * n);
  }

  const HOTSPOT_THRESHOLD = 0.30;
  const mainSequenceBand = report => report?.main_sequence_band ?? 0.30;
  const sdpTolerance = report => report?.sdp_tolerance ?? 0.05;
  const computeInstability = (ca, ce) => ca + ce === 0 ? null : ce / (ca + ce);

  function nodeMetrics(node, ca, ce, band, inCycle = false) {
    const instability = computeInstability(ca, ce);
    const totalSyms = node.abstractCount + node.concreteCount;
    const abstractness = totalSyms === 0 ? null : node.abstractCount / totalSyms;
    const distance =
      instability != null && abstractness != null
        ? Math.abs(abstractness + instability - 1.0)
        : null;

    let zone = 'isolated';
    if (instability != null) {
      if (abstractness == null) zone = 'no_entities';
      else if (distance <= band) zone = 'main_sequence';
      else if (abstractness + instability < 1.0) zone = 'zone_of_pain';
      else zone = 'zone_of_uselessness';
    }
    const activePain = zone === 'zone_of_pain' && node.hotspot >= HOTSPOT_THRESHOLD;
    return { ca, ce, instability, abstractness, distance, zone, activePain, inCycle };
  }

  // Build hierarchical tree: package -> module -> file -> symbol
  function buildHierarchy(report, whatIf) {
    const fg = report.file_graph || { files: [], edges: [] };
    const ruleViolPairs = new Set(
      (report.rule_violations || []).map(rv => edgeKey(rv.from_file, rv.to_file))
    );

    const movedFiles = (whatIf && whatIf.movedFiles) || new Map();
    const cutEdges = (whatIf && whatIf.cutEdges) || new Set();

    const files = fg.files || [];
    const fileMap = new Map();
    const distinctPkgs = new Set(files.map(f => f.package || '<root>'));
    const multiPkg = distinctPkgs.size > 1;

    const nodes = new Map();
    function ensureNode(id, label, kind, parentId, pkgName = '') {
      if (!nodes.has(id)) {
        nodes.set(id, {
          id,
          label,
          kind, // 'package' | 'module' | 'file' | 'symbol'
          pkgName,
          parentId: parentId || null,
          children: [],
          files: [],
          loc: 0,
          commits: 0,
          churn: 0,
          hotspot: 0,
          abstractCount: 0,
          concreteCount: 0,
          language: '',
          languages: {},
          louvainCluster: 0,
          declarations: [],
          fileNode: null,
          symbolDecl: null,
        });
        if (parentId && nodes.has(parentId)) {
          const p = nodes.get(parentId);
          if (!p.children.includes(id)) p.children.push(id);
        }
      }
      return nodes.get(id);
    }

    const roots = [];
    const moduleRoots = [];
    const moduleLookup = new Map();
    const modView = (report.views || []).find(v => v.id === 'modules');
    const modClusterMap = new Map();
    if (modView) {
      for (const mn of modView.nodes) {
        for (const fp of mn.files || []) modClusterMap.set(fp, mn.louvain_cluster || 0);
      }
    }

    for (const f of files) {
      const effectiveMod = movedFiles.get(f.path) || f.module_id || f.package || '<root>';
      const pkgName = f.package || effectiveMod.split('/')[0] || '<root>';
      fileMap.set(f.path, { ...f, effectiveMod, pkgName });

      let parentForMod = null;
      if (multiPkg && effectiveMod !== pkgName) {
        const pkgId = `pkg:${pkgName}`;
        ensureNode(pkgId, pkgName, 'package', null, pkgName);
        if (!roots.includes(pkgId)) roots.push(pkgId);
        parentForMod = pkgId;
      }

      const modId = `mod:${effectiveMod}`;
      const modLabel =
        parentForMod && effectiveMod.startsWith(pkgName + '/')
          ? effectiveMod.slice(pkgName.length + 1)
          : effectiveMod;
      ensureNode(modId, modLabel, 'module', parentForMod, pkgName);
      if (!parentForMod && !roots.includes(modId)) roots.push(modId);
      if (!moduleRoots.includes(modId)) moduleRoots.push(modId);
      moduleLookup.set(effectiveMod, modId);

      const fileId = `file:${f.path}`;
      const shortFile = f.path.split('/').slice(-2).join('/');
      const fn = ensureNode(fileId, shortFile, 'file', modId, pkgName);
      fn.fileNode = f;
      fn.declarations = f.declarations || [];
      fn.language = f.language || '';
      fn.louvainCluster = modClusterMap.get(f.path) || 0;

      // Add top-level exported (or all if none exported) symbols under file for symbol-level drilldown
      const decls = f.declarations || [];
      const hasPub = decls.some(d => d.visibility !== 'private');
      const visibleDecls = decls.filter(d => !hasPub || d.visibility !== 'private').slice(0, 28);
      for (const d of visibleDecls) {
        const symId = `sym:${f.path}#${d.name}`;
        const sn = ensureNode(symId, d.name, 'symbol', fileId, pkgName);
        sn.symbolDecl = d;
        sn.loc = 1;
        sn.language = f.language || '';
        sn.abstractCount = d.is_abstract ? 1 : 0;
        sn.concreteCount = d.is_abstract ? 0 : 1;
      }

      // Propagate stats up to file -> module -> package
      const commits = f.git ? f.git.commits : 0;
      const churn = f.git ? f.git.lines_added + f.git.lines_deleted : 0;
      const hotspot = f.git ? f.git.hotspot_score : 0;
      const absC = (f.symbols && f.symbols.abstract_declarations) || 0;
      const conC = (f.symbols && f.symbols.concrete_declarations) || 0;

      let curId = fileId;
      while (curId) {
        const cur = nodes.get(curId);
        cur.files.push(f.path);
        cur.loc += f.loc || 0;
        cur.commits += commits;
        cur.churn += churn;
        cur.hotspot = Math.max(cur.hotspot, hotspot);
        cur.abstractCount += absC;
        cur.concreteCount += conC;
        cur.languages[f.language] = (cur.languages[f.language] || 0) + 1;
        curId = cur.parentId;
      }
    }

    for (const node of nodes.values()) {
      const langs = Object.entries(node.languages).sort((a, b) => b[1] - a[1]);
      if (langs.length > 0 && !node.language) node.language = langs[0][0];
      node.children.sort();
    }
    roots.sort();
    moduleRoots.sort();

    // Active file edges (with What-If cut flags)
    const fileEdges = (fg.edges || []).map(e => {
      const key = edgeKey(e.from, e.to);
      return {
        ...e,
        key,
        isCut: cutEdges.has(key),
        isRuleViolation: ruleViolPairs.has(key),
      };
    });

    // Compute live module-level summary for What-If comparison
    const activeEdges = fileEdges.filter(e => !e.isCut);
    const modIds = [...new Set(files.map(f => fileMap.get(f.path).effectiveMod))].sort();
    const modEdgeMap = new Map();
    const caSets = new Map(modIds.map(id => [id, new Set()]));
    const ceSets = new Map(modIds.map(id => [id, new Set()]));

    for (const e of activeEdges) {
      const sf = fileMap.get(e.from);
      const tf = fileMap.get(e.to);
      if (!sf || !tf || sf.effectiveMod === tf.effectiveMod) continue;
      caSets.get(tf.effectiveMod).add(e.from);
      ceSets.get(sf.effectiveMod).add(e.to);
      const k = edgeKey(sf.effectiveMod, tf.effectiveMod);
      if (!modEdgeMap.has(k)) modEdgeMap.set(k, { from: sf.effectiveMod, to: tf.effectiveMod, weight: 0 });
      modEdgeMap.get(k).weight += 1;
    }

    const modEdgesArr = [...modEdgeMap.values()];
    const modScc = computeTarjanScc(modIds, modEdgesArr);
    const modInst = new Map();
    for (const id of modIds) {
      const ca = caSets.get(id).size;
      const ce = ceSets.get(id).size;
      modInst.set(id, computeInstability(ca, ce));
    }
    let sdpCount = 0;
    const tol = sdpTolerance(report);
    for (const e of modEdgesArr) {
      const iF = modInst.get(e.from);
      const iT = modInst.get(e.to);
      if (iF != null && iT != null && iT - iF > tol) sdpCount++;
    }
    const propCost = computePropagationCost(modIds, modEdgesArr);

    return {
      nodes,
      roots,
      moduleRoots,
      fileMap,
      fileEdges,
      moduleMetrics: {
        modIds,
        cycles: modScc.cycles,
        sdpCount,
        propCost,
      },
    };
  }

  // Project compound graph for the currently expanded container IDs
  function projectCompoundGraph(hierarchy, options) {
    const {
      expandedIds,
      groupByPackage = false,
      hideIsolated = true,
      search = '',
      lang = 'all',
      edgeFilter = 'all',
      minWeight = 1,
      showTemporal = false,
      showCutEdges = true,
      report,
    } = options;

    const q = search.trim().toLowerCase();
    const visibleNodes = [];
    const visibleLeaves = new Set();
    const expandedContainers = new Set();

    function matchesFilter(node) {
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
      if (!node || !matchesFilter(node)) return;
      const canExpand = node.children.length > 0;
      const isExpanded = canExpand && expandedIds.has(id);

      visibleNodes.push({
        id: node.id,
        label: node.label,
        kind: node.kind,
        pkgName: node.pkgName || '',
        parentId: node.parentId && expandedContainers.has(node.parentId) ? node.parentId : null,
        depth,
        canExpand,
        isExpanded,
        data: node,
      });

      if (isExpanded) {
        expandedContainers.add(id);
        for (const childId of node.children) walk(childId, depth + 1);
      } else {
        visibleLeaves.add(id);
      }
    }

    const startRoots = groupByPackage ? hierarchy.roots : hierarchy.moduleRoots;
    for (const rootId of startRoots) walk(rootId, 0);

    // Map any file (or file+symbol) to its deepest visible leaf node in the compound graph
    function resolveVisibleLeaf(filePath, symbolName) {
      if (symbolName) {
        const symId = `sym:${filePath}#${symbolName}`;
        if (visibleLeaves.has(symId)) return symId;
      }
      let cur = `file:${filePath}`;
      while (cur) {
        if (visibleLeaves.has(cur)) return cur;
        const n = hierarchy.nodes.get(cur);
        cur = n ? n.parentId : null;
      }
      return null;
    }

    const edgeAcc = new Map();
    const caSets = new Map([...visibleLeaves].map(id => [id, new Set()]));
    const ceSets = new Map([...visibleLeaves].map(id => [id, new Set()]));

    for (const fe of hierarchy.fileEdges) {
      if (fe.isCut && !showCutEdges) continue;

      const srcLeaf = resolveVisibleLeaf(fe.from, null);
      if (!srcLeaf) continue;

      // If target file is expanded into symbols and we know imported symbols, wire directly to symbols!
      const tgtFileId = `file:${fe.to}`;
      let targets = [];
      if (expandedContainers.has(tgtFileId) && fe.symbols && fe.symbols.length > 0) {
        for (const sym of fe.symbols) {
          const symLeaf = resolveVisibleLeaf(fe.to, sym);
          if (symLeaf && symLeaf !== srcLeaf) targets.push(symLeaf);
        }
      }
      if (targets.length === 0) {
        const tgtLeaf = resolveVisibleLeaf(fe.to, null);
        if (tgtLeaf && tgtLeaf !== srcLeaf) targets.push(tgtLeaf);
      }

      for (const tgtLeaf of targets) {
        if (!fe.isCut) {
          caSets.get(tgtLeaf)?.add(fe.from);
          ceSets.get(srcLeaf)?.add(fe.to);
        }
        const key = edgeKey(srcLeaf, tgtLeaf);
        if (!edgeAcc.has(key)) {
          edgeAcc.set(key, {
            id: key,
            from: srcLeaf,
            to: tgtLeaf,
            weight: 0,
            activeWeight: 0,
            dipWeight: 0,
            cutCount: 0,
            isRuleViolation: false,
            isTemporal: false,
            fileEdges: [],
          });
        }
        const agg = edgeAcc.get(key);
        agg.weight += 1;
        if (fe.isCut) agg.cutCount += 1;
        else agg.activeWeight += 1;
        if (fe.is_dip) agg.dipWeight += 1;
        if (fe.isRuleViolation) agg.isRuleViolation = true;
        agg.fileEdges.push(fe);
      }
    }

    // Compute live Instability, Abstractness, Distance, Zone, Tarjan SCC on visible leaves
    const leafIds = [...visibleLeaves];
    const activeEdgesForScc = [...edgeAcc.values()].filter(e => e.activeWeight > 0);
    const { nodeScc, inCycle } = computeTarjanScc(leafIds, activeEdgesForScc);
    const band = mainSequenceBand(report);
    const tol = sdpTolerance(report);

    const leafMetrics = new Map();
    for (const id of leafIds) {
      const node = hierarchy.nodes.get(id);
      const ca = caSets.get(id)?.size || 0;
      const ce = ceSets.get(id)?.size || 0;
      leafMetrics.set(id, nodeMetrics(node, ca, ce, band, inCycle.has(id)));
    }

    // Also attach metrics to containers (rolled up)
    for (const vn of visibleNodes) {
      vn.metrics = leafMetrics.get(vn.id) || {
        ca: 0,
        ce: 0,
        instability: null,
        abstractness: null,
        distance: null,
        zone: 'container',
        activePain: vn.data.hotspot >= HOTSPOT_THRESHOLD,
        inCycle: false,
      };
    }

    let edges = [...edgeAcc.values()].map(e => {
      const mFrom = leafMetrics.get(e.from);
      const mTo = leafMetrics.get(e.to);
      const isAllCut = e.activeWeight === 0 && e.cutCount > 0;
      const isCycleEdge =
        !isAllCut && inCycle.has(e.from) && nodeScc.get(e.from) === nodeScc.get(e.to);
      const isSdpViolation =
        !isAllCut &&
        mFrom &&
        mTo &&
        mFrom.instability != null &&
        mTo.instability != null &&
        mTo.instability - mFrom.instability > tol;
      const isDipOnly = e.weight > 0 && e.dipWeight === e.weight;
      return {
        ...e,
        isAllCut,
        isCycleEdge,
        isSdpViolation,
        isDipOnly,
      };
    });

    // Optional hidden temporal coupling edges overlay (from Git history)
    if (showTemporal && report && report.git && report.git.temporal_couplings) {
      const existingPairs = new Set(edges.map(e => e.id));
      const addedTemp = new Set();
      for (const tc of report.git.temporal_couplings) {
        if (tc.has_static_edge) continue;
        const a = resolveVisibleLeaf(tc.file_a, null);
        const b = resolveVisibleLeaf(tc.file_b, null);
        if (!a || !b || a === b) continue;
        const pairKey = a < b ? edgeKey(a, b) : edgeKey(b, a);
        if (addedTemp.has(pairKey) || existingPairs.has(edgeKey(a, b)) || existingPairs.has(edgeKey(b, a))) {
          continue;
        }
        addedTemp.add(pairKey);
        edges.push({
          id: `temp:${pairKey}`,
          from: a,
          to: b,
          weight: tc.shared_commits,
          activeWeight: tc.shared_commits,
          dipWeight: 0,
          cutCount: 0,
          isAllCut: false,
          isCycleEdge: false,
          isSdpViolation: false,
          isDipOnly: false,
          isRuleViolation: false,
          isTemporal: true,
          temporalStrength: tc.strength,
          fileEdges: [],
          temporalInfo: tc,
        });
      }
    }

    // Apply edge filter + minWeight
    edges = edges.filter(e => {
      if (e.isTemporal) return true;
      if (e.weight < minWeight) return false;
      if (edgeFilter === 'cycles' && !e.isCycleEdge) return false;
      if (edgeFilter === 'sdp' && !e.isSdpViolation) return false;
      if (edgeFilter === 'rules' && !e.isRuleViolation) return false;
      if (edgeFilter === 'dip' && e.dipWeight === 0) return false;
      return true;
    });

    // Count & optionally hide isolated leaf nodes (zero incident edges) to keep the graph clean
    const connectedIds = new Set();
    for (const e of edges) {
      connectedIds.add(e.from);
      connectedIds.add(e.to);
    }

    let hiddenIsolatedCount = 0;
    let finalNodes = visibleNodes;
    if (edgeFilter !== 'all' || (hideIsolated && !q)) {
      const kept = new Set();
      for (const n of visibleNodes) {
        if (n.isExpanded) continue;
        const isConn = connectedIds.has(n.id);
        if (edgeFilter !== 'all') {
          if (isConn) kept.add(n.id);
        } else if (hideIsolated) {
          if (isConn) kept.add(n.id);
          else hiddenIsolatedCount++;
        } else {
          kept.add(n.id);
        }
      }
      // Keep parent containers that have at least one kept descendant
      const byId = new Map(visibleNodes.map(n => [n.id, n]));
      for (const id of [...kept]) {
        let p = byId.get(id)?.parentId;
        while (p) {
          kept.add(p);
          p = byId.get(p)?.parentId;
        }
      }
      // If everything would be hidden (e.g. a purely isolated repo), keep all nodes
      if (kept.size > 0 || edgeFilter !== 'all') {
        finalNodes = visibleNodes.filter(n => kept.has(n.id));
      } else {
        hiddenIsolatedCount = 0;
      }
    }

    return {
      nodes: finalNodes,
      edges,
      leafIds: finalNodes.filter(n => !n.isExpanded).map(n => n.id),
      hiddenIsolatedCount,
    };
  }

  // Compute N-hop upstream / downstream blast radius from startId
  function computeBlastRadius(edges, startId, maxDepth = Infinity, direction = 'both') {
    const outAdj = new Map();
    const inAdj = new Map();
    for (const e of edges) {
      if (e.isTemporal || e.isAllCut) continue;
      if (!outAdj.has(e.from)) outAdj.set(e.from, []);
      if (!inAdj.has(e.to)) inAdj.set(e.to, []);
      outAdj.get(e.from).push(e.to);
      inAdj.get(e.to).push(e.from);
    }

    const result = new Map([[startId, { hop: 0, dir: 'self' }]]);

    function bfs(adj, dirLabel) {
      const q = [[startId, 0]];
      const visited = new Set([startId]);
      while (q.length > 0) {
        const [cur, d] = q.shift();
        if (d >= maxDepth) continue;
        for (const nxt of adj.get(cur) || []) {
          if (!visited.has(nxt)) {
            visited.add(nxt);
            const prev = result.get(nxt);
            if (!prev || d + 1 < prev.hop) {
              result.set(nxt, { hop: d + 1, dir: prev && prev.dir !== dirLabel ? 'both' : dirLabel });
            } else if (prev.dir !== dirLabel && prev.dir !== 'self') {
              prev.dir = 'both';
            }
            q.push([nxt, d + 1]);
          }
        }
      }
    }

    if (direction === 'downstream' || direction === 'both') bfs(outAdj, 'downstream');
    if (direction === 'upstream' || direction === 'both') bfs(inAdj, 'upstream');
    return result;
  }

  // Find shortest & near-shortest simple paths from sourceId to targetId
  function findPaths(edges, sourceId, targetId, maxPaths = 6) {
    if (!sourceId || !targetId || sourceId === targetId) return [];
    const adj = new Map();
    for (const e of edges) {
      if (e.isTemporal || e.isAllCut) continue;
      if (!adj.has(e.from)) adj.set(e.from, []);
      adj.get(e.from).push(e.to);
    }

    const found = [];
    let shortestLen = Infinity;
    const queue = [[sourceId]];
    let steps = 0;

    while (queue.length > 0 && found.length < maxPaths && steps < 4000) {
      steps++;
      const path = queue.shift();
      if (path.length > shortestLen + 2 || path.length > 9) continue;
      const last = path[path.length - 1];
      for (const nxt of adj.get(last) || []) {
        if (nxt === targetId) {
          const full = [...path, nxt];
          if (full.length < shortestLen) shortestLen = full.length;
          found.push(full);
          if (found.length >= maxPaths) break;
        } else if (!path.includes(nxt)) {
          queue.push([...path, nxt]);
        }
      }
    }
    return found;
  }

  return {
    esc,
    fmt,
    ideUri,
    edgeKey,
    computeTarjanScc,
    computePropagationCost,
    buildHierarchy,
    mainSequenceBand,
    nodeMetrics,
    projectCompoundGraph,
    computeBlastRadius,
    findPaths,
  };
})();
