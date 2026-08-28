# Codex HUD

Codex HUD is an independently maintained Codex CLI distribution with a Claude-HUD-inspired, three-row status surface above the composer.

```text
[gpt-5.6-sol medium] | codex-hud git:(main*) | API est. this run $3.27
Context [████████░░░░░░░░░░░░] 42% | 5h 93% (58m) | Weekly 80% (16h 58m)
Tokens 1.9M (in: 50k, out: 22k, cache: 1.9M)
```

Version `0.1.0` is based on OpenAI Codex `rust-v0.150.1` at commit `90854393966b21e9ebfd21b122334eb09a20c93d`.

> [!IMPORTANT]
> This is an independent project based on OpenAI Codex, not an official OpenAI release. The built-in Codex updater is disabled so it cannot replace Codex HUD with a stock binary.

## What the HUD shows

- Effective model and reasoning effort.
- Project, asynchronously resolved Git branch, detached HEAD, and staged, unstaged, or untracked dirty state.
- Active context consumption using Codex's context calculation and reserve.
- Every available rate-limit bucket, with Codex first and deterministic responsive compaction.
- Cumulative input, output, and cached-input tokens.
- A delta-based API-equivalent cost estimate for this process run.

Wide terminals show reset times and a context bar. Medium terminals keep all rate-limit buckets but drop reset times first. Narrow terminals show the highest-used bucket and a `+N` marker for the remaining buckets. Modal and approval surfaces temporarily replace the HUD so they keep the full terminal height.

## Build and run

Requirements are the same as upstream Codex: Git, Rust via [rustup](https://rustup.rs), and the platform build tools. The repository pins Rust through `codex-rs/rust-toolchain.toml`.

```sh
git clone https://github.com/iJustinn/codex-hud.git
cd codex-hud
./scripts/build.sh
./bin/codex-hud
```

The build script uses the repository-local `.toolchain` when present, otherwise it uses Cargo from `PATH`. It produces:

```text
bin/codex-hud
bin/codex-hud.toml
bin/pricing.toml
```

The executable identifies itself as `codex-hud 0.1.0+codex.0.150.1`.

## Login and Codex compatibility

`codex-hud` intentionally uses the standard Codex home and authentication state. If the official CLI is already signed in, the fork uses that same login:

```sh
./bin/codex-hud login status
```

It does not copy, migrate, print, or rewrite credentials. The official `codex` and forked `codex-hud` executables can be used side by side. All normal Codex commands and configuration continue to come from the pinned upstream release.

## HUD configuration

HUD configuration is loaded once at startup in this order:

1. `CODEX_HUD_CONFIG` and `CODEX_HUD_PRICING` environment overrides.
2. `codex-hud.toml` and `pricing.toml` beside the resolved executable.
3. Compiled defaults.

The default configuration is:

```toml
enabled = true
context_bar_width = 20
show_cost = true
show_rate_limits = true
```

Invalid HUD configuration falls back to defaults. Invalid or unreadable pricing disables the estimate. A missing pricing file uses compiled defaults. Unknown model IDs omit cost rather than displaying a misleading zero.

The estimate prices uncached input, cached input, and output token deltas separately. Reasoning tokens are already included in Codex's output counter and are not charged twice. Tool fees, regional uplifts, and long-context multipliers cannot be inferred from the TUI counters and are excluded. Review `pricing.toml` when OpenAI pricing changes.

## Development

The focused checks are:

```sh
cd codex-rs
just test -p codex-tui hud::tests branch_summary::tests
just test -p codex-cli
just fix -p codex-tui
just fmt
```

When updating the Codex base, use an explicit upstream `rust-v*` release tag, record its commit, refresh pricing against official model pages, and rerun the TUI and CLI suites. Codex HUD releases use `vMAJOR.MINOR.PATCH`; the upstream Codex base remains visible in the executable's build metadata.

## License

This fork retains the upstream [Apache-2.0 license](LICENSE). Codex is developed by OpenAI; this repository's HUD modifications are independently maintained.
