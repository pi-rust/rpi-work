# Local QR dependencies

Bundled for offline prototype use; the page does not request a CDN at runtime.

| File | Version | Source | License |
| --- | --- | --- | --- |
| `qrcode.js` | qrcode-generator 2.0.4 | [upstream](https://github.com/kazuhikoarase/qrcode-generator), npm distribution `dist/qrcode.js` | MIT, see `qrcode-LICENSE.txt` |
| `jsQR.js` | jsqr 1.4.0 | [upstream](https://github.com/cozmo/jsQR), npm distribution `dist/jsQR.js` | Apache-2.0, see `jsQR-LICENSE.txt` |

Retrieved on 2026-10-24 from the versioned npm mirror:

- `https://cdn.jsdelivr.net/npm/qrcode-generator@2.0.4/dist/qrcode.js`
- `https://cdn.jsdelivr.net/npm/jsqr@1.4.0/dist/jsQR.js`

SHA-256 of unmodified JavaScript:

```text
79ec86f82856005b1c887905cfccfcfbec3821ca61c7fd5a952faa5f778f791c  qrcode.js
bc40c8a15196236b2314db0856f72ca0b49980cd5413b8c852a7349f5fee0859  jsQR.js
```

The QR generator includes its copyright/license header. Its license file was retrieved from the upstream repository `LICENSE`. The decoder license is included from the versioned npm distribution. Do not remove these notices when redistributing the prototype.
