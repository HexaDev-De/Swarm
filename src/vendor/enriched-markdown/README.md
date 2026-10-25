# Enriched Markdown parser (vendored)

`md4c.js` is the Markdown parser that Software Mansion's
[Enriched Markdown](https://github.com/software-mansion/enriched-markdown) ships for the web:
md4c compiled to WebAssembly, with the binary inlined.

- Source: `react-native-enriched-markdown@1.1.1` on npm, file `lib/module/web/wasm/md4c.js`
- Tarball integrity: `sha512-JvbRIVy75MAoGCv8V6sSscu11/Ian588wfIgH+j64XFpf6gy5sNNkMyG087MH88jNCf3p7fTjgEAGxZMhEqL4A==`
- Change: the trailing CommonJS/AMD export block is replaced with `export default createMd4cModule;` so Vite can import it as a module. Nothing else is modified.
- License: MIT, see `LICENSE`.

It is vendored rather than installed because the npm package declares `react` and
`react-native` as required peers, which npm would install into this non-React app.
The renderer that turns its syntax tree into HTML lives in `src/enriched-markdown.ts`.

To update: `npm pack react-native-enriched-markdown@<version> --ignore-scripts`, copy the
file, reapply the export change, and update the version and integrity above.
