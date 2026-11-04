#!/usr/bin/env node
// Source-build preparation only: never downloads grammars or modifies submodules.
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, copyFileSync, writeFileSync, cpSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const generatorVersion = '0.25.10';
const checkOnly = process.argv.includes('--check');

function run(command, args, cwd) {
  execFileSync(command, args, {
    cwd, stdio: 'inherit', shell: process.platform === 'win32' && command === 'npx',
  });
}

function filesUnder(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap(entry =>
    entry.isDirectory() ? filesUnder(join(dir, entry.name)) : [join(dir, entry.name)]).sort();
}

try {
  // Check every submodule before changing the disposable output directory.
  for (const [name, file] of [
    ['terraform', 'dialects/terraform/src/parser.c'],
    ['solidity', 'src/parser.c'], ['erlang', 'src/parser.c'],
    ['cfml', 'cfml/src/parser.c'], ['cfml', 'cfscript/src/parser.c'],
    ['arkts', 'src/parser.c'], ['vbnet', 'grammar.js'], ['cobol', 'grammar.js'],
  ]) {
    if (!existsSync(join(root, 'vendor', `tree-sitter-${name}`, file))) {
      throw new Error(`Missing ${name} source. Run git submodule update --init --recursive first.`);
    }
  }

  // Cargo omits nested Cargo packages even with explicit include patterns. Stage
  // their build inputs outside submodules for both checkout and source-package builds.
  for (const [name, directories] of [
    ['terraform', [['dialects/terraform/src', 'src']]],
    ['solidity', [['src', 'src']]], ['erlang', [['src', 'src']]],
    ['arkts', [['src', 'src'], ['common', 'common']]],
    ['cfml', [['cfml/src', 'cfml/src'], ['cfscript/src', 'cfscript/src'], ['common', 'common']]],
  ]) {
    const source = join(root, 'vendor', `tree-sitter-${name}`);
    const dest = join(root, '.grammar-build', name);
    const inputs = directories.flatMap(([from, to]) => filesUnder(join(source, from))
      .map(file => [file, join(to, file.slice(join(source, from).length + 1))]));
    inputs.push([join(source, 'LICENSE'), 'LICENSE']);
    const hash = createHash('sha256');
    for (const [file, relative] of inputs) hash.update(relative).update(readFileSync(file));
    const fingerprint = hash.digest('hex');
    const stamp = join(dest, '.prepared');
    if (existsSync(stamp) && readFileSync(stamp, 'utf8') === fingerprint &&
        inputs.every(([, relative]) => existsSync(join(dest, relative)))) {
      console.log(`${name}: prepared (cached)`);
      continue;
    }
    if (checkOnly) throw new Error(`${name} is not prepared or its inputs changed. Run npm run prepare:grammars.`);
    rmSync(dest, { recursive: true, force: true });
    mkdirSync(dest, { recursive: true });
    for (const [from, to] of directories) cpSync(join(source, from), join(dest, to), { recursive: true });
    copyFileSync(join(source, 'LICENSE'), join(dest, 'LICENSE'));
    writeFileSync(stamp, fingerprint);
    console.log(`${name}: prepared`);
  }

  for (const name of ['vbnet', 'cobol']) {
    const source = join(root, 'vendor', `tree-sitter-${name}`);
    const patch = join(root, 'grammars', 'patches', `tree-sitter-${name}.patch`);
    const config = join(root, 'grammars', `tree-sitter-${name}.json`);
    const dest = join(root, '.grammar-build', name);
    const inputs = ['grammar.js', ...(name === 'cobol' ? ['src/scanner.c'] : [])];
    const hash = createHash('sha256').update(generatorVersion);
    for (const file of [patch, config, ...inputs.map(f => join(source, f))]) {
      hash.update(readFileSync(file));
    }
    const fingerprint = hash.digest('hex');
    const stamp = join(dest, '.prepared');
    const outputs = ['src/parser.c', 'src/scanner.c', 'src/tree_sitter/parser.h'];
    if (existsSync(stamp) && readFileSync(stamp, 'utf8') === fingerprint &&
        outputs.every(file => existsSync(join(dest, file)))) {
      console.log(`${name}: prepared (cached)`);
      continue;
    }
    if (checkOnly) {
      throw new Error(`${name} is not prepared or its inputs changed. Run npm run prepare:grammars.`);
    }
    rmSync(dest, { recursive: true, force: true });
    mkdirSync(join(dest, 'src'), { recursive: true });
    // Windows checkouts may use CRLF; patches use the upstream LF representation.
    for (const file of inputs) {
      writeFileSync(join(dest, file), readFileSync(join(source, file), 'utf8').replace(/\r\n/g, '\n'));
    }
    copyFileSync(config, join(dest, 'tree-sitter.json'));
    // Apply from the parent root: Git otherwise filters paths by the cwd prefix.
    run('git', ['apply', '--no-index', `--directory=.grammar-build/${name}`, patch], root);
    run('npx', ['--yes', `--package=tree-sitter-cli@${generatorVersion}`, 'tree-sitter', 'generate'], dest);
    for (const file of outputs) {
      if (!existsSync(join(dest, file))) throw new Error(`Generator did not produce ${name}/${file}`);
    }
    copyFileSync(join(source, 'LICENSE'), join(dest, 'LICENSE'));
    copyFileSync(join(root, 'grammars', 'patches', 'LICENSE-CodeGraph'), join(dest, 'LICENSE-CodeGraph'));
    writeFileSync(stamp, fingerprint);
    console.log(`${name}: prepared`);
  }
} catch (error) {
  console.error(`Grammar preparation failed: ${error.message}`);
  process.exitCode = 1;
}
