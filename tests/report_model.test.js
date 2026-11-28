const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');

const modelSource = fs.readFileSync(path.join(__dirname, '../src/report/assets/model.js'), 'utf8');
const model = vm.runInNewContext(modelSource + '\nArchModel');

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

  assert.equal(after.fileEdges[0].isCut, true);
  assert.equal(after.fileEdges[0].isRuleViolation, true);
  assert.equal(after.fileEdges[1].isCut, false);
  assert.equal(after.fileEdges[1].isRuleViolation, false);
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

  assert.equal(graph.edges.length, 2);
  assert.equal(graph.edges[0].weight, 1);
  assert.equal(graph.edges[1].weight, 1);
});
