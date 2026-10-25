import test from 'node:test';
import assert from 'node:assert/strict';
import { fileReference, findFileReferences, isPreviewableFile } from '../src/file-links.ts';

test('workspace filenames support line locations, spaces, absolute paths and Windows paths', () => {
  for (const [input, expected] of [
    ['src/app.ts:12:4', 'src/app.ts'], ['index.html#L12-L20', 'index.html'],
    ['design/My mockup.svg', 'design/My mockup.svg'], ['design/My%20mockup.svg', 'design/My mockup.svg'], ['/tmp/demo/index.html', '/tmp/demo/index.html'],
    ['C:\\project\\index.html:12', 'C:\\project\\index.html'], ['README.md', 'README.md'], ['file:///workspace/index.html', '/workspace/index.html'], ['file://localhost/workspace/index.html', '/workspace/index.html'], ['file:///C:/project/index.html', 'C:/project/index.html'],
    ['./public/logo.svg', './public/logo.svg'],
  ]) assert.equal(fileReference(input), expected);
});

test('network links, executable schemes and malformed references cannot become workspace actions', () => {
  for (const input of ['https://example.com/index.html', '//example.com/index.html', 'javascript:alert(1)',
    'data:text/html,test.html', 'file:///etc/passwd', 'mailto:me@example.com', 'file://remote/workspace/index.html', 'file:///workspace/index.html?secret=1', '#section',
    'index.html?secret=1', '%00secret.html', 'https%3A%2F%2Fexample.com%2Findex.html', 'index.html#something', 'app.ts\nrun-command', '<script>.html', 'x'.repeat(1100) + '.html']) {
    assert.equal(fileReference(input), undefined, input);
  }
});

test('plain response references preserve punctuation and never extract partial paths from URLs', () => {
  const value = 'See src/app.ts:12, ./index.html. Also logo.svg and https://example.com/harmful.html, me@example.com. Version 1.2.3 is ready.';
  assert.deepEqual(findFileReferences(value).map(ref => ({ text: value.slice(ref.start, ref.end), path: ref.path })), [
    { text: 'src/app.ts:12', path: 'src/app.ts' }, { text: './index.html', path: './index.html' },
    { text: 'logo.svg', path: 'logo.svg' },
  ]);
});

test('only browser-renderable assets are previewable', () => {
  for (const path of ['index.html', 'PUBLIC/LOGO.SVG', 'site.htm', 'images/a.webp', 'demo.pdf']) assert.equal(isPreviewableFile(path), true);
  for (const path of ['src/main.ts', 'config.json', 'movie.exe', 'readme.md', 'index.html.js']) assert.equal(isPreviewableFile(path), false);
});
