<p align="center">
  <img src="assets/brevier.svg" alt="" width="122">
</p>

# Brevier

A browser for reading, and nothing else. It fetches a page, reduces it to Markdown on
your own machine, and sets it in the typography *you* chose — not the one the site
shipped. It also reads Markdown documentation straight out of git repositories.

No JavaScript engine, no site CSS. The modern web ships a document wrapped in a program;
Brevier keeps the document and throws the program away. The site gets a vote on the words.
Not on the type.

Why it is built this way is in the [manifesto](MANIFESTO.md); what comes next, and what
has been decided, is in the [roadmap](ROADMAP.md) — every near-term item there is an open
issue labelled `roadmap`.

**Status: early.** The window runs on Linux; there is a Flatpak bundle and a tarball to
build or download, nothing in a store yet, and screen readers work on Linux only (see
[What it does not do](#what-it-does-not-do)).

![An article in Brevier: text set on ivory paper in the reader's own measure, with the
page's table of contents on the shelf at the right and the section being read marked in
it](assets/screenshot-article.png)

## Not a converter

"Web page → Markdown" is a shelf product, and Brevier does not pretend otherwise: the
pipeline is built from other people's crates (`dom_smoothie` for extraction, `htmd` for
conversion). The difference is what the Markdown is *for*. Converters make it data for a
machine — food for a model, an index, an archive. Here it is plumbing you never see, and
the product is the window: your measure, your leading, your type, on every site.

## Install

Nothing is in a store yet — two builds in the
[releases](https://github.com/gurov/brevier/releases/latest), and the source.

**A Flatpak bundle.** One file, one command; the runtime brings GTK, so it does not care
which distribution is underneath:

```sh
wget https://github.com/gurov/brevier/releases/download/v0.1.1/brevier.flatpak
flatpak install --user ./brevier.flatpak
flatpak run io.github.gurov.brevier https://example.com/article
```

The first install also pulls `org.gnome.Platform//49` from Flathub if it is missing —
about 400 MB, once for every Flatpak that uses it. After that Brevier is in the menu like
any other application. To build the bundle yourself, see `packaging/`: the build runs
offline the way Flathub's does, every crate declared with its checksum in
`packaging/cargo-sources.json` (regenerate it with `packaging/cargo-sources.py` after
bumping a dependency).

What the sandbox changes, stated rather than discovered later: history, bookmarks and
settings live in `~/.var/app/io.github.gurov.brevier/`; saving goes through the file
portal; a local `.md` path cannot be opened, because the sandbox gets no filesystem access
at all; and "Open in your browser" asks the portal, so the host picks.

**A tarball**, for a machine that already has GTK 4 and would rather not have a sandbox —
also in the [release](https://github.com/gurov/brevier/releases/latest), or built with
`packaging/tarball.sh`:

```sh
tar xf brevier-0.1.1-x86_64-linux.tar.gz
cd brevier-0.1.1-x86_64-linux && ./install.sh
```

`install.sh` puts the binaries, the desktop entry, the icon and the licenses under
`~/.local`, needs no root, and takes `--uninstall`. It wants GTK 4 in the system
(`libgtk-4-1` on Debian and Ubuntu) and a glibc no older than the build machine's.

## Build

Rust 1.88 or newer (edition 2024 needs 1.85; the image decoder's `slice::as_chunks` needs
1.88). The `ui` feature needs GTK 4 development files.

```sh
cargo build --release                  # brevier — the cli
cargo build --release --features ui    # brevier-ui — the window, on GTK4
sudo apt install libgtk-4-dev build-essential   # Debian/Ubuntu, for the ui feature
```

`ui` pulls in image decoding and saving; the cli builds without either.

## A desktop entry

The icon ships inside the binary, so the window wears it on X11 with nothing installed. A
launcher menu and a Wayland compositor pick the icon from the desktop entry and the app id
instead — to install both for your user:

```sh
cargo install --path . --features ui
install -Dm644 packaging/io.github.gurov.brevier.desktop \
        ~/.local/share/applications/io.github.gurov.brevier.desktop
install -Dm644 assets/brevier.svg \
        ~/.local/share/icons/hicolor/scalable/apps/io.github.gurov.brevier.svg
update-desktop-database ~/.local/share/applications
```

The id is `io.github.gurov.brevier`, after the repository — the form Flathub's rules give a
project on GitHub, since `brevier.dev` is not ours yet. The entry declares `http`, `https`
and `text/markdown`, so Brevier turns up in "Open with"; it does not make itself the
default (`xdg-settings set default-web-browser io.github.gurov.brevier.desktop` if you want
that). Made default it still lets you out — `Ctrl+O` hands the page to the first registered
browser that is not Brevier, or, inside a Flatpak, to the portal.

If the entry shows up without its icon, the shell cached its icon themes before the
directory existed: log out and back in, or restart the shell.

## Use

```sh
brevier https://example.com/article     # Markdown on stdout
brevier gh:rust-lang/book               # a repository's README
brevier gh:rust-lang/book/src           # the README of a directory inside it
brevier gh:rust-lang/book/src/          # …or what the directory holds, listed
brevier gl:owner/repo                   # the same for GitLab
brevier --docs gh:rust-lang/book        # entry points into its documentation
brevier --check <url>                   # score the page for a scriptless reader
brevier --links <url>                   # the article's outgoing links, one per line
brevier --nav <url>                     # the site's own navigation: menu and footer
brevier --raw <url>                     # no extraction, the whole page
brevier --html <url>                    # the extracted HTML, before conversion
brevier --stdin <url> < page.html       # HTML you already have; the url is
                                        # where it came from, for its links
brevier --save <url>                    # write it to a file instead of stdout
brevier brevier:history                 # what you have read, by day

brevier-ui <url> [<url>…]               # read in a window, one tab per address
```

A pasted GitHub or GitLab file URL is understood too, and so is a path to a local `.md`.
`--save` (in the `ui` build) writes `.md`, or a `.zip` of the text plus an `images/` folder
when the page has pictures; `-o` names the file, and without it an existing file of the
article's name stops the run rather than being overwritten.

`brevier-ui` takes as many addresses as you like, a tab each. It is a single application:
an address handed to it while it runs — from the command line, or a link you clicked in
another program — opens as a tab in the window you already have, the way a browser does.
Launching it with no address opens another window, which is how you ask for one.

Below is `gh:gurov/brevier` — this very README, read out of the repository in the same type
as any article, with its sections on the shelf:

![The repository mode: this README rendered in Brevier, its headings listed on the shelf
at the right](assets/screenshot-repository.png)

Exit codes: 1 bad url, 2 network, 3 http status, 4 content type, 5 nothing extracted,
6 conversion — so a batch run can tell "the site refused" from "extraction failed".

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
| `Ctrl+S` | save the article |
| `Ctrl+O` | hand the page to your system browser |

Ctrl+click and middle-click open a link in a new tab; middle-click a tab closes it.
Hovering a link shows where it goes. Settings, history and bookmarks are one menu in the
header. Selection, copying and the context menu come from GTK.

## In the window

- **Typography is the reader's, not the site's.** Today that means Brevier's own defaults
  — 16.5pt, a measure of 36 ems (about 65 characters), 1.55 leading, and headings that are
  *lighter* than the text rather than bolder, because at a large size weight shouts instead
  of leading — plus zoom and the theme. A settings page for face, size and measure is on
  the roadmap.
- **Ivory paper** (`#faf5ea`) instead of white, which glows on a screen; a warm dark theme
  is a switch away in the menu.
- **Page zoom on the browser's ladder** (67…200%) scales the whole typographic model, not
  just the body size — so a line still holds about 65 characters at every step. It is one
  step for the window, shared across tabs and kept for the run only.
- **Back and forward return you to where you were** — the same spot on the page, drawn from
  memory rather than fetched again, images already in place. An image that loads up above
  the viewport no longer jogs the text under your eyes.
- **Fonts ship inside the binary** — Noto Sans and Noto Sans Mono, under the OFL. If the
  operating system picked the type, the promise would not hold on any of the three.
- **A shelf on the right:** a repository's documentation on top, the page's table of
  contents under it with the section under your eyes marked as you scroll, the site's own
  menu below that. Its width is yours, by dragging the divider.
- **Directories are browsable** — a trailing slash (`gh:owner/repo/docs/`) or the shelf's
  "Files in this directory" lists what is there, so a README that links to nothing is not a
  dead end. This is the one place the repository mode asks the hosting's API, and only when
  you ask: GitHub allows sixty such requests an hour without a token; everything else comes
  from the CDN, which has no limit.
- **Images load right away, and a switch turns them off** — the decoder is the one serious
  attack surface once JavaScript is gone, and closing it should be possible. A formula in a
  line of text is drawn as a canvas; an illustration gets its own line and a caption.
- **A page that is a list of links** (a blog front page, a section of a site) is shown as a
  list, and the status line says so, instead of pretending there was an article to find.
- **Tabs come back.** Close the window with five things half-read and they are there next
  time: every tab, its back and forward, the tab and the line you were on. Only the tab you
  were on is fetched at startup; the rest load the moment you switch to one.
- **Pages you read are remembered, and the address bar suggests them** as you type. `Ctrl+H`
  opens the list itself, grouped by day; a star (`Ctrl+D`) keeps a page, and the two lists
  link to each other.

## How well does it work

On the M0 corpus of 108 live addresses: **80 readable — 74%**. Of the 89 pages that
actually reached the extractor, **80 — 90%**; the other 19 are closed by access rather than
by conversion (certificates 8, 403/401 7, host silent 2, empty behind an anti-bot 2).

The honest caveat: the rubric (`corpus/RUBRIC.md`) is ours, the scoring is by hand, and it
was relaxed once — knowing the numbers. A strict reading of the same markings gave 56%. So
those are the project's own regression figures, not a claim against anyone else.

Measured against someone else's ruler — `scrapinghub/article-extraction-benchmark`, 181
saved pages scored on word 4-grams — the whole reading pipeline gets **F1 0.929**
(precision 0.885, recall 0.978); extraction alone gets 0.951, against 0.947 for
Readability.js and 0.958 for trafilatura in the same table. It is a second opinion, not the
gate: that benchmark is news, and this program is for long-form, documentation and threads.

Extraction breaks constantly — sites change their markup and heuristics rot. That is the
permanent background of this kind of program, not a task that finishes.

## What it does not do

- **JavaScript** — never, in any form, not even "just for this one site".
- **Site CSS** — never; the typography is the reader's.
- **Forms, logins, cookies** — the web is read-only here, deliberately.
- **SPA sites** — honest degradation, plus "Open in your browser".
- **Video, audio, extensions** — no.
- **An AI model inside the product** — no. Conversion is not the bottleneck (90% of the
  pages that reach the extractor are readable), and the price would be hundreds of
  megabytes, seconds per article, non-deterministic output instead of byte-exact regression
  files, and the risk of handing the reader invented text in place of the author's.
- **Accessibility outside Linux** — GTK4 speaks AT-SPI, so screen readers work on Linux
  only; NVDA on Windows and VoiceOver on macOS see nothing here. That is the price of the
  toolkit, stated plainly rather than by omission.
- **A store listing** — not yet. A Flatpak bundle and a tarball are in Releases; Flathub is
  on the roadmap, and with it the updates a bundle cannot deliver.
- **Privacy** — not sold here. Sites may track a reader exactly as they always could.

Known limitation: a table is a grid of widgets anchored in the text buffer, so find-on-page
and "copy everything" do not see its contents. The saved Markdown has it in full.

## Markdown

CommonMark + GFM, through `comrak` — "Markdown" without a dialect means nothing. One
representation serves both modes, which is what makes saving nearly free; the price is what
it cannot carry (tables nested in lists, definition lists, footnotes, sub/sup, ruby), and
that loss is also the noise removal this program is for.

A site that serves Markdown is read exactly, no extraction in the way: `Accept:
text/markdown` comes first on every request, and a `<link rel="alternate"
type="text/markdown">` is followed when the first answer was HTML. Either way the status
line says the text is the author's own. Footnotes are reduced to GFM footnotes, and GitHub
alerts (`> [!NOTE]`) are read as alerts — a quote that says what it is, no coloured box.

## On the disk

History, open tabs and bookmarks are plain text, one line each, tab-separated:

```
~/.local/share/brevier/history.tsv       # Linux, or $XDG_DATA_HOME/brevier
~/.local/share/brevier/session.tsv       # …and the tabs you left open
~/.local/share/brevier/bookmarks.tsv     # …and the pages you kept
~/Library/Application Support/Brevier/   # macOS
%LOCALAPPDATA%\Brevier\                 # Windows
```

Settings (theme, images, the shelf) are in `$XDG_CONFIG_HOME/brevier/settings.tsv`, and a
cache goes to `$XDG_CACHE_HOME/brevier` — three directories, not one profile, because you
carry the first with you, edit the second by hand, and throw the third away without
looking. `BREVIER_DATA_DIR` moves all of them at once.

Text rather than a database: SQLite would mean a C library in a program that sells memory
safety, and a reading history is thousands of lines, not millions. The file is yours —
delete a line to forget a page, delete the file to forget everything, or use the button in
Settings that empties the journal and leaves bookmarks and tabs alone. Only the window
writes there; `brevier <url> | less` is a pipe tool and keeps no history. The reading place
in the session is an offset in the text, not in pixels, so a tab comes back where you left
it through a different window size, zoom or font.

## On the network

- **Not a crawler.** Brevier fetches the page you opened and what it takes to read it — its
  images, an alternate Markdown copy when offered, a dozen probes on a repository's CDN for
  where its documentation starts. It does not walk a site or fetch pages nobody asked for.
- **The User-Agent is honest** — `Brevier/0.1`. Chosen by measurement, not principle: on our
  corpus a browser-shaped UA lost 7:0, every case a 403 from an anti-bot.
- **TLS trust is delegated to the operating system** (`rustls` + `rustls-platform-verifier`).
  No bundled root store, no way to skip verification — a site signed by a CA your system
  does not know will not open until that root is installed, exactly as `curl` behaves.

## Contributing

Patches and bug reports are welcome. What is planned is in [ROADMAP.md](ROADMAP.md) and the
issues labelled `roadmap`; smaller items carry `bug` or `enhancement`. `cargo test` and
`cargo clippy --all-targets --features ui` should be clean, and any change to extraction or
conversion needs a corpus run diffed against `corpus/expected/` before it is committed —
those files exist to catch regressions, and they already have twice.

## License

MIT OR Apache-2.0, at your option.

The bundled fonts are under the SIL Open Font License; `assets/fonts/OFL-NotoSans.txt` must
travel with any distribution — a condition of the OFL, separate from the license on the
code.
