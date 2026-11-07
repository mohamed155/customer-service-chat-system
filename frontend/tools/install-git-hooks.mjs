// Installs the git hooks (lefthook) after `pnpm install`.
// Best-effort: it never fails the install, because Docker builds and CI jobs install without a
// git repository (infra/Dockerfile.frontend copies only package.json and the lockfile).
import { spawnSync } from 'node:child_process';
import { basename } from 'node:path';

const result = spawnSync('lefthook install', { shell: true, encoding: 'utf8' });
if (result.status === 0) process.exit(0);

const output = `${result.stderr ?? ''}${result.stdout ?? ''}`.trim();
const noGitRepository = /not a git repository|executable file not found|git.*not found/i.test(
  output,
);
const retry =
  basename(process.env.INIT_CWD ?? '') === 'frontend'
    ? 'pnpm exec lefthook install'
    : 'pnpm --dir frontend exec lefthook install';

if (noGitRepository) {
  console.warn(`lefthook install skipped: no git repository here. In a checkout, run: ${retry}`);
} else {
  console.warn(`lefthook install failed:\n${output}\nFix the problem above, then run: ${retry}`);
}
