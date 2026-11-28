const model = await Bun.file(new URL('../src/report/assets/model.js', import.meta.url)).text();
const smoke = `
const nodes = ['client', 'library'];
const edges = [{ from: 'client', to: 'library' }];
if (ArchModel.computePropagationCost(nodes, edges) !== 0.75) {
  throw new Error('The report model did not preserve the client dependency.');
}
if (ArchModel.computeTarjanScc(nodes, edges).inCycle.size !== 0) {
  throw new Error('A one-way dependency must remain acyclic.');
}
`;
const stdin = new TextEncoder().encode(model + '\n' + smoke);
for (const [runtime, command] of [
  ['Bun', [Bun.argv[0], 'run', '-']],
  ['Node', ['node', '--input-type=module', '-']],
] as const) {
  const child = Bun.spawnSync([...command], { stdin, stdout: 'inherit', stderr: 'inherit' });
  if (child.exitCode !== 0) {
    throw new Error(`${runtime} could not execute the report model`);
  }
  console.log(`Report model works in ${runtime}`);
}
