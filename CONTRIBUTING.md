# Contributing

Thanks for helping. Bug fixes, translations and support for changes in the
Claude or ChatGPT usage responses are especially welcome.

## Before you start

- For anything bigger than a small fix, open an issue first so we can agree on
  the approach.
- Found a security problem? Don't open an issue: see [SECURITY.md](SECURITY.md).

## Set up

Needs Node 20+ and Rust (`rustup`). On Windows also the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```sh
npm install
npm run tauri dev                      # the app with hot reload
cd src-tauri && cargo run -- --print   # just the readers, as JSON
```

The README's code map says where things live.

## Before you open a pull request

Run the same checks as CI:

```sh
npx tsc --noEmit
npm run build
cd src-tauri
cargo clippy --all-targets -- -D warnings
cargo test
```

And please:

- Keep changes focused; one topic per pull request.
- Add or update a unit test when you change how a response is parsed
  (`claude.rs`, `codex.rs`). Use made-up numbers, never a real response with
  your account details in it.
- New text in the window goes in both `en` and `nl` in `src/i18n.ts`
  (notifications and the tray menu: `src-tauri/src/i18n.rs`). If you don't
  speak Dutch, leave the English text in `nl` and say so in the PR.
- Don't add network calls, telemetry or new places where logins are stored.
  The app only reads the existing Claude Code and Codex logins and only talks
  to the two usage endpoints.
- New dependencies need a reason in the PR description.

## How pull requests are handled

CI runs on macOS and Windows. For a first-time contributor a maintainer
starts it after a quick look at the change. Pull requests are squash-merged.
Releases are made by the maintainer; your change ships with the next one.

By contributing you agree that your work is published under the
[MIT license](LICENSE).
