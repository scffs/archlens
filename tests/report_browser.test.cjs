const assert = require('node:assert/strict');
const { execFileSync, spawn } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');
const { pathToFileURL } = require('node:url');

const root = path.join(__dirname, '..');

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

function readSmokeResult(chrome, report, profile) {
  return new Promise((resolve, reject) => {
    const group = process.platform !== 'win32';
    const browser = spawn(chrome, [
      '--headless', '--disable-gpu', '--disable-extensions', '--disable-background-networking',
      '--no-first-run',
      '--no-default-browser-check', '--user-data-dir=' + profile,
      '--dump-dom', pathToFileURL(report).href,
    ], { detached: group, stdio: ['ignore', 'pipe', 'pipe'] });
    let cleaned = false;
    const cleanup = () => {
      if (cleaned) return;
      cleaned = true;
      clearTimeout(timeout);
      if (browser.pid !== undefined) {
        try {
          process.kill(group ? -browser.pid : browser.pid, 'SIGKILL');
        } catch (error) {
          if (error.code !== 'ESRCH') throw error;
        }
      }
      browser.stdout.destroy();
      browser.stderr.destroy();
    };
    let tail = '';
    let errors = '';
    const fail = () => {
      cleanup();
      reject(new Error('Chrome did not render the report smoke result: ' + errors));
    };
    const timeout = setTimeout(fail, 15000);
    browser.stdout.setEncoding('utf8').on('data', chunk => {
      tail = (tail + chunk).slice(-4096);
      const result = tail.match(/<output id="report-smoke">([^<]*)<\/output>/)?.[1];
      if (result === undefined) return;
      // The page has reported its result. Chrome can hang during headless teardown.
      cleanup();
      resolve(result);
    });
    browser.stderr.setEncoding('utf8').on('data', chunk => {
      errors = (errors + chunk).slice(-2048);
    });
    browser.on('error', error => {
      cleanup();
      reject(error);
    });
    browser.on('close', fail);
  });
}

test('the CLI report works offline across graph, DSM, inspector and What-If', async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'archlens-report-'));
  try {
    const fixture = path.join(temp, 'source');
    fs.mkdirSync(path.join(fixture, 'src/app'), { recursive: true });
    fs.mkdirSync(path.join(fixture, 'src/core'), { recursive: true });
    fs.writeFileSync(path.join(fixture, 'src/app/entry.ts'), "import { value } from '../core/value';\n");
    fs.writeFileSync(path.join(fixture, 'src/core/value.ts'), 'export function value() { return 1; }\n');
    const output = path.join(temp, 'report.html');
    const binary = process.env.ARCHLENS_BIN ?? path.join(root, 'target/debug/archlens');
    execFileSync(binary, ['analyze', fixture, '--no-git', '--quiet', '--html', output], { timeout: 10000 });

    let html = fs.readFileSync(output, 'utf8');
    html = html.replace('</body>', () => '<script>(' + checkReport + ')()</script></body>');
    fs.writeFileSync(output, html);

    const chrome = process.env.CHROME_BIN ?? 'google-chrome';
    assert.equal(await readSmokeResult(chrome, output, path.join(temp, 'chrome')), 'ok');
  } finally {
    fs.rmSync(temp, { recursive: true, force: true, maxRetries: 3 });
  }
});

