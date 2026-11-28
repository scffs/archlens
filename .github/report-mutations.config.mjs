const model = 'src/report/assets/model.js';
const modelLines = (await Bun.file(new URL('../' + model, import.meta.url)).text()).split('\n');

function modelRange(start, end) {
  const first = modelLines.findIndex(line => line.startsWith(start));
  const next = modelLines.findIndex(line => line.startsWith(end));
  if (first < 0 || next <= first) throw new Error('Update the report mutation scope for ' + start);
  return model + ':' + (first + 1) + '-' + next;
}

const asset = Bun.env.REPORT_MUTATION_ASSET ?? 'core';
const all = asset !== 'core';

export default {
  // Keep the PR gate bounded. The manual campaign exercises every shipped JS asset.
  mutate: all ? ['src/report/assets/' + asset] : [
    modelRange('  const edgeKey ', '  function ideUri('),
    modelRange('  function directedAdjacency(', '  // Tarjan SCC'),
    modelRange('  function nodeMetrics(', '  // Build hierarchical tree'),
  ],
  testRunner: 'command',
  commandRunner: {
    command: 'bun test ./tests/report_model.test.js'
      + (all ? ' ./tests/report_browser.test.mjs' : ''),
  },
  // The Stryker command runner reports test exit status without per-mutant coverage.
  coverageAnalysis: 'off',
  concurrency: all ? 4 : 2,
  // Let the CLI and Chrome reach their own limits and close the test profile.
  timeoutMS: all ? 28000 : 1500,
  thresholds: { break: all ? null : 95 },
  reporters: ['clear-text', 'progress', 'json'],
  ignorePatterns: ['/target', '/fuzz', '/mutants.out', '/mutants.out.old'],
};
