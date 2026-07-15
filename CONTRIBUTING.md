# Contributing to Fonos

Issues and pull requests are welcome.

## Dev setup

Prerequisites: [Rust](https://rustup.rs) stable + the Tauri CLI
(`cargo install tauri-cli --version "^2"`), and [Node.js](https://nodejs.org)
20+. macOS also needs Xcode Command Line Tools; Linux needs the packages
listed in [`.github/workflows/build-linux.yml`](.github/workflows/build-linux.yml).

```bash
git clone https://github.com/ethannortharc/fonos.git
cd fonos/fonos-desktop
npm install
npm run tauri dev
```

## Repo layout

`fonos-core` is the platform-independent Rust engine (recipes, providers,
vocabulary, storage, statistics). `fonos-desktop` is the Tauri app (Rust
backend + React/TypeScript UI) that adapts it to the desktop. See
[`ARCHITECTURE.md`](ARCHITECTURE.md) for the full map, and
[`fonos-core/README.md`](fonos-core/README.md) for the core's module-by-module
guide.

## Tests

```bash
cd fonos-desktop && npm test              # vitest (frontend unit tests)
cargo test --workspace --features ci      # Rust, both crates; ci skips hardware-dependent tests
cd fonos-desktop && npm run build         # tsc -b, the type-check gate
```

Some desktop tests need Microphone, Accessibility, or Screen Recording
permissions and are skipped under `--features ci`.

## Pull requests

- Use [conventional commits](https://www.conventionalcommits.org/) with a
  scope, matching the existing history — e.g. `feat(onboarding): ...`,
  `fix(engine): ...`, `fix(linux): ...`, `chore: ...`. Run `git log --oneline`
  for more examples.
- Keep PRs scoped to one change; match the surrounding code style.
- **i18n:** any user-facing string must be added to both the `en` and `zh`
  dictionaries in
  [`fonos-desktop/src/lib/i18n.tsx`](fonos-desktop/src/lib/i18n.tsx). English
  is the source of truth (`TKey = keyof typeof en`); missing `zh` keys fall
  back to English but should still be filled in.
- macOS release signing and notarization are maintainer-only — you don't need
  a signing identity to build or test locally (`cargo tauri build` produces an
  unsigned local build).

## License

By contributing, you agree your contributions are licensed under the
project's [MIT License](LICENSE).
