# Contributing to Brevier

Patches and bug reports are welcome. This file is for working on the code; for using
Brevier, see the [README](README.md).

Working with a coding agent? Give it [AGENTS.md](AGENTS.md) as well.

What is planned is in the [roadmap](ROADMAP.md) and the issues labelled `roadmap`; smaller
items carry `bug` or `enhancement`. What the program promises is in the
[manifesto](MANIFESTO.md), and those promises decide more than code quality does: a patch
that adds a JavaScript engine, applies a site's CSS, puts a model in the reading path or
makes Brevier hold a credential will not be merged, however well it is written.

## Languages

Everything on GitHub — issues, pull requests, commit messages — is in English. The
interface is in English too, and its texts live in the core (`src/failure.rs`,
`src/intro.rs`), so that every front-end says the same thing. Comments in the code are
mostly in Russian; write yours in whatever language you are most exact in.

## How the code is laid out

- **`src/`** — the core, the library `brevier`: fetching, extraction, conversion to
  Markdown, feeds, the repository mode, storage, and the page model. It links no toolkit,
  and that is a decision rather than an accident: the core has to cross to every front-end
  unchanged.
- **`src/page.rs`** — the page as both front-ends draw it: text, styled runs named like the
  window's tags (`body`, `h2`, `quote1`, `codeblock`…), links, anchors, the contents, and
  the places of images and tables. A change to how an article looks is made here, once.
- **`src/main.rs`** — `brevier`, the cli. The corpus runs on it.
- **`src/ui/`** — `brevier-ui`, the window, on GTK 4 (the `ui` feature).
- **`android/`** — the Android app: Kotlin over the core, through one JNI entry point
  (`src/android.rs`) that answers in JSON. No androidx.
- **`src/hosts.rs`** — the per-host rules, in one small table: reddit read through its
  feed, the search engine's results page, and royallib's reader, whose book comes part by
  part. Everything else is ruled by the shape of a page, never by its site. The results
  page is tested against a saved copy in `tests/fixtures/`; when DuckDuckGo changes its
  markup, that test fails instead of the reader.
- **`corpus/`** — the regression corpus, its expected outputs and the scripts that measure.
- **`packaging/`** — the Flatpak manifest, the offline crate list, the tarball, the desktop
  entry, the metainfo and the release notes.

Each module opens with a comment on why it is the way it is. Much of what looks
superfluous has been decided, with its cost; read that comment before simplifying.

## Building

### The cli and the window

Rust 1.88 or newer (edition 2024 needs 1.85; the image decoder's `slice::as_chunks` needs
1.88). The window needs GTK 4.10 or newer with its development files.

```sh
cargo build --release                  # brevier — the cli
cargo build --release --features ui    # brevier-ui — the window, on GTK 4
sudo apt install libgtk-4-dev build-essential   # Debian/Ubuntu, for the ui feature
```

`ui` pulls in `save`, `typeset` and `images` — image decoding, saving and hyphenation. The
cli builds without any of them, and a build without `save` says so when asked to `--save`.

To use a source build as a desktop application, install it with its desktop entry and icon
for your user:

```sh
cargo install --path . --features ui
install -Dm644 packaging/io.github.gurov.brevier.desktop \
        ~/.local/share/applications/io.github.gurov.brevier.desktop
install -Dm644 assets/brevier.svg \
        ~/.local/share/icons/hicolor/scalable/apps/io.github.gurov.brevier.svg
update-desktop-database ~/.local/share/applications
```

The icon ships inside the binary, so on X11 the window wears it with nothing installed; a
launcher menu and a Wayland compositor take it from the desktop entry and the app id
instead.

### The Android app

JDK 21, the Android SDK (platform 35) and the NDK, plus two Rust targets:

```sh
rustup target add aarch64-linux-android x86_64-linux-android
export JAVA_HOME=… ANDROID_HOME=…          # ANDROID_NDK_HOME if the NDK is elsewhere
cd android && ./gradlew assembleDebug                # arm64 and x86_64
cd android && ./gradlew assembleDebug -Pabis=x86_64  # the emulator only, faster
```

Gradle builds the core for each ABI itself, through `android/core.sh`, and the APK lands in
`android/app/build/outputs/apk/debug/` as `brevier-<version>[-<abi>]-debug.apk`. A debug
build is signed with the debug key and does not install over a release: uninstall first.
Release builds are signed by CI.

## Before a pull request

- **Every commit is signed off.** `git commit -s` adds a line `Signed-off-by: Your Name
  <you@example.com>` with the name and address the commit is made under. By it you state,
  under the [Developer Certificate of Origin](DCO), that you wrote the change or otherwise
  have the right to submit it under the project's license. CI checks every commit of a pull
  request; a forgotten sign-off is added with `git rebase --signoff main` and a force-push.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, the same with
  `--features ui`, and `cargo test`. CI runs all of them, and builds the core and the app
  for Android on every push.
- **A change to extraction or conversion** needs a corpus run diffed against
  `corpus/expected/` before it is committed — those files exist to catch regressions, and
  they have. Some sites change on their own between runs (a rotating footer, a one-time
  token, a relative date); run again before looking for the cause in the code.
- **A change to `--check`** runs `corpus/check.sh`: one page per group of findings, with
  its expected report, no network needed.
- **A change to the window or the app** is not covered by the tests. Say in the pull
  request how you checked it — on a running window, an emulator or a phone.
- **A new dependency** comes without default features when those bring weight it does not
  need, and `packaging/cargo-sources.json` is regenerated with
  `packaging/cargo-sources.py`; CI compares them. `cargo tree | grep webpki-roots` must stay
  empty: trust in certificates stays with the operating system.

## The corpus

The corpus is a list of live addresses (`corpus/urls.txt`) with the Markdown each one is
expected to give (`corpus/expected/`). What counts as readable is written down in
`corpus/RUBRIC.md` before a run, and the verdicts are marked by hand.

```sh
corpus/verify.sh                  # is each address still the page it was?
corpus/run.sh --ua honest         # a run: corpus/out/, a report and verdicts to mark
corpus/score.sh --ua honest       # the number at the gate
corpus/check.sh                   # the --check regression, offline
corpus/m2.sh                      # how much documentation is reachable from a README
```

Four jobs is the default; eight bring 403s from some hosts, and a page marked readable
slides into a refusal. `corpus/out/` is not in git — only the expected outputs are.

## Packaging and releases

```sh
flatpak run org.flatpak.Builder --force-clean --repo=packaging/repo \
    packaging/build packaging/io.github.gurov.brevier.yml
flatpak build-bundle --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo \
    packaging/repo brevier.flatpak io.github.gurov.brevier
packaging/tarball.sh              # the tarball and its install.sh
```

The Flatpak builds offline, the way Flathub builds: every crate is declared with its
checksum in `packaging/cargo-sources.json`.

The mark is `assets/brevier.svg`, and every raster icon is drawn from it: after changing
the mark, run `packaging/icons.py` (Inkscape and Pillow), which redraws the Android
launcher icons at every density and the store icon in `fastlane/`. It also refuses a mark
that is not an outline, not square, or whose gradient is not flat — what saving from
Inkscape tends to bring back; CI runs the same check with `--check`.

Releases are built by CI from a tag (`.github/workflows/release.yml`), not on a laptop. A
release `vX.Y.Z` needs the version in `Cargo.toml`, a `<release>` entry in the metainfo,
notes in `packaging/notes/vX.Y.Z.md` and the version in the README's download links; CI
checks the tag against `Cargo.toml`, builds the tarball, the Flatpak and the signed APK, and
publishes them with `SHA256SUMS`. A published tag is never moved: something newer is a new
version.

## License

Contributions are licensed as the project is: MIT OR Apache-2.0, at the user's option. The
sign-off on each commit (see above) records where the contribution came from; no separate
agreement is asked for. The bundled fonts are under the SIL Open Font License, and `assets/fonts/OFL-NotoSans.txt` must
travel with any distribution.
