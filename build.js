const fs = require('fs');
const { deflate } = require('node:zlib');

const BuildDocs = process.argv.includes('--docs');

const DIST = BuildDocs ? './docs' : './dist';

const CurveCore = fs.readFileSync('node_modules/@mojs/core/dist/mo.umd.js', { encoding: 'utf8' });
const Player = fs.readFileSync('node_modules/@mojs/player/build/mojs-player.min.js', { encoding: 'utf8' });
const CurveEditorMini = fs.readFileSync('node_modules/@mojs/curve-editor/app/build/mojs-curve-editor.min.js', { encoding: 'utf8' });

const IndexHTML = fs.readFileSync('./index.html', { encoding: 'utf8' })
  .replace(
    '<script src="node_modules/@mojs/core/dist/mo.umd.js"></script>',
    BuildDocs ? '<script src="https://cdn.jsdelivr.net/npm/@mojs/core"></script>' : `<script>${CurveCore}</script>`,
  )
  .replace(
    '<script src="node_modules/@mojs/player/build/mojs-player.js"></script>',
    BuildDocs ? '<script src="https://cdn.jsdelivr.net/npm/@mojs/player"></script>' : `<script>${Player}</script>`,
  )
  .replace(
    '<script src="node_modules/@mojs/curve-editor/app/build/mojs-curve-editor.js"></script>',
    BuildDocs ? '<script src="https://cdn.jsdelivr.net/npm/@mojs/curve-editor"></script>' : `<script>${CurveEditorMini}</script>`,
  )
;

deflate(IndexHTML, (err, buffer) => {
  if (err) {
    console.error(err);
    return;
  }

  fs.mkdirSync(DIST, { recursive: true });
  fs.writeFileSync(`${DIST}/index.html`, IndexHTML);

  if (!BuildDocs) fs.writeFileSync(`${DIST}/index.html.gz`, buffer);
});
