#!/usr/bin/env node
import { spawn, execFileSync } from 'node:child_process';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { MANIFEST, buildPrompt } from './manifest.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const RAW_DIR = path.join(HERE, 'raw-assets');
const LOG_DIR = path.join(HERE, 'logs');
const CODEX_OUT = 'C:\\Users\\Saqlain\\.codex\\generated_images';

function parseArgs(argv) {
  const opts = {
    only: null,
    concurrency: 8,
    force: false,
    attach: true,
    model: 'gpt-6.2-astra',
    timeoutMs: 240_000,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--only' && argv[i + 1]) opts.only = new Set(argv[++i].split(',').map((s) => s.trim()).filter(Boolean));
    else if (a === '--concurrency' && argv[i + 1]) opts.concurrency = Math.max(1, Number(argv[++i]) || 8);
    else if (a === '--model' && argv[i + 1]) opts.model = argv[++i];
    else if (a === '--force') opts.force = true;
    else if (a === '--no-attach') opts.attach = false;
    else if (a === '--timeout' && argv[i + 1]) opts.timeoutMs = Math.max(30_000, Number(argv[++i]) || 240_000);
  }
  return opts;
}

function resolveCodexBinary() {
  if (process.platform !== 'win32') return { cmd: 'codex', useShell: false };
  try {
    const lines = execFileSync('where.exe', ['codex'], { encoding: 'utf8' })
      .split(/\r?\n/)
      .map((s) => s.trim())
      .filter(Boolean);
    const exe = lines.find((l) => l.toLowerCase().endsWith('.exe'));
    if (exe) return { cmd: exe, useShell: false };
    const cmd = lines.find((l) => l.toLowerCase().endsWith('.cmd'));
    if (cmd) return { cmd, useShell: true };
  } catch {
    // Fallback to shell invocation on Windows if where.exe misses.
  }
  return { cmd: 'codex', useShell: true };
}

async function exists(p) {
  try {
    await fs.access(p);
    return true;
  } catch {
    return false;
  }
}

async function findNewestPng(dir) {
  const entries = await fs.readdir(dir, { withFileTypes: true });
  const candidates = [];
  for (const e of entries) {
    if (!e.isFile() || !e.name.toLowerCase().endsWith('.png')) continue;
    const full = path.join(dir, e.name);
    const st = await fs.stat(full);
    candidates.push({ full, mtime: st.mtimeMs });
  }
  candidates.sort((a, b) => b.mtime - a.mtime);
  return candidates[0]?.full ?? null;
}

async function generateOne(item, opts, codexBin) {
  const outPath = path.join(RAW_DIR, `${item.id}.png`);
  const logPath = path.join(LOG_DIR, `${item.id}.log`);
  if (!opts.force && (await exists(outPath))) {
    console.log(`[SKIP] ${item.id} (already exists)`);
    return { id: item.id, status: 'skip' };
  }

  const start = Date.now();
  const args = [
    'exec',
    '-m',
    opts.model,
    '--skip-git-repo-check',
    '--sandbox',
    'read-only',
    '-c',
    'model_reasoning_effort="low"',
    '--color',
    'never',
  ];

  let attachedRef = false;
  if (item.editOf && opts.attach) {
    const refPath = path.join(RAW_DIR, `${item.editOf}.png`);
    if (await exists(refPath)) {
      args.push('-i', refPath);
      attachedRef = true;
    } else {
      console.warn(`[WARN] ${item.id}: missing editOf parent ${item.editOf}.png, generating standalone`);
    }
  }
  args.push('-');

  const prompt = buildPrompt(item);

  const result = await new Promise((resolve) => {
    const child = spawn(codexBin.cmd, args, {
      stdio: ['pipe', 'pipe', 'pipe'],
      shell: codexBin.useShell,
      windowsHide: true,
    });

    let stdout = '';
    let stderr = '';
    let timedOut = false;

    const timer = setTimeout(() => {
      timedOut = true;
      child.kill('SIGTERM');
    }, opts.timeoutMs);

    child.stdout.on('data', (chunk) => { stdout += chunk.toString(); });
    child.stderr.on('data', (chunk) => { stderr += chunk.toString(); });
    child.on('error', (err) => {
      clearTimeout(timer);
      resolve({ code: -1, stdout, stderr: `${stderr}\nSpawn error: ${err.message}`, timedOut });
    });
    child.on('close', (code) => {
      clearTimeout(timer);
      resolve({ code, stdout, stderr, timedOut });
    });

    child.stdin.write(prompt);
    child.stdin.end();
  });

  const combined = `=== COMMAND ===\n${codexBin.cmd} ${args.join(' ')}\nATTACHED_REF=${attachedRef}\n\n=== STDOUT ===\n${result.stdout}\n\n=== STDERR ===\n${result.stderr}\n`;
  await fs.writeFile(logPath, combined, 'utf8');

  const sec = ((Date.now() - start) / 1000).toFixed(1);
  if (result.timedOut) {
    console.error(`[FAIL] ${item.id} (${sec}s): timed out`);
    return { id: item.id, status: 'fail', reason: 'timeout' };
  }

  const m = combined.match(/session id:\s*([0-9a-f-]{36})/i);
  if (!m) {
    console.error(`[FAIL] ${item.id} (${sec}s): no session id found in output (exit ${result.code})`);
    return { id: item.id, status: 'fail', reason: 'no_session_id' };
  }

  const sessionId = m[1];
  const sessionDir = path.join(CODEX_OUT, sessionId);
  if (!(await exists(sessionDir))) {
    console.error(`[FAIL] ${item.id} (${sec}s): session folder missing (${sessionDir})`);
    return { id: item.id, status: 'fail', reason: 'missing_session_dir' };
  }

  const pngPath = await findNewestPng(sessionDir);
  if (!pngPath) {
    console.error(`[FAIL] ${item.id} (${sec}s): no PNG produced in ${sessionDir}`);
    return { id: item.id, status: 'fail', reason: 'no_png' };
  }

  await fs.copyFile(pngPath, outPath);
  const st = await fs.stat(outPath);
  console.log(`[OK]   ${item.id} (${sec}s, ${(st.size / 1024).toFixed(0)} KB, session=${sessionId}${attachedRef ? `, -i ${item.editOf}` : ''})`);
  return { id: item.id, status: 'ok', sessionId };
}

async function runPool(items, concurrency, fn) {
  const results = [];
  let cursor = 0;
  async function worker() {
    while (cursor < items.length) {
      const idx = cursor++;
      results[idx] = await fn(items[idx]);
    }
  }
  await Promise.all(Array.from({ length: Math.min(concurrency, items.length) }, worker));
  return results;
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  await fs.mkdir(RAW_DIR, { recursive: true });
  await fs.mkdir(LOG_DIR, { recursive: true });

  const codexBin = resolveCodexBinary();
  const selected = opts.only ? MANIFEST.filter((m) => opts.only.has(m.id)) : MANIFEST;
  if (!selected.length) {
    console.error('No matching manifest items for --only.');
    process.exit(1);
  }

  const roots = selected.filter((m) => !m.editOf);
  const children = selected.filter((m) => m.editOf);

  console.log(`Generating ${selected.length} asset(s) (model=${opts.model}, concurrency=${opts.concurrency}, attach=${opts.attach}) via ${codexBin.cmd}...`);
  const results = [];
  if (roots.length) {
    results.push(...(await runPool(roots, opts.concurrency, (item) => generateOne(item, opts, codexBin))));
  }
  if (children.length) {
    results.push(...(await runPool(children, opts.concurrency, (item) => generateOne(item, opts, codexBin))));
  }

  const failed = results.filter((r) => r.status === 'fail');
  console.log(`\nSummary: ${results.filter((r) => r.status === 'ok').length} ok, ${results.filter((r) => r.status === 'skip').length} skipped, ${failed.length} failed.`);
  if (failed.length) process.exit(1);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
