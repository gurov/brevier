# Roadmap

What is planned, in two horizons, and what has been decided. Nothing here has a
date. Inside each list the order is roughly the order of attack. Every item in
*Near* is also an open issue labelled `roadmap`, where the work is tracked: this
file is the plan, the issues are the progress.

The working log — gates, measurements, what irritated the maintainer this week —
is kept outside the repository. What reaches this file is the decision and its
price, not the diary.

## Near — the next releases

### 1. Into the stores (#1)

Three doors of different price, in this order.

**Flathub.** The manifest lints clean and the id is `io.github.gurov.brevier`; what
is left is a checklist, not design: run the window on Wayland at least once (#12 —
every test so far ran on X11, and Fedora, GNOME and Ubuntu default to Wayland), move
the runtime to GNOME 50, turn on 2FA, fork `flathub/flathub` and open the PR. Review
is done by volunteers and has no deadline, which is why this goes first: the waiting
runs in parallel with everything below. After that Brevier is one search away
instead of one wget away — and updates arrive with the store, which the bundle in
Releases cannot do.

**crates.io.** `cargo install brevier --features ui` — the cheapest door and the
right one for the present audience. Needs `description`, `repository` and
`keywords` in `Cargo.toml`, and an `exclude` for the corpus and packaging
directories, which would otherwise travel with the crate.

**The cli as one file per tag**, for Linux, macOS and Windows: `brevier` links
neither GTK nor OpenSSL, so it really is one file. Linux built against musl,
otherwise the binary demands a glibc no older than the build machine's.

Published on tags, not on pushes: CI already builds every push, but handing out
every state means handing out half-finished work.

### 2. Native Markdown (#2)

Brevier already reads `text/markdown` without extraction, and already lists it in
`Accept` — at `q=0.9`, behind HTML. What is left:

- Prefer it: `Accept: text/markdown, text/html;q=0.9` on every fetch. When the
  answer is `text/markdown`, skip extraction and render as in repository mode; the
  status line says "Served as Markdown by the site". A corpus run comes first —
  content negotiation is where servers misbehave.
- Follow `<link rel="alternate" type="text/markdown">` when the first answer was
  HTML. Two requests for one page — still one page for one human, and the README
  says so rather than leaving it to a server log.
- `/llms.txt`, where a site has one, as another entry point on the shelf in `--docs`
  mode: whatever it was written for, it is an index of a site's text in Markdown.

Before the check, because one of the check's findings is whether a site answers
`Accept: text/markdown` — and the reader should benefit from a yes before we ask
sites for it.

### 3. The check (#3)

A developer should be able to point Brevier at their own site and be told what stands
between it and a clean read. `brevier --check <url>` — and `--check --stdin <url>` for
HTML that is not deployed yet — runs the ordinary pipeline, watches what happens at
every stage, and prints a report. In Markdown, naturally: it reads in the window as
`brevier:check`, and it diffs in a pull request.

The report has three parts.

**A score**, 0 to 100. Every finding costs a stated number of points, and the report
prints the arithmetic — nothing lives in a hidden formula. A few findings cap the
score instead of deducting from it: text that exists only after a script has run
caps it at 10, a 403 to an honest User-Agent is 0. A page that cannot be read does
not "read at 70" because its headings were in order. The exit code follows the score
against a threshold — 80 by default, `--min` to change it — so the check can sit in
CI next to the linter.

**Findings**, each with what was seen, what it cost, and what to change, grouped by
the stage that produced them:

- *Access* — refused an honest User-Agent (403/401: allow a client that runs no
  scripts); a certificate the operating system does not trust; a redirect chain of
  more than three hops; a content type that is not text.
- *Text* — how much of the page's text the extractor kept and how much it threw
  away (the noise ratio); whether the body is empty until a script runs (a
  `<noscript>` apology, `data-src` without `src`, text inside `<template>`); words
  that exist only as images.
- *Structure* — exactly one `h1`; heading levels in order, no skips; `<article>` or
  `<main>` present; paragraphs as `<p>` rather than `<div>` or `<br><br>`; code as
  `<pre><code class="language-…">`; tables as `<table>`; figures with captions;
  `lang` on `<html>`; a `<title>` that agrees with the `h1`; a byline and a date
  the extractor can find.
- *Extras* — `alt` on images; a feed advertised in `<head>`;
  `<link rel="alternate" type="text/markdown">`; whether the site answers
  `Accept: text/markdown`.

**What the reader will see** — the extracted Markdown, or its first screen — so the
finding "the byline was lost" comes with the evidence next to it.

Rules for the check itself. Every finding is deterministic and computed from the one
fetched response — no second fetch with a headless browser, no external service. The
advice is one sentence and names the element. The weights are in one table in the
source and repeated in every report, so a score can be argued with, line by line.

Work items:

- a `Check` stage that records events from fetch, extraction and conversion — the
  exit codes already tell the failures apart; the check turns them into sentences
- the structural checks, run on the fetched DOM and on the extracted one
- the weight table, the caps, and `--min`; the check's exit code is pass or fail
  against it, and does not reuse the reading codes 1–7
- the report template and `brevier:check`
- `corpus/check/`: one page per failing check, with its expected report, so the
  check is regression-tested like everything else
- a GitHub Action that runs the check on a list of addresses and fails the build
  below the threshold; a badge that shows the score

### 4. The type is the reader's (#4)

The first line of the manifesto, and today only half true: measure, leading, size
and face are Brevier's constants, and the reader's knobs are zoom and the theme.

A page in Settings for size, measure (kept in ems, so the line holds its length in
characters when the size changes), leading, and face — saved in `settings.tsv` next
to the theme, chosen once, for every site. Zoom stays what it is: a per-run, per-host
adjustment for the material, not for the type.

The face is the expensive part. Brevier ships Noto Sans so that the promise holds on
a machine with no fonts; a chosen face comes from the system through fontconfig, and
the shelf, the heading scale and the code panel must hold together under a face they
were not tuned for. Presets first, an arbitrary family after.

### 5. The companion extension (#5)

A WebExtension for Firefox and Chrome with one menu item, "Read in Brevier". It
takes the page as the browser finally rendered it and hands it over native messaging
to `brevier-ui --stdin <url>`, address included, for the links. JavaScript runs in
the browser that already has it; Brevier still has no engine.

The one menu item answers three things at once: SPA sites; pages behind a login —
the browser is logged in, and Brevier never holds a cookie; and how people find
Brevier in the first place — from the browser they already use, one page at a time,
until the reading moves.

Work items:

- the extension itself, Manifest V3, submitted to both stores
- the native-messaging host: a small binary, its manifest installed by `install.sh`
  and, for the Flatpak, a wrapper script on the host that calls `flatpak run` — the
  browser wants an executable outside the sandbox, and a Flatpak'd Firefox
  complicates it further; a cost to measure before promising
- the same path from the command line, so `--stdin` and the extension are one code
  path with two front doors

### 6. Typesetting by language (#6)

Typesetting, not editing: everything here happens in the window, and the saved
Markdown stays byte-identical.

- Hyphenation, with patterns bundled for a handful of languages and chosen by
  `<html lang>` (and per-element `lang` where present). Without `lang`, no
  hyphenation — and the check says so.
- The rules that keep a line from ending on a one-letter preposition or
  conjunction, which Czech and Russian typography both require and no reader mode
  does.
- Justified text as a setting, once hyphenation exists to make it bearable.

Known cost, to be paid before this ships: Pango breaks lines only where the text
allows, so hyphenation means soft hyphens and non-breaking spaces inserted into the
buffer — and the buffer is what copy, find-on-page and the session's reading offsets
see. Copy must strip them, find must skip them, offsets must not move. If that
cannot be done cleanly, the feature waits.

### 7. Feeds (#7)

- An RSS or Atom address opens as the "list of links" view, with dates.
- A site that advertises a feed gets it on the shelf: "This site has a feed."
- Subscriptions come later (see *Far*); this is only reading a feed when handed one.

### 8. The archive (#8)

- On by default. Every page read is kept under
  `$XDG_DATA_HOME/brevier/archive/<host>/<date>-<slug>.md.lz4`, one file per page,
  and the history line points to its copy. LZ4 in pure Rust (`lz4_flex`), for the
  same reason there is no SQLite; fast enough that the write is not felt and a search
  over the whole archive is a scan at memory speed.
- The frame is the standard one, so `lz4 -d` opens a file without Brevier;
  `brevier brevier:archive/…` prints it plain; `--save` still writes `.md`. The file
  is yours, and the only thing between you and it is a decompressor you already have.
- `brevier:archive`, and full-text search across it (`Ctrl+Shift+F`) — files, no
  database; a reading life is thousands of pages, not millions.
- Bookmarks keep their copy for good; the rest ages out with history. A setting turns
  the archive off, and "forget everything" empties it together with the journal.
- Data, not cache: it lives under the data directory, the one the reader carries
  with them, and only the window writes there — `brevier <url> | less` is a pipe
  tool.

### 9. When the page is gone (#9)

- 404, or a host that does not answer: the status line offers "Read the Wayback
  copy" — a button, never automatic; one page per request from a human.
- If the archive has the page: "You read this on ‹date›; open your copy."

### 10. Tables in the buffer (#10)

The known limitation: a table is a grid of widgets, so find-on-page and "copy
everything" do not see it. Either render tables as text in the buffer, or teach the
find and the copy to descend into the widgets. The saved Markdown already has the
table in full; the window should not know less than the file.

### 11. Threads, and the boundary of per-host rules (#11)

Two things keep a thread from reading as a thread, and neither is fixable by a rule
about form: Readability strips the author and time of every comment as chrome (a
Hacker News thread: no authors out of ninety-one), and drops a short reply that is
mostly a link by the link-density rule. That threshold is the heart of extraction
and the corpus has it pinned; moving it for threads breaks articles.

So threads need their own path past Readability, the way the repository mode has
one — and that is a rule about a host, which this project has so far refused. The
boundary is stated under *Decided* rather than crossed quietly: a per-host entry is
a small selector table, not code; it exists only for a genre form cannot tell apart —
threads, today; each entry has pages in the corpus and a rubric paragraph written
before it is measured; and when it fails, the page falls back to the generic path,
so the worst case is what the reader gets today.

Candidates, by how much text is behind them: Hacker News, Lobsters, Discourse
forums, old.reddit through its `.rss` (the only door still open), public Mastodon
pages.

Documentation hosts — MDN, docs.rs, Read the Docs, GitBook, Mintlify — are not on
this list until they are in the corpus. Most of what is wrong with them is probably
a form rule (a sidebar is a `nav`; a version switcher is a list of links), and the
measurement decides.

## Far — after that

### Send to

Lines in `settings.tsv` — `send ‹name› ‹command›` — and a "Send to…" menu in the
window. The command gets the article's Markdown on stdin and does what it likes:
append to a notes directory, print, run through `pandoc`. Anything that reads stdin.

### Subscriptions

Feeds on the shelf, `brevier:feeds`, unread marked. Refreshed when the reader asks —
no background polling; that would be crawling with a schedule.

### A readable-web note

A short document, separate from this repository, saying what a page should serve so
that *any* reading client reads it well — the list from the manifesto, written down
with the check as its reference test and the alternate-Markdown link as its centre.
Written after the check has run on enough sites to know which findings matter.

### macOS and Windows

Builds for both, once Linux is in a store. GTK 4 runs on both and the fonts ship
inside the binary, so the typography holds. What does not hold is accessibility: GTK
speaks AT-SPI, and VoiceOver and NVDA do not listen. The builds ship with that stated
in the README, the way the Linux-only note is stated today. Whether the gap is later
closed with a second front-end is a question for when there are readers on those
platforms to ask.

With them comes the extra-root setting (#18): a certificate the reader adds, valid
only for the hosts they list next to it. On Linux "install the root in the system"
is the answer; on the other two it stops being an obvious instruction. The design is
settled — the platform verifier first, the extra roots second, and never a way to
skip verification.

### Snap

For Ubuntu, where it is preinstalled: the `gnome` extension brings GTK 4, and
Launchpad builds on every push into the `edge` channel without a CI of our own.
After Flathub, if there are Ubuntu readers asking. A distribution archive is not on
the list: Debian wants every Rust dependency as its own package, and an LTS freezes
a version for two years while the questions about it come here.

### Print

`Ctrl+P`, in the reader's own type, through GTK's print dialog. A PDF falls out of
it for free.

### Gemini

`gemini://`; gemtext is nearly a subset of Markdown. The protocol's trust model —
trust the certificate on first use — is not the operating system's, which is a
decision before it is a feature.

### Other documents

EPUB is HTML in a zip and would arrive through the same pipeline; plain text is
trivial. Neither is a browser's job, and both are a reader's. Maybe.

### Proving you are a reader

Anti-bot walls will grow, and an honest client without scripts looks like a crawler
to them. Watch the work on signed requests (Web Bot Auth and its relatives) and adopt
whatever sites come to honour — a way to say "one person, one page" that a wall can
verify.

## Decided

Recorded so that nobody rediscovers them in six months.

- **Authenticated reading** goes through the companion extension. Brevier never
  holds a cookie or any other credential.
- **Platforms:** Linux first and in a store; macOS and Windows builds after, with the
  accessibility gap stated rather than hidden.
- **The archive** is on by default and compressed.
- **The check** gives a number, 0 to 100, with the arithmetic printed.
- **The alternate-Markdown link** is followed. Two requests for one page are still
  one page for one human.
- **Type is a setting; zoom is not.** Face, size, measure and leading will persist
  in `settings.tsv`; the zoom step lives per host, for the run only. Typography is
  the reader's once; the material differs by site every day.
- **Per-host rules have a boundary.** "By form, not by site" stands for articles,
  and site-rule databases stay out — for the licence and for the maintenance. A
  per-host entry is allowed only for a genre form cannot tell apart — threads — and
  only as a selector table with corpus pages behind each row and a fallback to the
  generic path.
- **Repository mode probes, and says so.** Opening a repository sends about a dozen
  requests to the hosting's CDN for well-known documentation paths — the one place
  Brevier sends more than a couple of requests for one page, against a CDN with no
  limit and never against the API. Named here because the manifesto promises one
  page per request, and the exception belongs on the record.
