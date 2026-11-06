// Bundles the frontend into web/dist, which the Rust server embeds at compile time.
import * as esbuild from 'esbuild';
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { basename } from 'node:path';

const watch = process.argv.includes('--watch');
rmSync('dist', { recursive: true, force: true });
mkdirSync('dist/assets', { recursive: true });
cpSync('public', 'dist', { recursive: true });

const common = {
  outdir: 'dist/assets',
  bundle: true,
  minify: !watch,
  sourcemap: watch,
  target: ['es2022'],
  logLevel: 'info',
  legalComments: 'none',
  // Content hashes in file names: /assets/* is served with an immutable cache policy.
  entryNames: watch ? '[name]' : '[name]-[hash]',
  metafile: true,
};

const builds = [
  // Application: ES modules with code splitting so highlight.js / marked / QR load on demand.
  {
    ...common,
    entryPoints: { app: 'src/main.ts', style: 'styles/main.css' },
    format: 'esm',
    splitting: true,
    chunkNames: 'chunks/[name]-[hash]',
    assetNames: 'fonts/[name]-[hash]',
    loader: { '.woff2': 'file', '.woff': 'file' },
  },
  // Theme bootstrap: tiny blocking script so the right theme paints first.
  { ...common, entryPoints: { theme: 'src/theme.ts' }, format: 'iife' },
];

/** Rewrites the asset references in index.html to the hashed output names. */
function writeIndex(results) {
  const names = {};
  for (const result of results) {
    for (const [file, output] of Object.entries(result.metafile.outputs)) {
      if (!output.entryPoint) continue;
      const source = basename(output.entryPoint).replace(/\.(ts|css)$/, '');
      const key = source === 'main' && file.endsWith('.css') ? 'style' : source === 'main' ? 'app' : source;
      names[key] = basename(file);
    }
  }
  // Subresource Integrity for the entry assets referenced from index.html.
  const sri = (name) => `sha384-${createHash('sha384').update(readFileSync(`dist/assets/${name}`)).digest('base64')}`;
  const html = readFileSync('index.html', 'utf8')
    .replace('src="/assets/theme.js"', `src="/assets/${names.theme}" integrity="${sri(names.theme)}"`)
    .replace('href="/assets/style.css"', `href="/assets/${names.style}" integrity="${sri(names.style)}"`)
    .replace('src="/assets/app.js"', `src="/assets/${names.app}" integrity="${sri(names.app)}"`);
  if (html.includes('"/assets/theme.js"') || html.includes('"/assets/style.css"') || html.includes('"/assets/app.js"')) {
    throw new Error('index.html: failed to rewrite an asset reference');
  }
  writeFileSync('dist/index.html', html);
}

if (watch) {
  cpSync('index.html', 'dist/index.html');
  for (const options of builds) await (await esbuild.context(options)).watch();
} else {
  writeIndex(await Promise.all(builds.map((options) => esbuild.build(options))));
}
