# Local vendored dependencies

Bundled for offline prototype use; the page does not request a CDN at runtime.

| File | Version | Source | License |
| --- | --- | --- | --- |
| `qrcode.js` | qrcode-generator 2.0.4 | [upstream](https://github.com/kazuhikoarase/qrcode-generator), npm distribution `dist/qrcode.js` | MIT, see `qrcode-LICENSE.txt` |
| `jsQR.js` | jsqr 1.4.0 | [upstream](https://github.com/cozmo/jsQR), npm distribution `dist/jsQR.js` | Apache-2.0, see `jsQR-LICENSE.txt` |
| `mermaid.min.js` | mermaid 12.2.0 | [official npm tarball](https://registry.npmjs.org/mermaid/-/mermaid-12.2.0.tgz), npm distribution `dist/mermaid.min.js` | MIT, see `mermaid-LICENSE.txt`; retain nested bundled license comments |

Retrieved on 2026-10-24 from the versioned npm mirror:

- `https://cdn.jsdelivr.net/npm/qrcode-generator@2.0.4/dist/qrcode.js`
- `https://cdn.jsdelivr.net/npm/jsqr@1.4.0/dist/jsQR.js`

SHA-256 of unmodified JavaScript:

```text
79ec86f82856005b1c887905cfccfcfbec3821ca61c7fd5a952faa5f778f791c  qrcode.js
bc40c8a15196236b2314db0856f72ca0b49980cd5413b8c852a7349f5fee0859  jsQR.js
```

The QR generator includes its copyright/license header. Its license file was retrieved from the upstream repository `LICENSE`. The decoder license is included from the versioned npm distribution. Do not remove these notices when redistributing the prototype.

## Mermaid offline renderer

Mermaid is bundled for offline conversation diagrams in the formal client, without a runtime CDN or remote rendering service. The bundle is the **untouched** `dist/mermaid.min.js` from the official `mermaid@12.2.0` npm package:

- Package: `https://registry.npmjs.org/mermaid/-/mermaid-12.2.0.tgz`
- npm package integrity (SHA-512): `sha512-w7nr/SFrLO7BaVTs3dQTvYSV6nYSBj0Yy9DjBhsIJtAcHqRzXSq4Pg32ujHZ/AdWsAMNEvvhbm5dt9E2tOiHzw==`
- Package `LICENSE` is preserved as `mermaid-LICENSE.txt`. Keep the bundle's nested dependency copyright and license comments as well as the standalone license when redistributing; the MIT package license does not replace those notices.

SHA-256 of the unmodified vendored files (distinct from the tarball integrity above):

```text
a8a6a6f805b074a8261a2825c86f99c8054acb3b35f50bb7e24a4f9f2c3c3b9f  mermaid.min.js
ec9fb67dcb25eccc416ed56e1aab819222c805a2a4bfe4cb19e7556bf2ffde80  mermaid-LICENSE.txt
```

Application security policy and theme adaptation live outside the vendor bundle in `apps/work-ui/src/features/conversation/mermaid.{rs,js,css}`. See [conversation usage and limits](../../docs/guides/codex-interactions.md#mermaid-图表) and [review evidence](../../docs/reviews/codex-visual-review.md#后续实现离线-mermaid-图表).
