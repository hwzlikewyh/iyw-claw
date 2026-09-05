# Codex Upstream Probe

This package is an explicit compile boundary for the pinned OpenAI Codex
in-process client. It is not part of the default `iyw-codex-harness` package
and is not wired into `iyw-claw`.

Run it only when the Codex git repository is available locally or network access
is intentional:

```text
cargo check --manifest-path harness/codex/upstream-probe/Cargo.toml
```

The parent synchronization script can run the same probe after verifying the
tag and commit:

```text
powershell -File harness/codex/scripts/sync-upstream.ps1 -RunProbe
```

The parent harness remains dependency-free so normal checks cannot trigger a
Codex workspace fetch. The probe deliberately keeps upstream types private;
the eventual application adapter must expose only the protocol-neutral
contracts from the parent package.

The probe repeats the locked Codex workspace root patches because Cargo does
not propagate `[patch]` entries from a Git dependency into its consumer.
