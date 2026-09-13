# Check

`https://check.example/flawed`

**Score 64 / 100.**

100 − 5 (structure-heading-order) − 8 (structure-landmark) − 4 (structure-code-lang) − 6 (structure-lang) − 4 (structure-byline) − 4 (extras-alt) − 2 (extras-feed) − 3 (extras-alt-markdown) = 64

## Access

Not fetched — the HTML came from stdin, so access, redirects and content type were not measured.

## Text

Nothing to fix.

## Structure

- **−5 · structure-heading-order** — a heading jumps from h1 to h3. Let heading levels descend without skipping, so the outline is the author's and not the type size's.
- **−8 · structure-landmark** — neither `<article>` nor `<main>` is present. Wrap the article in `<article>` or `<main>` so its body is unambiguous.
- **−4 · structure-code-lang** — a code block names no language. Name the language on `<pre><code class="language-…">`, so code is set and coloured as code.
- **−6 · structure-lang** — no `lang` on `<html>`. Declare the page's language with `lang` on `<html>`, so it can be hyphenated the right way.
- **−4 · structure-byline** — no author was found. Mark the author where the extractor can find it (`rel=author`, or an `<address>` in the article).

## Extras

- **−4 · extras-alt** — the only image has no alt text. Describe images in `alt`; with images off, a reader sees the description in the frame.
- **−2 · extras-feed** — no feed advertised in `<head>`. Advertise a feed in `<head>` (`<link rel=alternate type=application/rss+xml>`) so it can be followed.
- **−3 · extras-alt-markdown** — no `<link rel=alternate type=text/markdown>`. Offer `<link rel=alternate type=text/markdown>`: a reader then takes the exact text, with no extraction in the way.

## What the reader will see

````
# Why measure matters — The Typography Blog

### A short history

The measure of a line is the length a reader's eye can travel and still find the start of the next line without effort. Typographers settled on roughly sixty-five characters a long time ago, and screens have not changed the eye.

A second paragraph, long enough to be prose and not a caption, so that the extractor is in no doubt that this page carries an article worth keeping.

```
let measure = 65; // characters
```

![](https://check.example/diagram.png)

Follow us on every social network we could find, and subscribe to the newsletter, and rate this article, and consider our premium membership tier today.
````


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