# Check

`https://check.example/markup`

**Score 86 / 100.**

100 − 5 (text-image-headings) − 3 (structure-tables) − 3 (structure-captions) − 3 (extras-alt-markdown) = 86

## Access

Not fetched — the HTML came from stdin, so access, redirects and content type were not measured.

## Text

- **−5 · text-image-headings** — a heading is an image with no text in it. Set headings in text, not as pictures of text: words in an image cannot be hyphenated, searched, copied or read aloud.

## Structure

- **−3 · structure-tables** — a table is built from elements with `role=table`, not `<table>`. Make tables `<table>`: a grid of `<div>`s is a table only on screen, and a reader gets a pile of cells.
- **−3 · structure-captions** — an image caption is not a `<figcaption>`. Put an image and its caption in `<figure>` with `<figcaption>`, so the caption travels with the image.

## Extras

- **−3 · extras-alt-markdown** — no `<link rel=alternate type=text/markdown>`. Answer `Accept: text/markdown`, or offer `<link rel=alternate type=text/markdown>`: a reader then takes the exact text, with no extraction in the way.

## What the reader will see

```
# Harbours of the north

A. Writer

25 September 2026

A harbour is a sentence the coast writes about the sea: where the water is calm enough to stop, and deep enough to arrive. This page sets its parts the way a script-free reader cannot follow.

## ![Tides and moorings](https://check.example/tides.png)

The section above has a heading, but its words are a picture: a reader can show the picture, and cannot hyphenate, search or copy what it says.

![Boats moored in a stone harbour at dawn.](https://check.example/harbour.jpg)

Stone harbour at dawn, before the fleet goes out.

Harbour

Depth

North quay

Fish dock
…
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
| access-too-deep | Access | caps at 0 |
| access-redirects | Access | −3 |
| text-empty | Text | caps at 10 |
| text-script-only | Text | caps at 10 |
| text-noise | Text | −6 |
| text-lazy-images | Text | −4 |
| text-image-headings | Text | −5 |
| structure-h1 | Structure | −8 |
| structure-heading-order | Structure | −5 |
| structure-landmark | Structure | −8 |
| structure-paragraphs | Structure | −5 |
| structure-code-lang | Structure | −4 |
| structure-tables | Structure | −3 |
| structure-captions | Structure | −3 |
| structure-lang | Structure | −6 |
| structure-title | Structure | −4 |
| structure-title-h1 | Structure | −3 |
| structure-byline | Structure | −4 |
| structure-date | Structure | −2 |
| extras-alt | Extras | −4 |
| extras-feed | Extras | −2 |
| extras-alt-markdown | Extras | −3 |