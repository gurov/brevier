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
  `$XDG_DATA_HOME/brevier/archive/<host>/<date>-<slug>.md.lz4`, one file per page
  (`.md`, if the reader chose plain copies — item 15), and the history line points to its
  copy. LZ4 in pure Rust (`lz4_flex`), for the
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

Done. Per-host rules, kept in one small table in `src/hosts.rs`
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
- **Mastodon** (#37), by the server's software rather than by host: the page of a post is
  empty without JavaScript, so the post and its replies are asked for from the same
  server, two requests. The author's own chain of replies continues the post; a post that
  answers another shows what it answers. Akkoma and Pleroma are not covered.

### 9. Tables in the buffer (#10)

Done, the second way: the table stays a grid of widgets — text in the buffer would
have needed tab stops per table and lost the wrapping inside cells — and find and copy
go into it. Find-on-page searches every cell the way it searches the text and steps
through the matches in reading order, a table's cells where the table stands; copying
a selection that holds a table puts the table in its place as rows of tab-separated
cells, which pastes as text or into a spreadsheet. Android finds in tables the same
way. The window no longer knows less than the saved file.

### 10. The companion extension (#5)

Dropped, on 13 November 2026, before any of it was built. The plan was a WebExtension
for Firefox and Chrome with one menu item, "Read in Brevier", handing Brevier the page
as the browser had rendered it: a way in for pages built by scripts and pages behind a
login, and a way for people to find Brevier from the browser they already use. But a
reader who is in that browser already has a reading mode one click away — Chrome,
Firefox and Safari each carry one — and a second reading mode inside the same browser
is not something anyone is missing. What it would have cost — two extension stores, a
native-messaging host on three platforms, browsers that live in sandboxes of their own —
buys too little.

So a page built by scripts degrades honestly, with "Open in your browser", and a page
behind a login is read in the browser. Brevier still holds no cookie.

### 11. Updates on Linux (#1)

Done. The Flatpak updates from Brevier's own repository rather than from a store:
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
file per release for Linux, macOS and Windows, since 0.15.0 — `brevier` links neither
GTK nor OpenSSL. Published on tags, not on pushes.

### 12. Reading progress (#19)

Done in the window and on Android. A long page is read
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
  are rows like the others, with their place in the text at the right, "40%", the way
  a book's contents gives page numbers (#24): where the row is, while the bar says how
  much of it is read. Headings carry no number; their name says where they are.
  A section several screens long is split by waypoints of its own, one level down,
  shown for the current section only: they come and go as the reader moves on.
- **A file you own:** `reading.tsv`, one line per page — address, a fingerprint of the
  text, the place, the stretches read. A thousand pages at most, the oldest leaving
  first; "forget everything" empties it. The core does the counting, so the window and
  Android share one implementation.

Known cost: offsets hold only while the text is the same, and pages change. When the
fingerprint differs, the place moves with the nearest heading's anchor, and what was
read starts over.

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

### 14. A white page (#44)

Done, in the window and on Android; the Windows branch has not yet met a live Windows.

The ivory paper is a choice, not an accident: pure white glows on a screen, and the warm
tone takes the glow off without touching the contrast. But the page is the reader's, and
some readers want black on white and nothing softer. A switch beside the theme — usual
or high contrast — in the window and on the phone.

- **High contrast in the light theme:** white paper, ink near black, and everything
  else darkened to match — muted text, comments in code, table rules, the shelf. Text
  of any kind at 7:1 or better. In the dark theme, the same rule the other way: black
  paper, light ink.
- **The usual set has a debt of its own.** Muted text on ivory measures 4.4:1, just
  under the 4.5 that WCAG asks for; comments in code, 3.8. Those are raised whatever
  the switch says.
- **The system is listened to** where it states a preference: the appearance portal on
  Linux, the contrast level on Android 14 and later, high-contrast mode on Windows.
- The colours stay in one table in the core, shared by the window and the phone. The
  site keeps the ivory.

### 15. Plain copies (#43)

Done, in the window; the phone writes compressed copies, for the reason below.

The archive promises that only a decompressor stands between the reader and their text.
For the reader's own eyes that holds; for their tools it does not — desktop search,
Obsidian, another reader app do not open LZ4. A setting beside the archive switch writes
copies as plain `.md`, the header kept: those tools read it as properties.

- **Both kinds side by side.** Switching changes how new copies are written; the old
  ones stay as they are, and the archive reads, searches and cleans up both.
- **Size is not the argument.** On the corpus, LZ4 gives 1.84×: 28 KB of Markdown a
  page against 15, a thousand pages 28 MB against 15. Compressed stays the default
  (see *Decided*) unless that is decided again.
- **The phone is a case apart.** There the archive lives in the app's private storage,
  where no other app sees it, compressed or not. A plain copy helps there only together
  with a folder the reader picks, and that is a decision of its own.

### 16. Pictures in the archive (#42)

Done. The reader's own pictures came first: since 0.19.0 a note can hold them, in a
margin beside its passage or under its paragraph.

A copy is Markdown: the text outlives the site, the pictures die with it. For most pages
that is the right trade. A bookmark, though, promises a page kept for good, and in
documentation the diagrams and screenshots are half of the page.

- **Kept for the pages kept for good** — bookmarks and pages with notes. Every other
  copy stays text only. No new setting.
- **Not one extra request.** The pictures of a page being read are already downloaded;
  bookmarking it, writing a note on it or reading it again copies them from the tab and
  from the week's cache. Pictures turned off stay off, and a picture not at hand is not
  fetched for this.
- **Beside the copy, not inside it.** The copy's text is untouched — search, notes and
  the reading place rest on it. The pictures lie in the archive byte for byte, as `--save`
  writes them, one folder per page rather than per copy (`archive/_pictures/`), with a
  list of which address is which file; they leave with the page's last copy. A kept page
  shows its pictures from there before asking the site.
- **Not into the phone's backup.** Android backs an app up to Google only up to 25 MB
  and stops altogether beyond that, bookmarks and notes included. Pictures would get
  there fast, and the archive's text gets there too, at about 1,700 pages — so the
  backup leaves the archive's pictures out (moving to a new phone takes them along).
  The archive's text still goes into the backup.

Known cost: disk, up to 32 MB a page.

### 17. The bar steps aside, on the phone (#45)

Done, with the rules Firefox for Android uses for its own bar. Sliding is the default:
decided on 25 October 2026, before the two weeks, while readers are still few.

On a phone the bar — Back, the address, Contents, tabs, the menu — takes a real share
of the screen, and Firefox has taught readers that it can slide away while they read.
A bar that comes and goes is also the quickest way to irritate, so the rules come
first:

- **The text does not move.** The bar slides over the article rather than handing it
  the room; no line shifts, and the reading place stays where it was.
- **It follows the finger:** out as far as the text went down, back as far as it came
  up, settling fully in or fully out on release. No timer, no animation that starts by
  itself, nothing on a twitch, a tap or a selection.
- **It stays where it is needed:** at the top and at the end of a page, on a new page,
  while an address is typed, a word is searched, text is selected or a note is written;
  never on Brevier's own pages or on a page shorter than two screens.
- **Jumps land below it.** Contents, find and "Continue from 43%" count from its lower
  edge while it is shown.
- The status bar and the progress strip stay. A setting keeps the bar in place; whether
  sliding is the default is decided after two weeks of reading with it, on a real phone.

## Far — after that

### Send to

Lines in `settings.tsv` — `send ‹name› ‹command›` — and a "Send to…" menu in the
window. The command gets the article's Markdown on stdin and does what it likes:
append to a notes directory, print, run through `pandoc`. Anything that reads stdin.

### Subscriptions

Feeds on the shelf, `brevier:feeds`, what is new marked but never counted. To be worked
through before it is built: an edition rather than a stream. Entries arrive once or
twice a day, and the list ends with a plain "That's all for today". A refresh at will
with a number beside it is a lever with a prize that may or may not be there, and it is
the prize that keeps people pulling, not the gesture. This is reasoning, not a
measurement: the experiments on stopping are about social media, not reading. How an
edition sits with a feed opened by its address, which is always fetched, is part of
the work.

### What do you remember?

At the end of a long article, an offer to write one line in the reader's own words,
with the article out of sight, kept as a note on the page. Recalling does more for
memory than rereading: self-testing is one of the two study techniques rated highest
in the review by Dunlosky and others (2013). But that is research on studying, and
someone reading for pleasure may only be irritated, so it stays off until the reader
turns it on. To be worked through before it is built, starting with the papers
themselves: what counts as a long article read to its end, how to offer this without
interrupting, and whether the reader's line should come back later (next item).

### Notes that come back

One old note on the start page, some weeks after it was written: the reader's own
words rather than a bare highlight, since going back over one's highlighting does
little for memory. No counter, no queue, nothing owed; a note left unread simply goes
away again. Spaced review is well supported, but this way of showing notes has not been
tested directly that we know of. Off until the reader turns it on. To be worked through
before it is built: how long to wait, which note, and what the phone does.

### Pages to turn, made known

Turning pages on the phone exists (#55) and is off by default. Scrolling has been found
to hurt the understanding of complex text, the more so for readers with less working
memory, because the words lose their place on a page; the findings are not uniform, and
one study found no difference. To be worked through before anything changes: how a
reader comes to know about it, and whether a page should begin at the same word every
time, since a place on a page is the whole argument.

### A readable-web note

A short document, separate from this repository, saying what a page should serve so
that *any* reading client reads it well — the list from the manifesto, written down
with the check as its reference test and the alternate-Markdown link as its centre.
Written after the check has run on enough sites to know which findings matter.

### Android

Done. A second front-end, not a second build: Kotlin over the same core, on F-Droid since
0.11.0. The window did not cross, its interaction being keyboard and mouse throughout;
what travelled with the core is the typographic model, so the measure holds wherever it
is drawn. The article is drawn in native text rather than in a WebView, because reading
the web through a browser engine is the thing this program exists to stop doing. The wall
that refused most of the web on Android, OCSP revocation checks after Let's Encrypt
stopped publishing responders, fell with `rustls-platform-verifier` 0.7.1, so trust stayed
with the operating system. Google Play comes next (#48). TalkBack should read the native
text, but that has not been tested yet.

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

- **Authenticated reading** stays in the browser. Brevier never holds a cookie or any
  other credential, and the companion extension that would have handed such pages over
  was dropped (item 10).
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
- **Kept out on purpose.** None of these is to be built, however good the case for it
  sounds:
  - **Pull-to-refresh.** An article does not change while it is read, and Reload is
    there; on a list it is the lever described under Subscriptions.
  - **Streaks, points and reading statistics.** The aim would move from the text to the
    number.
  - **A countdown of the time left in an article.** Hurry is when a screen loses most
    to paper.
  - **One word at a time (RSVP).** Going back over a line is part of understanding it.
  - **Bionic Reading.** We found no peer-reviewed evidence that it helps.
  - **Fonts said to help memory.** Sans Forgetica did not hold up when the experiments
    were repeated.
  - **A dark theme by default.** Dark text on a light page did better in proofreading
    experiments. Dark stays one choice away.
  - **Lectures.** No message tells the reader how to spend their time.
