# Third-party notices

NexDownload is MIT-licensed. It depends on the open-source packages below. The
authoritative license text for each package is distributed with that package and is
not reproduced here. Transitive dependency versions are pinned in `Cargo.lock` and
`apps/desktop/package-lock.json`.

The project does not bundle aria2, yt-dlp, FFmpeg, or any GPL-only component in the
default distribution.

## Rust dependencies

| Package | License |
| --- | --- |
| async-trait | MIT OR Apache-2.0 |
| axum | MIT |
| bytes | MIT |
| chrono | MIT OR Apache-2.0 |
| directories | MIT OR Apache-2.0 |
| futures-util | MIT OR Apache-2.0 |
| hex | MIT OR Apache-2.0 |
| http | MIT OR Apache-2.0 |
| percent-encoding | MIT OR Apache-2.0 |
| rand | MIT OR Apache-2.0 |
| regex | MIT OR Apache-2.0 |
| reqwest | MIT OR Apache-2.0 |
| serde / serde_json | MIT OR Apache-2.0 |
| sha2 | MIT OR Apache-2.0 |
| sqlx | MIT OR Apache-2.0 |
| thiserror | MIT OR Apache-2.0 |
| tokio / tokio-util | MIT |
| tower / tower-http | MIT |
| tracing / tracing-subscriber | MIT |
| url | MIT OR Apache-2.0 |
| uuid | Apache-2.0 OR MIT |
| tempfile | MIT OR Apache-2.0 |
| tauri / tauri-build | MIT OR Apache-2.0 |
| tauri-cli (`@tauri-apps/cli`, build-time) | Apache-2.0 OR MIT |
| WebView2Loader.dll (Microsoft WebView2 Loader) | Microsoft WebView2 SDK redistribution terms |

## Frontend dependencies

| Package | License |
| --- | --- |
| @tauri-apps/api | Apache-2.0 OR MIT |
| clsx | MIT |
| lucide-react | ISC |
| react / react-dom | MIT |
| react-router-dom | MIT |
| tailwind-merge | MIT |
| zustand | MIT |
| @playwright/test | Apache-2.0 |
| @testing-library/jest-dom | MIT |
| @testing-library/react | MIT |
| @types/react / @types/react-dom | MIT |
| @vitejs/plugin-react | MIT |
| autoprefixer | MIT |
| jsdom | MIT |
| postcss | MIT |
| tailwindcss | MIT |
| typescript | Apache-2.0 |
| vite | MIT |
| vitest | MIT |

## Build and distribution notes

- Rust dependencies are obtained through crates.io and pinned by `Cargo.lock`.
  Platform-specific Tauri dependencies (WebView2, WebKitGTK, GTK, and related system
  libraries) are not vendored by this repository and retain their own licenses.
- npm dependencies are obtained through the npm registry and pinned by
  `apps/desktop/package-lock.json`. They are build/test dependencies except for the
  runtime React packages and Tauri API.
- The application icon is an original NexDownload asset and is not copied from a
  third-party download manager.
- Windows bundles include `WebView2Loader.dll` from the Microsoft WebView2 SDK. It is
  redistributed under Microsoft's WebView2 SDK terms and is not covered by NexDownload's
  MIT license. The loader is copied from the `webview2-com-sys` crate used to build the
  desktop application.
- The browser extension is original code and uses only browser extension APIs. It has
  no runtime third-party JavaScript dependencies.
- The transitive Rust graph includes MPL-2.0 components used by webview/CSS tooling
  (`cssparser`, `cssparser-macros`, `dtoa-short`, `option-ext`, `selectors`). Their
  source and MPL-2.0 terms remain applicable. No GPL-only crate is present in the
  default graph.

If you redistribute a compiled binary, include this notice, the MIT license, and the
license texts of the dependencies included in that binary. To audit the current
dependency graph:

```powershell
cargo metadata --format-version 1
npm --prefix apps/desktop ls --all
```
