import { expect, test } from 'bun:test';

// The HTML embeds this script without module exports. Expose its existing API for Bun.
Bun.plugin({
  name: 'load the report model',
  setup(build) {
    build.onLoad({ filter: /[/]report[/]assets[/]model[.]js$/ }, async ({ path }) => ({
      contents: await Bun.file(path).text() + '\nexport default ArchModel;',
      loader: 'js',
    }));
  },
});
const { default: model } = await import('../src/report/assets/model.js');

function sourceFile(filePath, moduleId = filePath) {
  return { path: filePath, module_id: moduleId, language: 'javascript' };
}

test('file edges with an arrow in a path retain separate cuts and rule violations', () => {
  const first = { from: 'a.js', to: 'b.js→c.js' };
  const second = { from: 'a.js→b.js', to: 'c.js' };
  const report = {
    file_graph: {
      files: [first.from, first.to, second.from, second.to].map(file => sourceFile(file)),
      edges: [first, second],
    },
    rule_violations: [{ from_file: first.from, to_file: first.to }],
  };

  const before = model.buildHierarchy(report);
  const cutEdges = new Set([before.fileEdges[0].key]);
  const after = model.buildHierarchy(report, { cutEdges });

  expect(after.fileEdges[0].isCut).toBe(true);
  expect(after.fileEdges[0].isRuleViolation).toBe(true);
  expect(after.fileEdges[1].isCut).toBe(false);
  expect(after.fileEdges[1].isRuleViolation).toBe(false);
});

test('visible edges with an arrow in a module ID stay distinct', () => {
  const report = {
    file_graph: {
      files: [
        sourceFile('from-a.js', 'a'),
        sourceFile('to-bc.js', 'b→mod:c'),
        sourceFile('from-ab.js', 'a→mod:b'),
        sourceFile('to-c.js', 'c'),
      ],
      edges: [
        { from: 'from-a.js', to: 'to-bc.js' },
        { from: 'from-ab.js', to: 'to-c.js' },
      ],
    },
  };

  const hierarchy = model.buildHierarchy(report);
  const graph = model.projectCompoundGraph(hierarchy, { expandedIds: new Set(), report });

  expect(graph.edges.length).toBe(2);
  expect(graph.edges[0].weight).toBe(1);
  expect(graph.edges[1].weight).toBe(1);
});

test('an explicitly zero Main Sequence band is used in the projected graph', () => {
  const target = sourceFile('core.js');
  target.symbols = { abstract_declarations: 3, concrete_declarations: 1 };
  const report = {
    main_sequence_band: 0,
    file_graph: {
      files: [sourceFile('entry.js'), target],
      edges: [{ from: 'entry.js', to: target.path }],
    },
  };

  const hierarchy = model.buildHierarchy(report);
  const graph = model.projectCompoundGraph(hierarchy, { expandedIds: new Set(), report });
  const core = graph.nodes.find(node => node.id === 'mod:core.js');

  expect(core.metrics.distance).toBe(0.25);
  expect(core.metrics.zone).toBe('zone_of_pain');
});

test('an explicitly zero SDP tolerance is used in both report graph and simulation', () => {
  const edges = [
    { from: 'source', to: 'target' },
    { from: 'source', to: 'source-dependency' },
    { from: 'source-caller', to: 'source' },
    { from: 'target-caller', to: 'target' },
    ...Array.from({ length: 5 }, (_, i) => ({ from: 'target', to: 'target-dependency-' + i })),
  ];
  const paths = new Set(edges.flatMap(edge => [edge.from, edge.to]));
  const report = {
    sdp_tolerance: 0,
    file_graph: { files: [...paths].map(file => sourceFile(file)), edges },
  };

  const hierarchy = model.buildHierarchy(report);
  const graph = model.projectCompoundGraph(hierarchy, { expandedIds: new Set(), report });
  const edge = graph.edges.find(edge => edge.from === 'mod:source' && edge.to === 'mod:target');

  expect(hierarchy.moduleMetrics.sdpCount).toBe(1);
  expect(edge.isSdpViolation).toBe(true);
});

test('cycles and propagation measure the same directed graph', () => {
  const ids = ['a', 'b', 'c'];
  const edges = [
    { from: 'a', to: 'b' },
    { from: 'a', to: 'b' },
    { from: 'b', to: 'a' },
    { from: 'c', to: 'c' },
    { from: 'c', to: 'unknown' },
    { from: 'unknown', to: 'a' },
  ];

  const { inCycle } = model.computeTarjanScc(ids, edges);
  expect([...inCycle].sort()).toEqual(['a', 'b']);
  expect(model.computePropagationCost(ids, edges)).toBe(5 / 9);
});

function randomFrom(seed) {
  let state = seed;
  return limit => {
    state = (Math.imul(1664525, state) + 1013904223) >>> 0;
    return Math.floor(state / 2 ** 32 * limit);
  };
}

test('generated directed graphs agree with independent reachability', () => {
  const labels = ['entry', 'a→b', 'модуль', '中', '空 格', 'line\nbreak', 'leaf'];
  const pick = randomFrom(0x45f613);

  for (let sample = 0; sample < 64; sample++) {
    const ids = labels.slice(0, sample % (labels.length + 1));
    const edges = [];
    const reachable = ids.map((_, from) => ids.map((_, to) => from === to));

    for (const [from, source] of ids.entries()) {
      for (const [to, target] of ids.entries()) {
        if (pick(7) > sample % 7) continue;
        edges.push({ from: source, to: target });
        if (pick(4) === 0) edges.push({ from: source, to: target });
        reachable[from][to] = true;
      }
      edges.push({ from: source, to: 'unknown' }, { from: 'unknown', to: source });
    }

    for (const via of ids.keys()) {
      for (const from of ids.keys()) {
        for (const to of ids.keys()) {
          reachable[from][to] ||= reachable[from][via] && reachable[via][to];
        }
      }
    }

    const { nodeScc, inCycle } = model.computeTarjanScc(ids, edges);
    expect(nodeScc.size, `sample ${sample}: component membership`).toBe(ids.length);
    for (const [from, source] of ids.entries()) {
      const cyclic = ids.some((_, to) => from !== to && reachable[from][to] && reachable[to][from]);
      expect(inCycle.has(source), `sample ${sample}: cycle at ${source}`).toBe(cyclic);
      for (const [to, target] of ids.entries()) {
        expect(nodeScc.get(source) === nodeScc.get(target), `sample ${sample}: ${source} and ${target}`)
          .toBe(reachable[from][to] && reachable[to][from]);
      }
    }

    const cost = ids.length === 0 ? 0 : reachable.flat().filter(Boolean).length / ids.length ** 2;
    expect(model.computePropagationCost(ids, edges), `sample ${sample}: propagation`).toBe(cost);
  }
});

test('generated What-If edits retain exactly the visible imports and cuts', () => {
  const files = ['a.js', 'b.js→c.js', 'a.js→b.js', 'c.js', 'файл.ts', '空 格.js']
    .map((file, index) => sourceFile(file, index % 2 === 0 ? 'left' : 'right'));
  const pick = randomFrom(0x2b7d10);

  for (let sample = 0; sample < 48; sample++) {
    const edges = files.flatMap(({ path: from }) => files
      .map(({ path: to }) => ({ from, to }))
      .filter(() => pick(4) === 0));
    const movedFiles = new Map(files.filter(() => pick(3) === 0).map(({ path: file }) => [
      file, pick(2) === 0 ? 'left' : 'right',
    ]));
    const cutEdges = new Set(edges.filter(() => pick(2) === 0)
      .map(({ from, to }) => JSON.stringify([from, to])));
    const report = { file_graph: { files, edges } };
    const hierarchy = model.buildHierarchy(report, { movedFiles, cutEdges });
    const modules = new Map(files.map(file => [file.path, movedFiles.get(file.path) ?? file.module_id]));

    for (const showCutEdges of [true, false]) {
      const expected = new Map();
      for (const { from, to } of edges) {
        const cut = cutEdges.has(JSON.stringify([from, to]));
        const source = modules.get(from);
        const target = modules.get(to);
        if (source === target || (cut && !showCutEdges)) continue;
        const key = JSON.stringify([`mod:${source}`, `mod:${target}`]);
        const counts = expected.get(key) ?? [0, 0, 0];
        counts[0]++;
        counts[cut ? 2 : 1]++;
        expected.set(key, counts);
      }

      const projected = model.projectCompoundGraph(hierarchy, {
        expandedIds: new Set(), hideIsolated: false, report, showCutEdges,
      });
      const actual = new Map();
      for (const edge of projected.edges) {
        const key = JSON.stringify([edge.from, edge.to]);
        expect(edge.id).toBe(key);
        actual.set(key, [edge.weight, edge.activeWeight, edge.cutCount]);
      }
      expect(actual.size, `sample ${sample}: unique edges`).toBe(projected.edges.length);
      expect(actual, `sample ${sample}: showCutEdges=${showCutEdges}`).toEqual(expected);
    }
  }
});

test('reports isolated nodes, empty modules and the Main Sequence boundary', () => {
  const node = { abstractCount: 1, concreteCount: 3, hotspot: 1 };
  expect(model.nodeMetrics(node, 0, 0, 0.25)).toMatchObject({
    abstractness: 0.25, instability: null, distance: null,
    zone: 'isolated', activePain: false, inCycle: false,
  });
  expect(model.nodeMetrics({ abstractCount: 0, concreteCount: 0, hotspot: 1 }, 1, 1, 0.25))
    .toMatchObject({
      abstractness: null, instability: 0.5, distance: null,
      zone: 'no_entities', activePain: false,
    });
  expect(model.nodeMetrics(node, 1, 1, 0.25)).toMatchObject({
    abstractness: 0.25, instability: 0.5, distance: 0.25,
    zone: 'main_sequence', activePain: false, inCycle: false,
  });
});

test('marks active pain at the hotspot boundary only inside the Zone of Pain', () => {
  const node = { abstractCount: 1, concreteCount: 3, hotspot: 0.30 };
  expect(model.nodeMetrics(node, 3, 1, 0.25, true)).toMatchObject({
    abstractness: 0.25, instability: 0.25, distance: 0.5,
    zone: 'zone_of_pain', activePain: true, inCycle: true,
  });
  expect(model.nodeMetrics({ ...node, hotspot: 0.29 }, 3, 1, 0.25).activePain).toBe(false);
  expect(model.nodeMetrics({ abstractCount: 3, concreteCount: 1, hotspot: 1 }, 1, 3, 0.25))
    .toMatchObject({
      abstractness: 0.75, instability: 0.75, distance: 0.5,
      zone: 'zone_of_uselessness', activePain: false,
    });
});
