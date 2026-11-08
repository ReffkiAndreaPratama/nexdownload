# Security policy

## Supported versions

| Version | Supported |
| --- | --- |
| 0.1.x | Yes, during Phase 1 |
| Older | No |

## Reporting a vulnerability

Please report security issues privately through GitHub’s **Security advisories** for
`ReffkiAndreaPratama/nexdownload`:

```text
https://github.com/ReffkiAndreaPratama/nexdownload/security/advisories/new
```

Do not open a public issue, discussion, or pull request containing an exploit, a live
token, private URLs, or personal paths. Include:

- affected version and commit;
- operating system and build type;
- a minimal reproduction or proof of concept;
- impact and required attacker position;
- suggested mitigation, if known.

Maintainers will acknowledge the report, investigate, and coordinate disclosure. Please
allow reasonable time for a fix before publishing details.

## Threat model

NexDownload is a local desktop application and a localhost API. It assumes the local
user account is trusted. It is not a multi-user service and does not provide account
isolation, per-user permissions, or remote identity management.

The API is designed to resist a process or web page on the same machine trying to use it
without authorization:

- The API binds to `127.0.0.1` by default.
- Protected endpoints require a bearer token.
- Token comparison is constant-time.
- Requests are rate-limited and audited.
- CORS is restricted to localhost, Tauri, and browser-extension origins.
- The token is stored in a separate file with `0600` permissions on Unix.
- Engine errors and diagnostics avoid exposing credentials; URL/token redaction helpers
  are provided for logs.

## Operational guidance

- Keep the API bound to loopback whenever possible.
- Do not share the token in screenshots, logs, issues, or chat.
- Treat `NEXDOWNLOAD_TRUST_PROXY=1` as a declaration that a trusted reverse proxy
  terminates TLS. It does not provide encryption by itself.
- If exposing the API to a LAN, use a TLS reverse proxy, restrict network access, and
  rotate the token if it may have leaked.
- Run NexDownload under a non-administrator account and keep the OS, Rust toolchain,
  Node/npm dependencies, and browser up to date.
- Review the download destination and any rule that sets `require_confirmation` to
  `false`. Rules can redirect files; use them deliberately.
- The browser extension can read the token from its own storage. Install only the
  extension you built or reviewed, and do not grant broad host permissions you do not
  need.
- Do not run untrusted code as a NexDownload plugin or downloaded file.

## Known limitations

- The application does not implement TLS itself. A reverse proxy is required for
  non-loopback HTTPS.
- The API token is a long-lived shared secret; there is no per-device revocation UI in
  Phase 1. Rotate `api-token` and restart the service if needed.
- The headless server prints the token to its terminal at startup. Protect terminal
  scrollback and logs.
- The desktop shell can open or reveal files chosen by the authenticated user or local
  UI. It validates absolute paths but is not a sandbox for untrusted paths.
- The default distribution does not bundle anti-malware scanning. Verify files with
  trusted tools before opening them.
- Downloaded content is untrusted input. Do not execute it merely because NexDownload
  reports a matching checksum; a checksum proves identity, not safety.

## Dependency and supply-chain policy

- `Cargo.lock` and `apps/desktop/package-lock.json` are committed.
- Dependencies are reviewed for license compatibility with MIT distribution.
- GPL-only components are not part of the default distribution.
- The optional browser extension is plain JavaScript with no runtime third-party
  package dependency.
- Security-relevant updates should include a changelog entry and tests where practical.

## Disclosure timeline

The project aims to:

1. acknowledge a report within 7 days;
2. provide an initial assessment within 14 days;
3. ship a fix or mitigation as soon as practical depending on severity and complexity;
4. credit the reporter if they wish and if disclosure is coordinated.

These are targets, not a contractual guarantee.
