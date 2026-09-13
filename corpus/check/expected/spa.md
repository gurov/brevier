# Check

`https://check.example/spa`

**Score 10 / 100.** The page cannot be read as it stands: the text is absent until a script runs.

Even served, the markup would bring it to 79.

## Access

Not fetched — the HTML came from stdin, so access, redirects and content type were not measured.

## Text

- **caps at 10 · text-script-only** — the text is absent until a script runs. Put the words in the HTML: they arrive here only after a script runs, and this reader runs none.

## Structure

- **−8 · structure-h1** — no `<h1>` on the page. Give the page exactly one `<h1>` — its title, once.
- **−8 · structure-landmark** — neither `<article>` nor `<main>` is present. Wrap the article in `<article>` or `<main>` so its body is unambiguous.

## Extras

- **−2 · extras-feed** — no feed advertised in `<head>`. Advertise a feed in `<head>` (`<link rel=alternate type=application/rss+xml>`) so it can be followed.
- **−3 · extras-alt-markdown** — no `<link rel=alternate type=text/markdown>`. Offer `<link rel=alternate type=text/markdown>`: a reader then takes the exact text, with no extraction in the way.

## What the reader will see

Nothing was extracted.

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