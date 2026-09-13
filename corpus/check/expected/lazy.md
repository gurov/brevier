# Check

`https://check.example/lazy`

**Score 87 / 100.**

100 − 4 (text-lazy-images) − 4 (structure-byline) − 2 (extras-feed) − 3 (extras-alt-markdown) = 87

## Access

Not fetched — the HTML came from stdin, so access, redirects and content type were not measured.

## Text

- **−4 · text-lazy-images** — 3 images carry their address in a `data-` attribute, not `src`. Give images a real `src`: an address parked in a `data-` attribute for a script to move never loads here.

## Structure

- **−4 · structure-byline** — no author was found. Mark the author where the extractor can find it (`rel=author`, or an `<address>` in the article).

## Extras

- **−2 · extras-feed** — no feed advertised in `<head>`. Advertise a feed in `<head>` (`<link rel=alternate type=application/rss+xml>`) so it can be followed.
- **−3 · extras-alt-markdown** — no `<link rel=alternate type=text/markdown>`. Offer `<link rel=alternate type=text/markdown>`: a reader then takes the exact text, with no extraction in the way.

## What the reader will see

```
# A gallery

An article with real prose above the pictures, wide enough that the extractor keeps it and does not mistake the page for a bare list of links to elsewhere.

![First photo](https://check.example/photo-1.jpg) ![Second photo](https://check.example/photo-2.jpg) ![Third photo](https://check.example/photo-3.jpg)
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