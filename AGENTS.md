# AGENTS.md

For coding agents working on Brevier, and for the people who run them. It does not
repeat [CONTRIBUTING.md](CONTRIBUTING.md) — building, the corpus, packaging and releases
are there; read it too. This file is what an agent needs on top: what must not be built,
what is already decided, and what counts as having checked a change.

Brevier is built with agents in the workshop and with no model in the product. A pull
request is welcome however it was written, as long as a person stands behind it: they
read the change, checked it the way this file asks, and say how.

## What Brevier is

A reading browser without JavaScript. It fetches a page, reduces it to Markdown on the
reader's machine and sets it in the reader's type, not the site's — plus a mode for
reading the documentation of git repositories. A core library in Rust (`brevier`), a cli
on top of it, a GTK 4 window (`brevier-ui`, the `ui` feature) and an Android app
(`android/`, Kotlin over the same core).

What it promises is in [MANIFESTO.md](MANIFESTO.md); where it is going and what has been
settled is in [ROADMAP.md](ROADMAP.md), under "Decided". Where this file and those two
disagree, they win.

## Never

These are not open questions, and a patch that does any of them will not be merged:

- **Run JavaScript** — no engine, in any form, not "only for this site".
- **Apply a site's CSS.** The reader sets the type.
- **Put a model in the reading path.** Extraction and conversion are deterministic rules in
  Rust, checked byte for byte against `corpus/expected/`. (Models in the workshop — to
  mark the corpus, to find where rules cut living text — are fine.)
- **Hold a credential**: no cookies kept, no logins, no forms. Authenticated reading is the
  companion extension's job.
- **Work around TLS.** Trust is the operating system's (`rustls-platform-verifier`);
  `danger_accept_invalid_certs` and bundled roots are out. `cargo tree | grep webpki-roots`
  stays empty.
- **Crawl.** Requests are measured by reason: what reading a page takes, never walking a
  site or fetching pages nobody asked to read.
- **Rule extraction by site.** The open web is ruled by the shape of a page. Named
  per-host rules exist only in the table in `src/hosts.rs`, each row with a test or
  corpus pages.

## Decided — do not undo

Much of what looks superfluous was chosen, with a cost. Every module opens with a comment
saying why it is the way it is: read it before simplifying. A few that agents tend to
"fix":

- **Markdown is the internal representation**, not a file format: one renderer for the web
  and for `.md` files. What Markdown cannot hold is lost on purpose.
- **The core links no toolkit.** GTK stays in `src/ui/`, Android in `src/android.rs` and
  `android/`.
- **One page model for both front-ends** (`src/page.rs`). Style names are the window's tag
  names and the Android span names; a change to how an article looks is made there, once,
  and then drawn in both.
- **The window draws with `GtkTextView`**, tables as widgets on anchors, formulas as
  `GdkPaintable` in the buffer (not widgets — scrolling stutters). **Android draws native
  text**, never a WebView.
- **Interface text is English and lives in the core** (`src/failure.rs`, `src/intro.rs`),
  so every front-end says the same thing.
- **Dependencies come without default features** when those bring weight; no `serde_json`
  in the core (`src/json.rs` exists for that).
- **The wording of ROADMAP.md and MANIFESTO.md is the maintainer's.** Correct facts there;
  do not rewrite, reorder or soften.

## What counts as checked

Say in the pull request which of these you did and what you saw:

- **Any change:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, the
  same with `--features ui`, `cargo test` (the page model's tests need
  `--features images,typeset`).
- **Extraction or conversion:** run the old and the new binary over the **same saved
  HTML** — download the corpus pages once, build the old binary from the base commit in a
  separate worktree and target directory, run both through `brevier --stdin <url> <
  page.html`, diff the outputs. A live run mixes your change with whatever the sites
  changed since. Every difference must be one you meant; update `corpus/expected/` only
  for those.
- **`--check`:** `corpus/check.sh`, offline.
- **The window or the app:** tests do not cover them. Run it — a window, an emulator, a
  phone — and say what you looked at. A screenshot helps; a description of a pixel
  measurement is better than "looks fine".
- **A new host rule:** a test against the host's real markup — a saved page in
  `tests/fixtures/`, or the parts the rule reads, condensed in the test.

## Conventions

- **English on GitHub**: commits, pull requests, issues. A commit subject is a plain
  sentence about the change ("Read royallib books part by part (#20)"); the body says why,
  and what it cost.
- **Comments in the code** are mostly Russian; write yours in the language you are most
  exact in. Match the surrounding code: its comment density, naming and idiom. A comment
  says why, not what.
- **One concern per pull request**, small enough to read in one sitting.
- **Every commit of a pull request is signed off** (`git commit -s`) under the
  [Developer Certificate of Origin](DCO) — by the person submitting it. An agent does not
  sign off on anyone's behalf: the sign-off is that person's statement.

## Pitfalls

- **Some corpus sites change between runs** — a rotating footer, a one-time token, a
  relative date, a block served every other time. Run again before looking for the cause
  in the code.
- **Corpus runs use four jobs.** Eight bring 403s, and a readable page slides into a
  refusal.
- **`tests/wire.rs` fails wholesale after the checkout moves**: the binary's path is baked
  in at compile time. `touch tests/wire.rs`.
- **GTK calls handlers at moments of its own choosing**, including from inside another
  widget's setter. A handler that borrows shared state uses `try_borrow` and gives up
  quietly when it is busy.
- **Pango's negative indent indents the continuation, not the first line** — that is how
  list items, verse and code get their hanging indent.
- **On Android, `optString` returns JSON `null` as the string "null"** — read nullable
  fields with `text()` (`Core.kt`).

## Claude Code

The maintainer keeps a private `CLAUDE.md`, which is gitignored. To give your Claude this
file, create your own `CLAUDE.md` in the checkout with the line `@AGENTS.md`.
