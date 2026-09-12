<p align="center">
  <img src="assets/brevier.svg" alt="" width="104">
</p>

# Brevier

A browser for reading. It fetches a page, reduces it to Markdown on your own machine,
and renders it in the typography *you* chose — not the one the site shipped. It also
reads Markdown documentation straight out of git repositories.

There is no JavaScript engine and no site CSS. What survives is the text.

**Status: early.** The window runs on Linux and is built from source: there are no
packaged builds yet, and screen readers are supported on Linux only — see
[What it does not do](#what-it-does-not-do).

![An article in Brevier: text set on ivory paper in the reader's own measure, with the
page's table of contents on the shelf at the right and the section being read marked in
it](assets/screenshot-article.png)

## Not a converter

"Web page → Markdown" is a shelf product, and Brevier does not pretend otherwise: the
pipeline is built from other people's crates (`dom_smoothie` for extraction, `htmd` for
conversion). The difference is what the Markdown is *for*. Converters make it data for a
machine — food for a model, an index, an archive. Here it is an internal representation,
and the product is the window: your measure, your leading, your type, on every site.

## Build

Rust 1.88 or newer — edition 2024 needs 1.85, and `slice::as_chunks` in the image
decoder needs 1.88. CI builds on current stable.

```sh
cargo build --release                  # brevier — the cli
cargo build --release --features ui    # brevier-ui — the window, on GTK4
```

The `ui` feature needs GTK 4 development files:

```sh
sudo apt install libgtk-4-dev build-essential   # Debian/Ubuntu
```

`ui` pulls in image decoding (`images`) and saving (`save`). The cli builds without any
of them.

## A desktop entry

The icon ships inside the binary: the window wears it on X11 with nothing installed —
Brevier unpacks it into its own cache and points GTK's icon theme there. A launcher menu
and a Wayland compositor need more than that, because there the icon is chosen by the
desktop entry and the application id, not by the program. To install both for your user:

```sh
cargo install --path . --features ui
install -Dm644 packaging/dev.brevier.Brevier.desktop \
        ~/.local/share/applications/dev.brevier.Brevier.desktop
install -Dm644 assets/brevier.svg \
        ~/.local/share/icons/hicolor/scalable/apps/dev.brevier.Brevier.svg
update-desktop-database ~/.local/share/applications
```

The entry declares `http`, `https` and `text/markdown`, so Brevier appears in "Open with"
and can be chosen as the browser for a link. It does not make itself the default.

## Use

```sh
brevier https://example.com/article     # Markdown on stdout
brevier gh:rust-lang/book               # a repository's README
brevier gh:rust-lang/book/src           # the README of a directory inside it
brevier gh:rust-lang/book/src/          # …or what the directory holds, listed
brevier gl:owner/repo                   # the same for GitLab
brevier --docs gh:rust-lang/book        # entry points into its documentation
brevier --links <url>                   # the article's outgoing links, one per line
brevier --raw <url>                     # no extraction, the whole page
brevier --html <url>                    # the extracted HTML, before conversion
brevier --stdin <url> < page.html       # HTML you already have; the url is
                                        # where it came from, for its links
brevier --save <url>                    # write it to a file instead of stdout
brevier --save -o notes.zip <url>       # …under a name you chose
brevier brevier:history                 # what you have read, by day
brevier <url> | less

brevier-ui <url> [<url>…]               # read in a window, one tab per address
```

A pasted GitHub or GitLab file URL is understood too, and so is a path to a local `.md`.

`--save` needs the `save` feature (the `ui` build has it): `.md` is the text alone,
`.zip` is the text plus an `images/` folder with the links rewritten into the archive.
Without `-o` the name comes from the article's title, and an existing file of that name
stops the run rather than being overwritten — with `-o` your choice is older than ours.

`brevier-ui` takes as many addresses as you like, a tab each, and answers `--help` and
`--version` in the terminal. It is a single application: an address handed to it while it
is running — from the command line, or from a link you clicked in another program — opens
as a tab in the window you already have, the way a browser does. Launching it with no
address opens another window, which is how you ask for one.

Below is `gh:gurov/brevier` — this very README, read out of the repository in the same
type as any article, with its sections on the shelf:

![The repository mode: this README rendered in Brevier, its headings listed on the shelf
at the right](assets/screenshot-repository.png)

Exit codes: 1 bad url, 2 network, 3 http status, 4 content type, 5 nothing extracted,
6 conversion. They exist so that a batch run can tell "the site refused" from
"extraction failed" — different numbers, different work.

## Keys

| | |
|---|---|
| `Enter` | open the address |
| `Space`, `PageUp`/`PageDown` | page down / up |
| arrows, `Home`/`End` | line, top, bottom |
| `Ctrl+L` | focus the address bar |
| `Ctrl+T` / `Ctrl+W` | new tab / close tab |
| `Ctrl+H` | what you have read |
| `Ctrl+D` | keep this page, or take it off again |
| `Ctrl+Tab`, `Ctrl+PageUp`/`PageDown` | switch tabs |
| `Ctrl+F` | find on page |
| `Ctrl++` / `Ctrl+-` / `Ctrl+0` | zoom the page in, out, back to 100% |
| `Ctrl`+wheel | the same, with the mouse |
| `Ctrl+S` | save the article |
| `Ctrl+O` | hand the page to your system browser |

Ctrl+click and middle-click open a link in a new tab. Selection, copying and the context
menu come from GTK.

## In the window

- **Typography is fixed by the reader, not the site:** 16.5pt, a measure of 36 ems
  (about 65 characters), 1.55 leading, and headings that are *lighter* than the text
  rather than bolder — at a large size weight shouts instead of leading.
- **Ivory paper** (`#faf5ea`) instead of white, which glows on a screen; a warm dark
  theme is one switch away, behind the gear.
- **Page zoom on the browser's own ladder** (67…200%), and it scales the whole
  typographic model, not just the body size — so a line still holds about 65 characters
  at every step. The step is remembered per site for this run only, and a button in the
  header shows it whenever it is not 100%.
- **Fonts ship inside the binary** — Noto Sans and Noto Sans Mono, under the OFL. If the
  operating system picked the type, the promise would not hold on any of the three.
- **A shelf on the right:** entry points into a repository's documentation on top, the
  open page's table of contents under them, with the section under your eyes marked as
  you scroll. Its width is yours, by dragging the divider.
- **Directories are browsable** — a trailing slash in the address (`gh:owner/repo/docs/`)
  or the shelf's "Files in this directory" lists what is there, so a README that links to
  nothing is no longer a dead end. This is the one place the repository mode asks the
  hosting's API, and only when you ask for it: GitHub allows sixty such requests an hour
  without a token, everything else comes from the CDN, which has no limit.
- **Images load right away, and a switch in Settings turns them off** — the decoder is
  the one serious attack surface once JavaScript is gone, and closing it should be
  possible.
  A formula standing in a line of text is drawn as a canvas, an illustration gets its
  own line and a caption.
- **Code blocks get a deliberately dumb highlighter** — comment, string, number,
  keyword; four things visible in any language, with no grammars and no C regex engine.
- **A page that is a list of links** (a blog front page, a section of a site) is shown
  as a list, and the status line says "A list of links, not an article" instead of
  pretending there was an article to find.
- **`Ctrl+S` saves** a `.md`, or a zip with an `images/` folder when the page has
  pictures and they were loaded.
- **Tabs come back.** Close the window with five things half-read and they are there next
  time: every tab, its back and forward, the tab you were on, and the line you were on in
  each. Only the tab you were on is fetched at startup; the rest are named on the strip —
  by the title they had when you read them — and load the moment you switch to one.
  Opening Brevier with an address on the command line does not restore — you asked for
  a page, not for yesterday.
- **A star keeps the page** (`Ctrl+D`, or the star in the header) and pressing it again
  takes it off. The list is `brevier:bookmarks`; History and Bookmarks link to each
  other, so one button in the header reaches both.
- **Pages you read are remembered, and the address bar suggests them** as you type —
  what matched from the start of the host first, then the rest of the address, then the
  title; among equals, where you go often and where you went last. `Ctrl+H` opens the
  list itself (`brevier:history`), grouped by day, set in the same type as an article,
  because it is the same Markdown.

## How well does it work

On the M0 corpus of 108 live addresses: **80 readable — 74%**. Of the 89 pages that
actually reached the extractor, **80 — 90%**; the other 19 are closed by access rather
than by conversion (certificates 8, 403/401 7, host silent 2, empty extraction behind an
anti-bot 2).

The honest caveat: the rubric (`corpus/RUBRIC.md`) is ours, the scoring is done by hand,
and it was relaxed once — knowing the numbers. A strict reading of the same markings
gave 56%. So those are the project's own regression figures. Scripts and expected outputs
live in `corpus/`.

Measured against someone else's ruler — `scrapinghub/article-extraction-benchmark`, 181
saved pages, article body scored on word 4-grams — the whole reading pipeline gets
**F1 0.929** (precision 0.885, recall 0.978); the extraction alone gets 0.951, against
0.947 for Readability.js and 0.958 for trafilatura in the same table. Two thirds of the
gap between those two numbers is the headline and the byline, which we print on purpose
and that benchmark's ground truth excludes by definition. `corpus/bench.py` reproduces
it. That benchmark is news, though, and this program is for long-form, documentation and
threads — it is a second opinion, not the gate.

Extraction breaks constantly: sites change their markup and heuristics rot. That is the
permanent background of this kind of program, not a task that finishes.

## What it does not do

- **JavaScript** — never, in any form, not even "just for this one site".
- **Site CSS** — never; the typography is the reader's.
- **Forms, logins, cookies** — the web is read-only here, deliberately.
- **SPA sites** — honest degradation, plus "Open in your browser".
- **Video, audio, extensions** — no.
- **An AI model inside the product** — no. Conversion is not the bottleneck (90% of the
  pages that reach the extractor are readable), and the price would be hundreds of
  megabytes, seconds per article, non-deterministic output instead of byte-exact
  regression files, and the risk of handing the reader invented text in place of the
  author's.
- **Accessibility outside Linux** — GTK4 speaks AT-SPI, so screen readers work on Linux
  only: NVDA on Windows and VoiceOver on macOS will not see anything in this window.
  That is the price of the toolkit choice, stated plainly rather than by omission.
- **Packaged builds** — not yet.
- **Privacy** — not sold here. Sites may track a reader exactly as they always could.

Known limitation: a table is drawn as a grid of widgets anchored in the text buffer, so
find-on-page and "copy everything" do not see its contents. The saved Markdown has the
table in full.

## Markdown

CommonMark + GFM, through `comrak` — "Markdown" without a dialect means nothing. The
same representation serves both modes, which is what makes saving nearly free; the price
is what it cannot carry: tables nested in lists, definition lists, footnotes, sub/sup,
ruby. That loss is also the noise removal this program is for.

GitHub alerts (`> [!NOTE]`) are read as alerts: a quote that says what it is, not a quote
whose first line reads "[!NOTE]". No coloured box — the colour would be the site's
typography, the label is the meaning.

Footnotes are reduced to one form. On the web a footnote is a link to an anchor with its
text in a list at the bottom, and every site spells it differently; none of it survived
conversion, so the mark became a dead link and, in `less`, a line of noise — 277 of them
on one Wikipedia article. They now become GFM footnotes, and in the window the mark is a
superscript that jumps to the note and back.

## On the disk

History and the open tabs live in plain text files, one line each, tab-separated:

```
~/.local/share/brevier/history.tsv            # Linux, or $XDG_DATA_HOME/brevier
~/.local/share/brevier/session.tsv            # …and the tabs you left open
~/.local/share/brevier/bookmarks.tsv          # …and the pages you kept
~/Library/Application Support/Brevier/        # macOS
%LOCALAPPDATA%\Brevier\                      # Windows
```

What you decided once — the theme, whether images load, the shelf and its width — is in
`$XDG_CONFIG_HOME/brevier/settings.tsv`, and a cache will go to `$XDG_CACHE_HOME/brevier`:
three directories rather than one profile, because the three have different fates —
you would carry the first with you, edit the second by hand, and throw the third away
without looking. `BREVIER_DATA_DIR` moves all of them, the way `--user-data-dir` does
for a browser.

The settings file is the one people actually open in an editor, so its parsing is the
forgiving kind: any whitespace separates a name from its value, an unknown line is
skipped, a missing one keeps its default.

Page zoom is the deliberate exception: it is remembered per site, but only for the run.
What differs between sites is not the typography — Brevier sets that — but the material,
and a file of per-site overrides is a question of "why does this site open strangely?"
half a year later.

Text rather than a database: SQLite would mean a C library in a program that sells
memory safety, and a reading history is thousands of lines, not millions. The file is
yours — open it in an editor, delete a line to forget a page, delete the file to forget
everything. The oldest visits are dropped once there are more than five thousand.

Settings has a button that forgets everything you have read — it empties the journal
and nothing else: bookmarks and open tabs stay. One page is forgotten by deleting its
line from the file.

Only the window writes there. `brevier <url> | less` is a pipe tool and keeps no
history of its own, and neither the pages Brevier failed to open nor its own
`brevier:history` are recorded.

The session file is a snapshot, not a log: `here` or `tab`, the entry of that tab's
history you were on, how far down the text you were, then its addresses. The place is
an offset in the text rather than in pixels, so it survives a different window size,
a different zoom step, even a different font. When several windows are open, the first
one owns the session — the second is "open me one more link", not "here are my tabs".

## On the network

- **One page per request from a human.** Brevier does not crawl, does not prefetch and
  does not fan out over a site. robots.txt addresses crawlers; this is not one.
- **The User-Agent is honest** — `Brevier/0.1`. Chosen by measurement, not by principle:
  on our corpus a browser-shaped UA lost 7:0, every case a 403 from an anti-bot.
- **TLS trust is delegated to the operating system** (`rustls` +
  `rustls-platform-verifier`). There is no bundled root store, and no way to skip
  verification. Practical consequence: a site signed by a CA your system does not know
  will not open until that root is installed — `curl` behaves the same way.

## Contributing

Patches and bug reports are welcome. `cargo test` and
`cargo clippy --all-targets --features ui` should be clean, and any change to extraction
or conversion needs a corpus run diffed against `corpus/expected/` before it is
committed: those files exist to catch regressions, and they have already caught two.

Two Python scripts live in `corpus/`, both workshop tools that never ship with the
product: `reference.py`, a marker's aid that compares our output with the page as a
headless browser sees it (needs `beautifulsoup4` and Chrome), and `bench.py`, which
runs the external `scrapinghub/article-extraction-benchmark` (no dependencies).

## License

MIT OR Apache-2.0, at your option.

The bundled fonts are under the SIL Open Font License; `assets/fonts/OFL-NotoSans.txt`
must travel with any distribution — that is a condition of the OFL, separate from the
license on the code.
