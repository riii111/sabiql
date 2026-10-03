# Release validation

A local build or mock test does not establish compatibility with every database,
terminal or OS password store. Record the exact commit, platform and result for
each release candidate. Do not publish a tag just to test builds.

## Build without publishing

Run **Release Build Validation** manually on the release candidate before tagging.
The Release workflow also runs the same validation before publishing; everyday
PR/main CI does not build the distribution matrix. Validation checks Android
compilation and builds all five distributed binaries:
macOS Intel/Apple Silicon, Linux x86_64/ARM64 and Windows x86_64. It uploads build
artifacts but does not create a GitHub Release or publish crates. Also run a
native FreeBSD build with system dbus/pkg-config and `nix build` before claiming
those environments are supported.

The Release workflow accepts only a stable tag matching `[workspace.package].version`.
Prerelease or mismatched tags fail before publication. Choose the next unpublished
version explicitly at release time; the Nix package derives its version from Cargo.

## Acceptance checks

Use dedicated test credentials, databases and configuration directories.

- PostgreSQL: multi-statement rollback, explicit COMMIT followed by an error,
  connection refusal/capacity/timeout, and interruption after a write. Confirm
  drafts survive definitive failures while uncertain writes still trigger refresh.
- PostgreSQL URI credentials: encoded delimiters, IPv6, multiple hosts and socket
  paths; explicit passwords must win over service/environment/passfile defaults.
- MySQL: supported server/CLI versions, TLS verification, unknown URI option
  rejection, metadata, writes and authentication through the intended proxy.
- SQLite: safe-mode minimum version, reads/writes and CSV export into an isolated
  destination. Permission failures must not be reported as successful exports.
- macOS: upgrade from an older binary and inspect Keychain permission prompts;
  test allow, deny and a locked store. Old-credential cleanup failure must preserve
  committed configuration and any replacement credential; test both no mutation
  and completed deletion with a lost response. Verify the cleanup warning.
- Windows: Credential Manager save/read/delete, denial and failure recovery.
- Linux: unlocked, locked and unavailable Secret Service. Verify bounded startup,
  responsive settings and prompt-independent process exit.
- Clipboard: Auto and explicit choices, settings save/cancel/failure, local native
  copying, SSH and supported multiplexer/terminal receivers. Test complete values
  at the size limit and refusal above it. A private PTY test proves sender output
  preservation, not clipboard acceptance. Preserve any existing clipboard formats
  or use a dedicated test session.
- Settings: v2/v3 compatibility, v4 only when saving password references, backup and
  rollback guidance, and a missing credential when editing unrelated fields.

Cloud/IAM/proxy and latency claims require their own acceptance evidence. A local
protocol fixture does not prove managed-database or cloud-shell compatibility.
