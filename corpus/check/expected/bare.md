# Check

`https://check.example/bare`

**Score 65 / 100.**

100 − 8 (structure-h1) − 8 (structure-landmark) − 6 (structure-lang) − 4 (structure-title) − 4 (structure-byline) − 2 (extras-feed) − 3 (extras-alt-markdown) = 65

## Access

Not fetched — the HTML came from stdin, so access, redirects and content type were not measured.

## Text

Nothing to fix.

## Structure

- **−8 · structure-h1** — no `<h1>` on the page. Give the page exactly one `<h1>` — its title, once.
- **−8 · structure-landmark** — neither `<article>` nor `<main>` is present. Wrap the article in `<article>` or `<main>` so its body is unambiguous.
- **−6 · structure-lang** — no `lang` on `<html>`. Declare the page's language with `lang` on `<html>`, so it can be hyphenated the right way.
- **−4 · structure-title** — no `<title>`. Give the page a `<title>`: it names the tab, the saved file and the history line.
- **−4 · structure-byline** — no author was found. Mark the author where the extractor can find it (`rel=author`, or an `<address>` in the article).

## Extras

- **−2 · extras-feed** — no feed advertised in `<head>`. Advertise a feed in `<head>` (`<link rel=alternate type=application/rss+xml>`) so it can be followed.
- **−3 · extras-alt-markdown** — no `<link rel=alternate type=text/markdown>`. Offer `<link rel=alternate type=text/markdown>`: a reader then takes the exact text, with no extraction in the way.

## What the reader will see

```
The measure of a line is the length a reader's eye can travel and still find the start of the next line without effort. This page wraps it in nothing at all: no heading, no article, no language, no title. It reads, but a client has to guess at every one of them, and a guess is what a reader mode should never have to make.
```


## The weights

Every check and what it costs. Argue with a number here, not with a hidden formula.

| check | stage | cost |
|---|---|---:|
| access-forbidden | Access | caps at 0 |
| access-http | Access | caps at 0 |
| access-cert | Access | caps at 0 |
| access-unreachable | Access | caps at 0 |
| access-content-type | Access | caps at 0 |
| access-too-large | Access | caps at 0 |
| text-empty | Text | caps at 10 |
| text-script-only | Text | caps at 10 |
| text-noise | Text | −6 |
| text-lazy-images | Text | −4 |
| structure-h1 | Structure | −8 |
| structure-heading-order | Structure | −5 |
| structure-landmark | Structure | −8 |
| structure-paragraphs | Structure | −5 |
| structure-code-lang | Structure | −4 |
| structure-lang | Structure | −6 |
| structure-title | Structure | −4 |
| structure-title-h1 | Structure | −3 |
| structure-byline | Structure | −4 |
| extras-alt | Extras | −4 |
| extras-feed | Extras | −2 |
| extras-alt-markdown | Extras | −3 |