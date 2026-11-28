import { $ } from 'bun';
import { expect, test } from 'bun:test';

const root = import.meta.dir + '/..';

function checkReport() {
  const output = document.createElement('output');
  output.id = 'report-smoke';
  document.body.append(output);
  const check = (condition, message) => {
    if (!condition) throw new Error(message);
  };
  const openTab = name => {
    document.querySelector('[data-tab="' + name + '"]').click();
    check(!document.getElementById('tab-' + name).classList.contains('hidden'), 'open ' + name);
  };

  try {
    check(document.getElementById('project-title').textContent.includes('archlens ·'), 'load report');
    openTab('dsm');
    document.querySelector('td.cell-dep').click();
    document.querySelector('#inspector [data-cut-file]').click();
    const banner = document.getElementById('whatif-banner');
    check(!banner.classList.contains('hidden'), 'show simulated cut');
    let plan;
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText: text => { plan = text; return Promise.resolve(); } },
    });
    document.getElementById('btn-whatif-export').click();
    check(plan?.includes('src/app/entry.ts→src/core/value.ts'), 'export simulated change');
    document.getElementById('btn-whatif-reset').click();
    check(banner.classList.contains('hidden'), 'reset simulated cut');
    document.getElementById('btn-focus-edge-graph').click();
    check(document.querySelector('[data-tab="graph"]').classList.contains('active'), 'focus graph edge');
    document.getElementById('btn-edge-close').click();
    check(!document.querySelector('#inspector [data-cut-file]'), 'clear selected edge');
    for (const name of ['mainseq', 'treemap', 'diagnostics', 'graph']) openTab(name);
    output.textContent = 'ok';
  } catch (error) {
    output.textContent = error.message;
  }
}

async function replaceEmbeddedAssets(html, originalDirectory) {
  const currentDirectory = root + '/src/report/assets';
  for (const asset of new Bun.Glob('*.js').scanSync(currentDirectory)) {
    const [original, replacement] = await Promise.all([
      Bun.file(originalDirectory + '/' + asset).text(),
      Bun.file(currentDirectory + '/' + asset).text(),
    ]);
    expect(html.split(original).length, 'embedded copy count for ' + asset).toBe(2);
    html = html.replace(original, () => replacement);
  }
  return html;
}

async function scanSmokeResult(stream) {
  const decoder = new TextDecoder();
  let tail = '';
  for await (const chunk of stream) {
    tail = (tail + decoder.decode(chunk, { stream: true })).slice(-4096);
    const result = tail.match(/<output id="report-smoke">([^<]*)<\/output>/)?.[1];
    if (result !== undefined) return result;
  }
}

async function readSmokeResult(chrome, report, profile) {
  const browser = Bun.spawn([
    chrome, '--headless', '--disable-gpu', '--disable-extensions', '--disable-background-networking',
    '--no-first-run', '--no-default-browser-check', '--user-data-dir=' + profile,
    '--dump-dom', Bun.pathToFileURL(report).href,
  ], { detached: true, stdout: 'pipe', stderr: 'pipe' });
  const errors = browser.stderr.text();
  let timeout;
  try {
    const result = await Promise.race([
      scanSmokeResult(browser.stdout),
      new Promise(resolve => { timeout = setTimeout(resolve, 15000); }),
    ]);
    if (result !== undefined) return result;
  } finally {
    clearTimeout(timeout);
    // Stop the private Chrome process group after the marker or timeout. Headless teardown can hang.
    await $`kill -KILL -- ${'-' + browser.pid}`.quiet().nothrow();
    await browser.exited;
  }
  throw new Error('Chrome did not render the report smoke result: ' + (await errors).slice(-2048));
}

test('the CLI report works offline across graph, DSM, inspector and What-If', async () => {
  const temporaryDirectory = Bun.env.TMPDIR ?? '/tmp';
  const temp = (await $`mktemp -d ${temporaryDirectory + '/archlens-report-XXXXXX'}`.quiet().text()).trim();
  try {
    const fixture = temp + '/source';
    await Bun.write(fixture + '/src/app/entry.ts', "import { value } from '../core/value';\n");
    await Bun.write(fixture + '/src/core/value.ts', 'export function value() { return 1; }\n');
    const output = temp + '/report.html';
    const binary = Bun.env.ARCHLENS_BIN ?? root + '/target/debug/archlens';
    const cli = Bun.spawn([binary, 'analyze', fixture, '--no-git', '--quiet', '--html', output], {
      stdout: 'ignore', stderr: 'pipe', timeout: 10000, killSignal: 'SIGKILL',
    });
    const errors = cli.stderr.text();
    if (await cli.exited !== 0) throw new Error('Archlens failed to generate the report: ' + await errors);

    let html = await Bun.file(output).text();
    // Replace only the script copies embedded by the CLI during mutation runs.
    const originalAssets = Bun.env.ARCHLENS_BASELINE_ASSETS;
    if (originalAssets) {
      html = await replaceEmbeddedAssets(html, originalAssets);
      const environment = { __STRYKER_ACTIVE_MUTANT__: Bun.env.__STRYKER_ACTIVE_MUTANT__ };
      html = html.replace('<head>', () => '<head><script>globalThis.process = {env:'
        + JSON.stringify(environment) + '};</script>');
    }
    html = html.replace('</body>', () => '<script>(' + checkReport + ')()</script></body>');
    await Bun.write(output, html);

    const chrome = Bun.env.CHROME_BIN ?? 'google-chrome';
    expect(await readSmokeResult(chrome, output, temp + '/chrome')).toBe('ok');
  } finally {
    await $`rm -rf ${temp}`.quiet();
  }
}, 30000);
