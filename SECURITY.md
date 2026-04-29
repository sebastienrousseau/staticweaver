# Security Policy

## Supported Versions

`staticweaver` is pre-`1.0.0`. Only the latest published release receives
security fixes.

| Version | Supported                   |
| :------ | :-------------------------- |
| `0.0.x` | :white_check_mark: (latest) |
| older   | :x:                         |

## Reporting a Vulnerability

**Please do not open a public GitHub issue for security bugs.**

Instead, use one of these channels:

- **GitHub Security Advisories (preferred).** Open a private advisory at
  <https://github.com/sebastienrousseau/staticweaver/security/advisories/new>.
  This keeps the disclosure private until a fix lands.
- **Email.** Send details to <sebastian.rousseau+security@gmail.com>.
  Include a minimal reproducer where possible.

### What to include

- A description of the issue and its impact.
- The affected version(s) (`cargo pkgid staticweaver` output is ideal).
- A reproducer — ideally a small `main.rs` snippet that triggers the
  behaviour, or a link to a branch.
- Any suggested fix or workaround.

### What to expect

- **Acknowledgement within 72 hours** that the report was received.
- **Initial triage within 7 days** — confirmation or rejection with
  reasoning.
- **Fix timeline**: high-severity issues aim for a patched release within
  14 days of confirmation; medium/low within 30 days.
- **Public advisory** published via GitHub Security Advisories when a
  fix ships, crediting the reporter (unless they prefer anonymity).

## Scope

In scope:

- Any correctness or safety issue in the `staticweaver` crate itself —
  XSS, path traversal, SSRF, denial-of-service, memory safety, supply-chain.
- Issues in the example suite that could mislead users into unsafe
  patterns.

Out of scope:

- Issues in upstream dependencies — report those to the upstream project
  first; we will track and pin a fix once available.
- Theoretical issues without a concrete reproducer.
- Denial-of-service requiring non-standard configuration (e.g. a user
  explicitly passing an unbounded URL to a custom fetcher).

## Hardening guarantees

`staticweaver` ships with the following hardening on by default:

- `#![forbid(unsafe_code)]` at the crate root.
- HTML escape for every context substitution in `render_template` and
  `render_page`. Opt-out is explicit (`{{!key}}` per tag or
  `.with_html_escape(false)` globally).
- Path validation on `render_page(layout)` — rejects `/`, `\`, `..`, and
  null bytes before any filesystem call.
- Remote template fetching is gated behind the non-default
  `remote-templates` cargo feature.
- When `remote-templates` is enabled: 10 s request timeout, 1 MiB body
  cap, `Content-Type` validation, and the `rustls-tls-native-roots`
  TLS backend (no OpenSSL pull-in).
- `cargo deny check` passes on every CI run (advisories, bans, licenses,
  sources). Yanked crates are denied.
- All release commits are GPG-signed; `Assisted-by:` trailers track AI
  tooling provenance per the Linux kernel coding-assistants convention.

See the README's `## Security` section for a summary.
