// noinspection JSUnresolvedLibraryURL

const fs = require('fs');
const { gzipSync } = require('node:zlib');

const BuildDocs = process.argv.includes('--docs');

const DIST = BuildDocs ? './docs' : './dist';

const CurveCore = fs.readFileSync('node_modules/@mojs/core/dist/mo.umd.js', { encoding: 'utf8' });
const Player = fs.readFileSync('node_modules/@mojs/player/build/mojs-player.min.js', { encoding: 'utf8' });
const CurveEditor = fs.readFileSync('node_modules/@mojs/curve-editor/app/build/mojs-curve-editor.min.js', { encoding: 'utf8' });

/**
 * @param {string} str
 * @param {string} oldStr
 * @param {string} newStr
 * @return {string}
 */
function replace(str, oldStr, newStr) {
  const index = str.indexOf(oldStr);
  return str.substring(0, index) + newStr + str.substring(index + oldStr.length);
}

let IndexHTML = fs.readFileSync('./index.html', { encoding: 'utf8' });
IndexHTML = replace(
  IndexHTML, '<script id="allape_dev_id_core" src="node_modules/@mojs/core/dist/mo.umd.js"></script>',
  BuildDocs ? '<script src="https://cdn.jsdelivr.net/npm/@mojs/core"></script>' : `<script>${CurveCore}</script>`,
);
IndexHTML = replace(
  IndexHTML,
  '<script id="allape_dev_id_player" src="node_modules/@mojs/player/build/mojs-player.js"></script>',
  BuildDocs ? '<script src="https://cdn.jsdelivr.net/npm/@mojs/player"></script>' : `<script>${Player}</script>`,
);
IndexHTML = replace(
  IndexHTML,
  '<script id="allape_dev_id_curve_editor" src="node_modules/@mojs/curve-editor/app/build/mojs-curve-editor.js"></script>',
  BuildDocs ? '<script src="https://cdn.jsdelivr.net/npm/@mojs/curve-editor"></script>' : `<script>${CurveEditor}</script>`,
);

fs.mkdirSync(DIST, { recursive: true });
fs.writeFileSync(`${DIST}/index.html`, IndexHTML);
if (!BuildDocs) {
  const gzipped = gzipSync(IndexHTML);
  fs.writeFileSync(`${DIST}/index.html.gz`, gzipped);
  fs.writeFileSync(`./esp32/src/assets/index.html.gz`, gzipped);
}
