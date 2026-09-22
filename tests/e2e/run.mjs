import assert from 'node:assert/strict';
import { spawnSync, execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { createServer } from 'node:net';
import { mkdir, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const [suite = 'core', ...extra] = process.argv.slice(2);
assert(['core', 'resource', 'archive'].includes(suite) && !extra.length,
  'Usage: node tests/e2e/run.mjs [core|resource|archive]');
const root = fileURLToPath(new URL('../../', import.meta.url));
const protocol = suite === 'archive' ? '28' : (process.env.FUUL_E2E_PROTOCOL ?? '28');
assert(['27', '28'].includes(protocol), 'FUUL_E2E_PROTOCOL must be 27 or 28');
assert.match(execFileSync('stellar', ['--version'], { encoding: 'utf8' }), /^stellar 27\.1\.0 /);
const context = JSON.parse(execFileSync('docker', ['context', 'inspect'], { encoding: 'utf8' }))[0];
const endpoint = process.env.DOCKER_HOST || context.Endpoints.docker.Host;
assert(endpoint.startsWith('unix://'), 'E2E requires a local Docker Unix socket');
const socket = createServer();
await new Promise((resolve, reject) => socket.once('error', reject).listen(0, '127.0.0.1', resolve));
const port = socket.address().port;
await new Promise(resolve => socket.close(resolve));
const id = `fuul-${suite}-${randomUUID().slice(0, 8)}`;
const env = { ...process.env, FUUL_E2E_PORT: String(port), FUUL_E2E_PROTOCOL: protocol };
if (suite === 'archive') {
  const tag = process.env.FUUL_ARCHIVAL_IMAGE || 'fuul-core-archival:20260919-v28.0.1';
  const image = JSON.parse(execFileSync('docker', ['image', 'inspect', tag], { encoding: 'utf8' }))[0];
  assert.equal(image.Config.Labels['org.opencontainers.image.revision'], '947aad8413c189d85504acf72207e85eeda9b021');
  env.FUUL_ARCHIVAL_IMAGE = image.Id;
}
const compose = ['compose', '-p', id, '-f', `tests/e2e/compose${suite === 'archive' ? '.archival' : ''}.yaml`];
const directory = new URL(`../../.local/e2e/${id}/`, import.meta.url);
await mkdir(directory, { recursive: true });
function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, env, stdio: 'inherit' });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${command} failed (${result.signal || result.status})`);
}
let passed = false;
try {
  run('stellar', ['contract', 'build', '--locked']);
  run('docker', [...compose, 'up', '-d', '--wait', '--wait-timeout', '360', '--pull', suite === 'archive' ? 'never' : 'missing']);
  const paths = suite === 'core'
    ? ['support', 'fuul_factory', 'fuul_project', 'fuul_manager', 'fuul_upgrade']
    : [suite === 'resource' ? 'fuul_resource' : 'fuul_storage'];
  run('bun', ['test', ...paths.map(path => `./tests/e2e/${path}`), '--max-concurrency', '1', '--timeout', '900000']);
  passed = true;
} finally {
  const logs = spawnSync('docker', [...compose, 'logs', '--no-color'], { cwd: root, env, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  await writeFile(new URL('node.log', directory), logs.stdout || logs.stderr || '');
  const cleanup = spawnSync('docker', [...compose, 'down', '--volumes'], { cwd: root, env, stdio: 'inherit' });
  await writeFile(new URL('result.json', directory), JSON.stringify({ suite, protocol: Number(protocol), passed,
    cleanupPassed: cleanup.status === 0, image: env.FUUL_ARCHIVAL_IMAGE, finishedAt: new Date().toISOString() }, null, 2) + '\n');
  assert.equal(cleanup.status, 0, 'Could not remove this test network');
}
