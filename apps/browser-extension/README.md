# NexDownload browser extension

A Manifest V3 extension for Chromium-based browsers (Chrome, Edge, Brave, Vivaldi).
It connects to the authenticated NexDownload local API and can either intercept new
browser downloads or send a link through the context menu.

The extension is an optional convenience layer. NexDownload itself never depends on
it, and all download work is performed by the native NexDownload engine.

## Features

- Intercepts new `http://` and `https://` browser downloads and queues them in
  NexDownload. If the API is unavailable or rejects the request, the original browser
  download is **not** cancelled.
- Context-menu action: **Download with NexDownload** on links.
- Popup with connection status, speed, active/completed counts, recent tasks, and an
  add-URL form.
- Rule confirmation is never bypassed: if a NexDownload rule requires confirmation,
  the extension reports it and the download can be approved in the desktop app.
- Token and API URL are stored only in `chrome.storage.local` for this extension.

## Install from source

1. Build or install NexDownload and copy the API token from
   **Settings → Local API → Copy token**.
2. Open `chrome://extensions` (or `edge://extensions`) and enable **Developer mode**.
3. Choose **Load unpacked** and select `apps/browser-extension`.
4. Open the extension **Options**, enter the API URL
   (default `http://127.0.0.1:47821`) and token, then choose **Test connection** and
   **Save settings**.
5. Grant the host permission prompt if your API is not on the default localhost hosts.

The desktop app must be running for the extension to communicate with it. A headless
server can be started with `nexdownloadd --frontend apps/desktop/dist`; see the main
[development guide](../../docs/DEVELOPMENT.md).

## Permissions

| Permission | Why it is needed |
| --- | --- |
| `downloads` | Detect new browser downloads and cancel them only after NexDownload accepts the task. |
| `storage` | Store the API URL, token, and interception preferences locally. |
| `contextMenus` | Add the “Download with NexDownload” link action. |
| `notifications` | Report queued downloads and configuration/authentication problems. |
| Host permissions | Reach the local API. `127.0.0.1` and `localhost` are granted by default; other hosts are requested explicitly in Options. |

The extension does not inject content scripts, does not read page contents, and does
not send data anywhere except the API URL configured by the user.

## Development

The extension is plain JavaScript with no build step. After editing, use the reload
button on the extension card in `chrome://extensions`.

Static syntax check:

```powershell
node --check background.js
node --check popup.js
node --check options.js
```

The API CORS allow-list accepts `chrome-extension://` and `moz-extension://` origins so
the pages can call the local API. Authentication is still required.

## Packaging

Create a zip containing `manifest.json`, the HTML/CSS/JS files, and `icons/`:

```powershell
Compress-Archive -Path manifest.json,background.js,popup.html,popup.css,popup.js,options.html,options.css,options.js,icons -DestinationPath nexdownload-extension-0.1.0.zip
```

The packaged extension is not published by the project. Install it unpacked or
distribute it through a browser store after reviewing the store’s review requirements.
