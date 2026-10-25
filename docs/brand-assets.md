# Provider brand assets

Retrieved from provider-owned sources on September 12, 2026. These marks identify the corresponding integrations in PandaCode. They are not PandaCode branding and do not imply sponsorship or endorsement. The marks remain the property of Anthropic, OpenAI, and Moonshot AI respectively; the application's source license does not grant trademark rights.

| Local file | Original source | Preparation | Bytes |
| --- | --- | --- | ---: |
| `src/assets/providers/claude.svg` | [Claude's official website](https://claude.com/) — the orange spark path in its `ClaudeWordmark` SVG | Extracted the separate spark path, unchanged, into a 125 × 125 SVG canvas. Preserved the official `#D97757` fill. No tracing or redrawing. | 1,992 |
| `src/assets/providers/openai.svg` | [OpenAI's official logo archive](https://cdn.openai.com/brand/OpenAI-Logos-2025.zip), `OpenAI-logos(new)/SVGs/OpenAI-black-monoblossom.svg`, provided through its [design guidelines](https://openai.com/brand/) | Copied byte for byte. Preserved the black Blossom and its original canvas/clear space. Used to identify the OpenAI Codex integration. | 2,969 |
| `src/assets/providers/kimi.png` | [MoonshotAI/kimi-cli, web/public/logo.png](https://github.com/MoonshotAI/kimi-cli/blob/main/web/public/logo.png) ([raw asset](https://raw.githubusercontent.com/MoonshotAI/kimi-cli/main/web/public/logo.png)) | Copied byte for byte. The official repository provides this 128 × 128 PNG icon; it was not traced into an invented vector. Upstream Git blob: `033c0f4fb2e28a9daeca0205812d15bf3e547893`. | 11,486 |

Total source asset size: 16,447 bytes. The helper in `src/provider-logos.ts` resolves a fixed provider-to-local-asset mapping. Vite bundles the assets; rendering performs no remote brand-asset requests. Decorative images have empty alt text and `aria-hidden="true"`; adjacent provider text supplies the accessible name. Keep the logos proportional and preserve their colors when changing the interface.

SHA-256 checksums:

```text
2c98c5e8bc7c72a8faccaf501225ffe3ba2eb3a554e6bfc127c5485db1be3baf  claude.svg
7be72f1fea831d3ba81a545cee79b7e0ae69449d5d7837c9571ccbfb4aa1e00b  openai.svg
dbd00e2ad61ea8832ef0b024662a4a8a5d1b66f0599d5d42e1c9688b9d4cfdf6  kimi.png
```

## Swarm artwork

The app icon (`public/icon.svg`), decorative honeycombs silhouette (`public/honeycombs.svg`), and interface honeycombs mark in `src/dom.ts` are custom vector artwork for Swarm. Native icons are generated from the app SVG with the Tauri icon command. Decorative honeycombs silhouettes use theme-aware CSS masks; they do not animate, receive pointer events, or appear in the accessibility tree. Provider logos remain unchanged.

- OpenCode: `https://opencode.ai/favicon-96x96-v3.png` (official website favicon, retrieved 2026-09-16).
- Z.AI / GLM: `https://z-cdn.chatglm.cn/z-ai/static/logo.svg` (official chat.z.ai logo, retrieved 2026-09-16).
- Antigravity: `https://antigravity.google/assets/image/antigravity-logo.png` (official website logo, retrieved 2026-09-16).
- Xiaomi MiMo: `https://platform.xiaomimimo.com/favicon.png` (official open-platform site icon, 535 × 534 PNG, copied byte for byte, retrieved 2026-09-22; SHA-256 `30f5305380e8a2f85c652b905ef5e155e0c00c7d56d7ab6bd8b1eb40dad61ba7`). Identifies the MiMo Token Plan integration; the mark remains Xiaomi's property.
- Grok (xAI): `https://grok.com/images/android-chrome-192x192.png` (official grok.com site icon, 192 × 192 PNG, copied byte for byte, retrieved 2026-09-21; SHA-256 `768cbbf7a7528d6e7b3af8112d7cde43eb6c9c8989ad9b06be8047dae1227463`). Identifies the Grok Build integration; the mark remains xAI's property.
