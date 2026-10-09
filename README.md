<p align="center">
  <img src="assets/brevier.svg" alt="" width="122">
</p>

# Brevier

A browser for reading. Brevier downloads a page, turns it into clean text on your own
computer, and sets it in the typography you chose — not the one the site chose.

- **Articles** without scripts, site styles, pop-ups or ads.
- **Documentation** straight from git repositories: `gh:owner/repo` opens its README.
- **Feeds** — RSS, Atom and JSON Feed — as lists of links.
- **Search** from the address bar.

There is no JavaScript engine and no site CSS. The site decides the words; you decide how
they look. Inside, every page becomes Markdown, made with existing libraries
(`dom_smoothie`, `htmd`) — but the Markdown is plumbing, not the product. The product is
the reading window.

Why it is built this way is in the [manifesto](MANIFESTO.md); what comes next is in the
[roadmap](ROADMAP.md).

**Status: early.** Brevier runs on Linux, as a GTK window, and on Android, as a native app
over the same core. Builds are in [Releases](https://github.com/gurov/brevier/releases/latest);
nothing is in a store yet. Screen readers are known to work on Linux only (see
[What it does not do](#what-it-does-not-do)).

![An article in Brevier: an essay on keyboard latency set on ivory paper in the reader's
own measure, hyphenated, its quotes ruled at the left; the shelf at the right lists the
page's contents with the section being read marked, and the site's own links under
them](assets/screenshot-article.png)

## Install

**Linux, Flatpak.** One file. The runtime brings GTK, so any distribution will do:

```sh
wget https://github.com/gurov/brevier/releases/download/v0.6.0/brevier.flatpak
flatpak install --user ./brevier.flatpak
flatpak run io.github.gurov.brevier https://example.com/article
```

The first install also downloads `org.gnome.Platform//49` from Flathub if you don't have
it — about 400 MB, shared with other Flatpak apps. In the sandbox your data lives in
`~/.var/app/io.github.gurov.brevier/`, saving goes through the system's file dialog, and
local files (a `.md`, a saved feed) can't be opened.

**Linux, tarball.** For a system that already has GTK 4 (`libgtk-4-1` on Debian and
Ubuntu) and no wish for a sandbox:

```sh
tar xf brevier-0.6.0-x86_64-linux.tar.gz
cd brevier-0.6.0-x86_64-linux && ./install.sh
```

`install.sh` installs into `~/.local`, needs no root, and `--uninstall` removes it again.

**Android** 7.0 or later, 64-bit ARM. Download
[`brevier-0.6.0-arm64.apk`](https://github.com/gurov/brevier/releases/download/v0.6.0/brevier-0.6.0-arm64.apk)
on the phone and open it; Android asks once to allow installs from the app you opened it
with. Updates install over it as long as they are signed with the same release key:

```
30:A4:32:B8:2F:3E:F0:FC:E5:F4:68:1F:D7:F9:FE:20:5F:4B:17:A2:F4:34:19:C2:45:0C:A2:6B:E6:8D:34:80
```

An F-Droid listing is under review.

Every release is built by CI from its tag, and `SHA256SUMS` lists the checksum of each
file. To build from source, see [CONTRIBUTING.md](CONTRIBUTING.md#building).

**Make it your browser.** Brevier registers for `http`, `https` and `feed://` links and
for Markdown and feed files, so it shows up in "Open with". It doesn't make itself the
default; if you want that:

```sh
xdg-settings set default-web-browser io.github.gurov.brevier.desktop
```

`Ctrl+O` still hands any page to your other browser. On Android, Settings has
**Make default**. If the menu shows Brevier without its icon, log out and back in — the
desktop shell cached its icon list before the icon was installed.

## Use

`brevier-ui <url> [<url>…]` opens a window, one tab per address. An address sent while
Brevier is running — from the command line or a link clicked in another app — opens as a
tab in the window you already have. Starting it with no address opens a new window.

The same core works on the command line:

```sh
brevier https://example.com/article     # Markdown on stdout
brevier https://example.com/feed.xml    # an RSS, Atom or JSON feed, as a list of links
brevier gh:rust-lang/book               # a repository's README
brevier gh:rust-lang/book/src           # the README of a directory inside it
brevier gh:rust-lang/book/src/          # …or what the directory holds, listed
brevier gl:owner/repo                   # the same for GitLab
brevier --docs gh:rust-lang/book        # entry points into its documentation
brevier --check <url> [<url>…]          # score pages for a reader without scripts
brevier --links <url>                   # the article's outgoing links
brevier --nav <url>                     # the site's own menu and footer links
brevier --raw <url>                     # no extraction, the whole page
brevier --html <url>                    # the extracted HTML, before conversion
brevier --stdin <url> < page.html       # HTML you already have; the url is
                                        # where it came from, for its links
brevier --save <url>                    # save to a file instead of stdout
brevier brevier:history                 # what you have read, by day
brevier brevier:archive                 # a copy of every article you read
brevier "brevier:archive?q=latency"     # …searched for words
brevier brevier:archive/<host>/<file>   # one copy, printed as `lz4 -d` would
brevier "borrow checker"                # not an address: a search
brevier "?danluu.com"                   # a leading ? searches for anything
```

A GitHub or GitLab file URL, a `feed://` link, a local `.md` file or a saved feed work too.
`--save` writes a `.md`, or a `.zip` with an `images/` folder when the page has pictures;
`-o` sets the file name. Without `-o` it never overwrites an existing file.

Exit codes, for scripts: 1 bad address, 2 network, 3 HTTP status, 4 content type,
5 nothing extracted, 6 conversion (or a broken feed).

A chapter of the Rust book, read straight from the repository
(`gh:rust-lang/book/src/ch03-02-data-types.md`), with the book's documentation on the
shelf:

![The repository mode: a chapter of the Rust book rendered in Brevier, with highlighted
code; the shelf lists the repository's documentation, its contributing guide and its
files, then the chapter's sections](assets/screenshot-repository.png)

### Check your own site

`--check` scores a page from 0 to 100 for a reader that runs no scripts. It says what to
change and how many points each finding cost, and exits with an error below `--min`
(80 by default). In the window the same report is in the menu, **Check this page**.

![The check of example.com inside Brevier: a score of 81 out of 100, the arithmetic behind
it, and the findings by stage, each with what to change](assets/screenshot-check.png)

In CI it is a GitHub Action:

```yaml
- uses: gurov/brevier@v0.6.0   # a release tag, or @main
  with:
    urls: |
      https://example.com/
      https://example.com/docs/getting-started
    min: 80                      # fail the job below this
    badge: brevier-check.svg     # optional: an SVG with the lowest score
```

The reports go to the job summary; the lowest score is the step's `score` output.

## Keys

| | |
|---|---|
| `Enter` | open the address |
| `Space` / `Backspace`, `PageDown` / `PageUp` | page down / up |
| arrows, `Home` / `End` | line, top, bottom |
| `Tab` / `Shift+Tab` | mark the next / previous link |
| `Enter` / `Ctrl+Enter` on a marked link | follow it / open it in a new tab |
| `Ctrl+L` | go to the address bar |
| `Ctrl+R`, `F5` | reload, skipping the saved copy |
| `Ctrl+T` / `Ctrl+W` | new tab / close tab |
| `Ctrl+Tab`, `Ctrl+PageUp` / `PageDown` | switch tabs |
| `Ctrl+F` | find on page |
| `Ctrl+H` | history |
| `Ctrl+Shift+F` | search your archive |
| `Ctrl+D` | bookmark the page, or remove the bookmark |
| `Ctrl++` / `Ctrl+-` / `Ctrl+0`, `Ctrl`+wheel | zoom in / out / reset |
| `Ctrl+S` | save the article |
| `Ctrl+O` | open the page in your other browser |

With the mouse: `Ctrl`+click or middle-click opens a link in a new tab, middle-click on a
tab closes it, and the back and forward buttons go back and forward. Hovering a link shows
where it goes. Settings, history and bookmarks are in the menu at the top right.

## What you get

**Reading**

- **Your typography, the same on every site.** For now that means Brevier's defaults —
  16.5 pt, about 65 characters per line, airy leading, headings lighter than the text —
  plus zoom and a dark theme. Settings for face, size and measure are on the roadmap.
- **Hyphenation by the page's language** (English and Russian are built in); in Russian
  and Czech no line ends on a one-letter word. Copy and find-on-page see the plain text.
- **Ivory paper** instead of glaring white, and a warm dark theme.
- **Zoom (67–200%) scales the whole layout**, so a line still holds about 65 characters.
- **Fonts are built in** — Noto Sans and Noto Sans Mono — so a page looks the same on every
  system.
- **Poems keep their lines and stanzas.**

**Finding your way**

- **The shelf on the right** lists the repository's documentation, the page's contents
  (the section you are reading is marked), the site's feeds and the site's own menu. Drag
  the divider to resize it.
- **Back and forward return to the same spot**, instantly, without loading the page again.
- **Pick up where you stopped.** A page you read for more than ten minutes remembers where
  you were and what you have read. Open it again and the status line offers **Continue
  from 43%**. The shelf shows how much of each section you have read, and a thin bar under
  the text shows how far you are, with a dot for each section.
- **Next page.** A page that names the one after it — a series, a book chapter, a thread's
  next page — ends with a **Next:** line, and `Space` at the very end goes there.
- **A new tab shows the five pages you read last.**
- **Tabs come back** after you close the window; only the visible one loads at start.
- **History and bookmarks**, and the address bar suggests pages you have read.
- **The archive keeps every article you read**, as a compressed Markdown file, so you can
  read and search it after the site has changed or gone: `brevier:archive`, and
  `Ctrl+Shift+F` (Search your archive, in the menu on the phone).

**Any kind of page**

- **A list of links** — a blog's front page, a section of a site — is shown as a list, and
  the status line says so.
- **Feeds** open as lists of entries with date, author and a short summary — from an
  address, a `feed://` link or a file. A feed of comments on one page (a reddit thread,
  WordPress comments) reads as a thread.
- **Repository folders can be listed:** end the address with a slash
  (`gh:owner/repo/docs/`) or use **Files in this directory** on the shelf.
- **Search.** Anything that is not an address goes to DuckDuckGo Lite, and the results
  come back as a list of links. A leading `?` forces a search.
- **Images load right away, and one switch turns them off** — without JavaScript, the
  image decoder is the main thing left to attack.
- **Pages read in the last week open from a copy on disk**, with no download. `Ctrl+R`
  fetches a fresh one. Lists of links are always fetched, since what's new is the point.
- **Sites that serve Markdown are read exactly as written**, with no extraction. Brevier
  speaks CommonMark with GitHub's extensions; footnotes and GitHub alerts (`> [!NOTE]`)
  are kept.
- **Plain `http` pages are marked "Not secure".** There is no padlock: people read it as
  "this site is trustworthy", when it only means "the connection is encrypted".
- **A page that can't be shown says why** and offers **Open in your browser**.

A feed in Brevier — the Rust blog's, `blog.rust-lang.org/feed.xml`:

![The Rust blog's feed in Brevier: each entry's title as a link, its date and author in
italics, a few lines of its summary; the status line says it is a list of links, not an
article](assets/screenshot-feed.png)

## On the phone

The Android app is a second front-end over the same core. It draws the article as native
text — not in a WebView — with the same typography and colours as the window.

- Tabs, the shelf, find on page, zoom (from the menu or with a pinch), saving, history,
  bookmarks, settings, the dark theme, and a session that comes back.
- It can be your default browser, and **Open in your browser** still finds the others.
- It opens links, shared text, `feed://` links and Markdown or feed files from other apps.
- A long press on a link opens it in a new tab, copies it, or sends it to your browser.
- A very long page — a whole novel — shows its first screen at once.
- With a keyboard attached, the window's shortcuts work, and `Alt+←` / `Alt+→` go back and
  forward.

The maintainer uses it every day. It hasn't been tested with TalkBack yet.

<p align="center">
  <img src="assets/screenshot-phone-article.png" width="300"
       alt="The same essay on a phone: the text hyphenated in the phone's column, the quotes ruled at the left">
  <img src="assets/screenshot-phone-shelf.png" width="300"
       alt="The phone in the dark theme with the shelf open over the article: the page's contents with the current section marked, the site's links under them">
</p>

## How well it works

On our test set of 108 live pages, **80 read well (74%)**. Of the 89 pages that reached
the extractor, 80 read well (90%); the other 19 were blocked before that — by
certificates, 403/401 responses, or anti-bot walls.

These are our own numbers. The rubric (`corpus/RUBRIC.md`) is ours, the marking is done
by hand, and we relaxed the rubric once after seeing the results; the strict reading gave
56%.

On an outside benchmark (`scrapinghub/article-extraction-benchmark`, 181 pages) the whole
pipeline scores **F1 0.929**, and extraction alone 0.951 — next to 0.947 for Readability.js
and 0.958 for trafilatura in the same table. That set is news; Brevier is aimed at long
reads, documentation and threads.

Extraction breaks as sites change their markup. Keeping up with that never ends.

## What it does not do

- **JavaScript** — never, in any form.
- **Site CSS** — never; the typography is yours.
- **Forms, logins, cookies** — no. Here the web is read-only, on purpose.
- **Single-page apps** — they degrade honestly, and **Open in your browser** is one click
  away.
- **Video, audio, extensions** — no.
- **An AI model inside** — no. It would add hundreds of megabytes and seconds per page,
  make the output unpredictable, and risk showing you words the author never wrote.
- **Screen readers outside Linux** — GTK's accessibility works only on Linux, which is one
  reason there are no Windows or macOS builds yet. The Android app uses native text, which
  TalkBack should read, but that hasn't been tested.
- **Privacy** — not promised. Sites can track you as they always could.
- **Tables in find and copy** — a table is drawn as a grid of widgets, so find-on-page and
  "copy all" don't see it. The saved Markdown has it in full.

## Your data

History, open tabs and bookmarks are plain text, one line per entry:

```
~/.local/share/brevier/history.tsv     # what you have read
~/.local/share/brevier/session.tsv     # the tabs you left open
~/.local/share/brevier/bookmarks.tsv   # the pages you kept
~/.local/share/brevier/archive/        # a copy of every article you read
~/.local/share/brevier/reading.tsv     # where you stopped in long pages
```

The archive holds one file per page, `<host>/<date>-<title>.md.lz4`: Markdown with a short
header (title, address, when you read it), compressed in the standard LZ4 format, so
`lz4 -d` opens it without Brevier. A copy stays as long as its line in the history, and a
bookmarked page keeps its latest copy for good. A switch in Settings stops new copies.

Settings are in `~/.config/brevier/settings.tsv`. The cache — the fonts, and week-old
copies of pages (up to 64 MB of text and 256 MB of images) — is in `~/.cache/brevier`.
The usual `XDG_*` variables move these folders, and `BREVIER_DATA_DIR` moves them all.
On Android they live in the app's own storage.

There is no database: delete a line to forget a page. **Forget everything** in Settings
clears the history, the saved copies and the archive, and keeps your bookmarks and tabs. Only the
window writes these files; `brevier <url>` on the command line keeps no history.

## On the network

- **Not a crawler.** Brevier fetches the page you opened and what it needs to show it: its
  images, a Markdown version if the site offers one, a few probes to find a repository's
  documentation. It never walks a site.
- **Markdown first.** Every request asks for `text/markdown` before HTML, and
  `<link rel="alternate" type="text/markdown">` is followed when a page has one.
- **Very few per-site rules.** reddit is read through its `.rss` feeds (its pages are empty
  without JavaScript); royallib's books open part by part; DuckDuckGo Lite results are
  unwrapped from its click tracker. Your search queries go to DuckDuckGo.
- **An honest User-Agent:** `Brevier/0.1`. On our test set, pretending to be a browser lost
  seven pages to anti-bot 403s and won none.
- **TLS trust comes from your operating system** (`rustls` with
  `rustls-platform-verifier`). There are no bundled certificates and no way to skip the
  check. On Android, revoked certificates are refused too, and so is a site whose
  revocation list can't be fetched.

## Contributing

Bug reports and patches are welcome. How the code is laid out, how to build it and what a
change needs before it is merged are in [CONTRIBUTING.md](CONTRIBUTING.md); the plans are
in the [roadmap](ROADMAP.md) and in the issues labelled `roadmap`.

## License

MIT OR Apache-2.0, at your option.

The bundled fonts are under the SIL Open Font License; `assets/fonts/OFL-NotoSans.txt`
must travel with any copy you distribute.
