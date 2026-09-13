# Brevier — a manifesto

The web has split in two. One half is applications: mail, maps, shops, anything with
a login and a button. The other half is documents: articles, documentation, threads —
the long text somebody wrote for somebody else to read. The browsers we have were
built for the first half and carry the second along as an afterthought: the text
arrives wrapped in the site's type, the site's colours, the site's scripts and the
site's advertising, and the reader takes what is given.

Brevier is for the second half only. These are its commitments.

## 1. The reader sets the type

Measure, leading, size, face, paper — chosen once, by the person reading, and the
same on every site. A site's stylesheet is an opinion about how its text should look.
Here it is not loaded. A page from a newspaper, a page from a wiki and a README out of
a repository are set in the same type, because they are the same thing: text, being
read.

Today the choice is Brevier's defaults plus zoom and the theme; a settings page for
face, size and measure is on the roadmap. The commitment is ahead of the code, and
this sentence says so.

## 2. A page is a document, not a program

Nothing on a page is executed. Not "scripts off by default" — no engine at all, so
there is nothing to enable, nothing to exploit, and nothing that changes the text
after it arrived. What a document cannot say without running a program, Brevier does
not say: it degrades honestly and offers the way out to a browser that runs programs.

## 3. The author's words, and only those

What is shown is what the author wrote, reduced to Markdown: the text, its headings,
its lists, its links, its images and their captions. Nothing is added — no summary,
no rewriting, no sentence the author did not put there — and nothing is invented to
fill a gap. Where extraction fails the status line says so instead of pretending.

## 4. On your machine, and the same every time

The reduction happens on the reader's computer, and it is deterministic: the same
page gives the same bytes. That is why the project keeps its expected outputs in a
directory and diffs them before every change. A reader that produces something
different each time cannot be tested, and a reader that cannot be tested rots.

## 5. What you read is a file you own

Markdown is the internal representation, so saving is nearly free: the article on the
screen and the file on the disk are the same thing. History, bookmarks and open tabs
are plain text, one line each, in a directory you can open, edit, carry and delete.
Nothing lives in an account. There is no account.

## 6. Honest

The User-Agent says `Brevier`. The numbers in the README are the project's own
regression figures, and they say how they were relaxed. A page that is a list of
links is shown as a list of links. What the program does not do is listed under its
own heading, including the parts that hurt.

## 7. One page per request from a human

Brevier does not crawl, does not prefetch and does not fan out over a site. Every
request is one person opening one page — the pattern the web was built for, and the
one it now treats worst. The one exception is named: opening a repository probes a
dozen well-known paths on the hosting's CDN to find where its documentation starts —
a dozen small requests for one page, against a CDN with no limit, never against the
site's API.

## 8. Not a replacement for your browser

The other half of the web stays where it is. `Ctrl+O` hands any page to the browser
you already have; a companion extension, when it exists, will hand one back, as that
browser rendered it. The desktop entry does not make Brevier the default unless you
ask it to. A reading instrument that tried to be a browser would be a worse browser
and a worse instrument.

## The web we ask for

None of the above needs anything from a site: the point of Brevier is to read the web
as it is. But some pages read better than others, and the difference is rarely money
or effort — it is a handful of habits.

- **The text is in the HTML.** A page whose words arrive only after a script has run
  has, for a reader without scripts, no words.
- **Headings mean something.** One `h1`, the rest in order, each saying what the
  section is rather than how large the type should be.
- **Things are what they say they are.** An article is an `<article>`, a paragraph
  is a `<p>`, a figure is a `<figure>` with its caption, code is `<pre><code>` with
  its language named, a table is a `<table>`.
- **Images have an address in `src` and a description in `alt`** — not an address
  hidden in a `data-` attribute for a script to move later.
- **The page says what language it is in**, so it can be hyphenated the right way.
- **A client that identifies itself and runs no scripts is not a crawler.** Do not
  send it a 403.
- **Serve Markdown to whoever asks for it.** Sites have begun doing this, for reasons
  of their own. Brevier already accepts `text/markdown` and reads it
  with no extraction in the way; asking for it first and following
  `<link rel="alternate" type="text/markdown">` are on the roadmap. A site that
  answers is read exactly.

`brevier --check <url>`, on the roadmap, will score a page against this list, 0 to
100, and say what to change and what each thing cost. It is a ruler, not a rule: it
measures what a reading client can measure, and it does not know whether the text is
any good.

## What this is not

Not a converter — the Markdown is a means; the window is the product. Not a privacy
product — a site sees a request like any other. Not a reader for applications or
video; a page behind a login reaches Brevier only through the browser that is logged
in, never through a credential Brevier holds. Not finished: extraction breaks as
sites change their markup, and keeping it working is the permanent background of the
project, not a milestone on the way to something else.
