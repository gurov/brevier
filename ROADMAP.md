# Roadmap

What is planned, in two horizons, and what has been decided. Nothing here has a
date. Inside each list the order is roughly the order of attack. Every item in
*Near* is also an issue labelled `roadmap`, where the work is tracked: this file is
the plan, the issues are the progress. An item whose issue is closed says "Done"
and describes what was built.

## Near — the next releases

### 1. The check (#3)

Done. A developer can point Brevier at their own site and be told what stands
between it and a clean read. `brevier --check <url>` — and `--check --stdin <url>` for
HTML that is not deployed yet — runs the ordinary pipeline, watches what happens at
every stage, and prints a report. In Markdown, naturally: it reads in the window and
on the phone as `brevier:check`, from "Check this page" in the menu, and it diffs in
a pull request.

The report has three parts.

**A score**, 0 to 100. Every finding costs a stated number of points, and the report
prints the arithmetic — nothing lives in a hidden formula. A few findings cap the
score instead of deducting from it: text that exists only after a script has run
caps it at 10, a 403 to an honest User-Agent is 0. A page that cannot be read does
not "read at 70" because its headings were in order. The exit code follows the score
against a threshold — 80 by default, `--min` to change it — so the check can sit in
CI next to the linter. Several addresses exit by the worst of them.

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

What was built:

- a `Check` stage that records events from fetch, extraction and conversion — the
  exit codes tell the failures apart; the check turns them into sentences
- the structural checks, run on the fetched DOM and on the extracted one
- the weight table, the caps, and `--min`
- the report template and `brevier:check`
- `corpus/check/`: pages with their expected reports, so the check is
  regression-tested like everything else
- a GitHub Action that runs the check on a list of addresses, writes the report to
  the job summary and fails the build below the threshold; a badge (`--badge`) that
  shows the lowest score

Still loose: the threshold of the noise ratio has not been calibrated on the corpus.

### 2. Native Markdown (#2)

Done.

- Every fetch sends `Accept: text/markdown, text/html;q=0.9` — Markdown first, where
  the header used to list it behind HTML. When the answer is `text/markdown`,
  extraction is skipped and the page renders as in repository mode; the status line
  says "Served as Markdown by the site".
- `<link rel="alternate" type="text/markdown">` is followed when the first answer was
  HTML. Two requests instead of one, for a page that then needs no extraction — a
  good trade. The address stays the page's own.
- `llms.txt`, where a repository has one at its root or in `docs/`, is another entry
  point on the shelf in `--docs` mode: whatever it was written for, it is an index of
  a site's text in Markdown.

### 3. Typesetting by language (#6)

Typesetting, not editing: everything here happens in the window, and the saved
Markdown stays byte-identical. Done, but for one thing.

- Hyphenation, with patterns bundled for a handful of languages and chosen by
  `<html lang>` — or, when `<html>` has none, by the `lang` of the container that
  holds the article. Without `lang`, no hyphenation — and the check says so. Still
  to come: a paragraph's own `lang`, which Markdown does not carry.
- The rules that keep a line from ending on a one-letter preposition or
  conjunction, which Czech and Russian typography both require and no reader mode
  does.
- Justified text as a setting, for paragraphs; headings, lists, code and verse keep
  a ragged edge.

The known cost was paid: Pango breaks lines only where the text allows, so
hyphenation means soft hyphens and non-breaking spaces inserted into the buffer —
and the buffer is what copy, find-on-page and the session's reading offsets see.
Copy strips them, find skips them, and offsets stay put, because the text in the
buffer is the same on every redraw.

### 4. The type is the reader's (#4)

The first line of the manifesto, and now true: measure, leading, size and face are
the reader's, next to zoom and the theme.

A page in Settings for size, measure (kept in ems, so the line holds its length in
characters when the size changes), leading, and face — saved in `settings.tsv` next
to the theme, chosen once, for every site. Zoom stays what it is: one step for the
whole window, for the run only — an adjustment on top of the type, not part of it.

The face is the expensive part. Brevier ships Noto Sans so that the promise holds on
a machine with no fonts; a chosen face comes from the system through fontconfig, and
the shelf, the heading scale and the code panel must hold together under a face they
were not tuned for. Presets came first — the system's serif and sans; an arbitrary
family is still to come.

### 5. Feeds (#7)

Done.

- An RSS, Atom or JSON Feed address — or a `feed://` link — opens as the "list of
  links" view, with dates.
- A site that advertises a feed gets it on the shelf: "This site has a feed."
- Subscriptions come later (see *Far*); this is only reading a feed when handed one.

### 6. The archive (#8)

Done.

- On by default. Every article read is kept under
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
  the archive off, and "forget everything" empties it together with the journal,
  bookmarks' copies aside.
- To the reader it is not a third list next to the history and the bookmarks: a copy
  is "your copy" in both, and the search is "Search what you've read" in the menu.

### 7. When the page is gone (#9)

Done, on the page that explains the failure rather than in the status line.

- 404 or 410, a server that fails, or a host that does not answer: a button offers
  "Read the Wayback copy" — never automatic: a copy is not the page, and swapping one
  for the other without asking would be pretending. Not on a 403: there the copy
  would be a way around the site's decision.
- If the archive has the page, whatever the failure: "You read this page on ‹date›,
  and your archive keeps a copy", and a button, "Open your copy".

### 8. Threads and documentation hosts (#11)

Done, but for Mastodon. Per-host rules, kept in one small table in `src/hosts.rs`
rather than a plugin system, each row with a page in the corpus.

A thread needed a path of its own, the way the repository mode has one. Measured on a
Hacker News page, the generic path stripped the author and time of every comment as
chrome (0 authors of 91) and dropped a short reply that was mostly a link by the
link-density rule (7 of 91) — a threshold the corpus has pinned for articles.

- **One look for every thread:** the post first, with its link, site, author and day;
  then every reply with its author and its day, linked to the reply. Replies are
  indented as quotes, three levels deep; a deeper reply says whom it answers. Lists of
  stories or topics read as lists of links, and a list's "More" or "Page 2" is its
  next page.
- **Hacker News and Lobsters**, by host: story pages, story lists, comment lists.
  Every reply carries an anchor, so a link to a comment opens the thread at it. The
  shelf gets the site's own menu, not the hundreds of "parent" and "next" links.
  Profiles and other pages take the general path.
- **Discourse**, by the forum software rather than by host. Recent forums answer
  `Accept: text/markdown` with Markdown that carries every post's author and date, and
  are read as they come; older ones serve a page for clients without JavaScript, and
  the rule reads that. Discourse threads are flat, and so is this.
- **reddit**, through the `.rss` of a thread or a subreddit: the pages are empty
  without JavaScript. Flat as well — the feed carries no nesting, and the page says so.
- **Documentation.** Read the Docs, GitBook and Mintlify sites serve Markdown
  themselves (#2). MDN and Wikipedia read through the general path, after fixes every
  page gets: a table whose rows are labelled keeps its labels, a fragment written with
  underscores finds its heading. Rust documentation — docs.rs, the standard library,
  any rustdoc build, known by its generator tag — has a rule of its own and comes
  whole, its code highlighted.
- **Still to come: Mastodon** (#37), put off on 13 October 2026.

### 9. Tables in the buffer (#10)

The known limitation: a table is a grid of widgets, so find-on-page and "copy
everything" do not see it. Either render tables as text in the buffer, or teach the
find and the copy to descend into the widgets. The saved Markdown already has the
table in full; the window should not know less than the file.

### 10. The companion extension (#5)

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
  browser wants an executable outside the sandbox
- the same path from the command line, so `--stdin` and the extension are one code
  path with two front doors

### 11. Updates on Linux (#1)

The Flatpak updates now, from Brevier's own repository rather than from a store:
`flatpak install --user https://brevier.bizibah.com/flatpak/brevier.flatpakref` once,
then `flatpak update`, GNOME Software or Discover bring every release. The site rebuilds
the repository from each release's bundle; its commits and summary are signed, and the
key travels in the `.flatpakref` and in the bundle, so a bundle from Releases updates
too. An update signed by another key, or older than the installed one, is refused.

Flathub was the plan, and it is set aside. Its policy on generative AI wants the
manifest, the submission and every reply to the reviewers written without AI, and the
AI-written code disclosed — nearly all of Brevier's — with no promise of acceptance; a
project only weeks old is "generally not accepted" either. What that costs is the
search: Brevier is not one search away in the store apps. The submission waited for
Wayland (#12), which is done; it can be taken up again if the policy or the project's
age changes.

Two cheaper doors alongside: crates.io (`cargo install brevier --features ui`, published
since 0.12.0), and the cli as one
file per tag for Linux, macOS and Windows — `brevier` links neither GTK nor OpenSSL.
Published on tags, not on pushes.

### 12. Reading progress (#19)

Done in the window and on Android, but for two things at the end. A long page is read
over several sittings, and it should not start from the top each time. Back, forward
and the session already returned to the place; now a page opened afresh — from a link,
the history, a bookmark, another tab — does too.

- **Continue, offered.** A page read for more than ten minutes is remembered: where
  the reader stopped, and which parts were read. Opened again, it opens at the top and
  offers "Continue from 43%" — in the status line in the window, as a line to tap on
  the phone. An offer, never a jump: opening a link can be a deliberate re-read. An
  address with a `#fragment` goes to the fragment, and no offer is made.
- **Ten minutes of reading**, counted only while the page is on screen and the reader
  is there. A page left open overnight has not been read for eight hours. Shorter
  visits are not written down.
- **What has been read, not how far.** A stretch of text counts once it has stayed on
  screen for a few seconds; one scrolled past does not. A jump through the contents to
  the last section does not read the article.
- **On the shelf**, a thin bar under every row: the share of its section read.
  Waypoints — the paragraph starts that stand in for headings when a page has none —
  are rows like the others.
- **A file you own:** `reading.tsv`, one line per page — address, a fingerprint of the
  text, the place, the stretches read. A thousand pages at most, the oldest leaving
  first; "forget everything" empties it. The core does the counting, so the window and
  Android share one implementation.

Known cost: offsets hold only while the text is the same, and pages change. When the
fingerprint differs, the place moves with the nearest heading's anchor, and what was
read starts over.

Still open:

- Every row of the shelf is labelled "40% · …", but the number is the row's place in
  the text, not the share read, which the bar shows. Which of the two belongs in the
  label is #24.
- A section several screens long is to be split by waypoints of its own, shown for the
  current section only. The core computes them; neither front-end shows them yet.

### 13. F-Droid (#31)

Done: Brevier is on F-Droid since 15 October 2026, starting with 0.11.0. The readers
most likely to want it — no JavaScript, no trackers, no account — look there first,
rather than in Releases.

- **Our APK, built by F-Droid.** The recipe in `fdroiddata` builds from source — Gradle
  in `android/`, the Rust core through the NDK, with Rust, the NDK and the JDK pinned —
  and F-Droid publishes the APK from Releases, with our signature, only when its own
  build matches it byte for byte. One key across Releases and F-Droid: an install from
  either updates from the other, and switching does not lose the reader's history.
  Every release checks the same in CI (`apksigcopier compare`).
- **The store page lives here**: the text, screenshots and a changelog per version in
  `fastlane/`, taken from the tagged commit.
- **New versions arrive by themselves**: F-Droid reads each new tag and builds it; a
  release reaches F-Droid a few days after Releases.

IzzyOnDroid, the quicker door, is closed: its policy declines apps whose code is written
with generative AI, and Brevier's is.

## Far — after that

### Send to

Lines in `settings.tsv` — `send ‹name› ‹command›` — and a "Send to…" menu in the
window. The command gets the article's Markdown on stdin and does what it likes:
append to a notes directory, print, run through `pandoc`. Anything that reads stdin.

### Subscriptions

Feeds on the shelf, `brevier:feeds`, unread marked. Refreshed when the reader asks;
whether to refresh on a timer as well is a question for when there are subscribers.

### A readable-web note

A short document, separate from this repository, saying what a page should serve so
that *any* reading client reads it well — the list from the manifesto, written down
with the check as its reference test and the alternate-Markdown link as its centre.
Written after the check has run on enough sites to know which findings matter.

### Android

A second front-end, not a second build — which is what separates it from the two
desktop builds below. The core crosses as it is: the library links no toolkit, its
dependencies are Rust the whole way down, and the dependency graph for
`aarch64-linux-android` holds no X11, no fontconfig, no GTK. CI cross-compiles it into
a shared library for arm64 on every push so that it stays that way, long before
anybody writes the front-end.

The window does not cross. GTK's Android backend is experimental, and the interaction
is keyboard and mouse throughout — Ctrl+L, a tooltip on hover, a middle click, a
divider dragged to width. None of those exist on a phone. So the front-end is native,
Kotlin over the core, and what travels with the core is the typographic model: the
measure is stated in ems rather than pixels, and it holds wherever it is drawn.

It draws the article itself, in native text, rather than handing it to a WebView. The
WebView is the cheaper road and an honest one to weigh: our Markdown, our HTML, our
CSS, scripts off by the toolkit's own default, and hyphenation, tables, selection and
TalkBack all arrive for free. What it costs is the promise. Reading the web through a
browser engine is the thing this program exists to stop doing, and the type would move
into a CSS layer that the desktop does not have, so one article would be set two ways
on two platforms. Drawing it natively is direct work rather than clever work: the core
hands a front-end a parsed structure instead of a blob of text, and the type is already
a handful of constants.

Nor is that front-end as large as the desktop window makes it look. Of the window's
five thousand lines, a quarter draw the article; the rest are tabs, the session,
history, the shelf, search, zoom and system glue, and a first release on a phone needs
almost none of it.

Two things argue for the platform rather than merely allowing it. That measure — 36
ems, some 65 characters — is a phone's natural column instead of a compromise on it.
And accessibility improves rather than degrades: native views speak to TalkBack, so
the gap the README states for every platform but Linux closes here instead of widening.

A spike has run, so what follows is measured rather than assumed. The core builds for
`aarch64-linux-android` and links into a 6 MB shared library; two JNI entry points are
enough for a phone to read a page; and the failure texts in the core serve an Android
front-end unchanged, which is what they were put in the core for.

Three of the four seams are small: the folders learn a fourth place, for which
`BREVIER_DATA_DIR` is the hook and it exists already; font fallback beyond the shipped
Noto has no fontconfig to ask and must ask the system instead; "open in your browser"
becomes an Intent.

The fourth is not small. The platform verifier needs a JNI initialisation and a Kotlin
component in the Gradle build, and with both in place it refuses most of the web:
Android's trust manager insists on OCSP revocation checking, Let's Encrypt stopped
publishing OCSP responders in August 2025, and the verifier reports the resulting
exception as a revoked certificate. Wikipedia, GitHub and the Rust blog all fail that
way, while a site whose certificate still carries an OCSP URL reads fine. The bug is
upstream and open (rustls/rustls-platform-verifier#221). That leaves three ways: wait
for the fix, patch the Kotlin component we already ship, or stop delegating trust to
the operating system — and the third costs a promise, so it is not a way.

### macOS and Windows

Windows has a build since 0.8.0, an installer and a zip; macOS not yet. GTK 4 runs on
both and the fonts ship inside the binary, so the typography holds. What does not hold is accessibility: GTK
speaks AT-SPI, and VoiceOver and NVDA do not listen. The builds ship with that stated
in the README, the way the Linux-only note is stated today. Whether the gap is later
closed with a second front-end is a question for when there are readers on those
platforms to ask.

With them comes the extra-root setting (#18): a certificate the reader adds, valid
only for the hosts they list next to it. On Linux "install the root in the system"
is the answer; on the other two it stops being an obvious instruction.

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
- **Platforms:** Linux first, updated from Brevier's own Flatpak repository (item 11);
  then Android, as a second front-end over
  the same core rather than another build of the same window — readers read on phones,
  and it is the one platform where accessibility improves rather than degrades; macOS
  and Windows builds after that, with their accessibility gap stated rather than hidden.
- **The Android front-end draws the article itself**, in native text rather than in a
  WebView: the type is a promise of the product, and a browser engine is precisely what
  the program exists to do without.
- **The archive** is on by default and compressed.
- **The check** gives a number, 0 to 100, with the arithmetic printed.
- **The alternate-Markdown link** is followed. A second request for a page that then
  needs no extraction is a good trade.
- **Requests are measured by reason, not counted.** "One page per request from a
  human" was considered as a rule and dropped: a page's images, an alternate Markdown
  copy, a dozen probes on a repository's CDN for its documentation are all part of
  reading the page. What stays out is crawling — walking a site, or fetching pages
  nobody asked to read.
- **Per-host rules exist, in one small table.** Extraction rules stay "by form, not by
  site" for the open web; threads and documentation hosts get named rules, each row
  with pages in the corpus, and no plugin system around them. This supersedes the
  earlier flat refusal of per-site rules.
- **Type is a setting; zoom is shared.** Face, size, measure, leading and justified
  text persist in `settings.tsv`; the zoom step is one for the window, applied to
  every page, for the run only — not per host, and not saved. (Per-host zoom was
  tried and dropped: it only made the next page open at a surprising size.)
- **A page read in the last week opens from a copy on disk**, images included —
  no fetch, no extraction. The week runs from the download. A list of links is
  always fetched, since its point is what is new; the status line says a page
  is a copy and how old it is; Reload (`Ctrl+R`) fetches it as it is now;
  "forget everything" deletes the copies. This is a cache, not the archive
  (#8): the archive keeps pages for good.
