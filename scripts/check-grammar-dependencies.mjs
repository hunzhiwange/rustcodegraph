#!/usr/bin/env node
import { execFileSync } from 'node:child_process';

// Guard the parent index, not downloaded files: submodules legitimately contain C.
const entries = execFileSync('git', ['ls-files', '--stage', '-z', '--', 'vendor', '.grammar-build'],
  { encoding: 'utf8' }).split('\0').filter(Boolean);
const invalid = entries.filter(entry => !entry.startsWith('160000 ') ||
  !/^160000 [a-f0-9]+ 0\tvendor\/tree-sitter-[^/]+$/.test(entry));
if (invalid.length) {
  console.error('Third-party grammar sources must be pinned Git submodules. Do not commit copied or generated sources:');
  console.error(invalid.join('\n'));
  process.exitCode = 1;
} else {
  console.log(`Grammar dependency policy passed (${entries.length} submodules).`);
}
