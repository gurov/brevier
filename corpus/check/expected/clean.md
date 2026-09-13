# Check

`https://check.example/clean`

**Score 100 / 100.** Nothing stands between this page and a clean read.

## Access

Not fetched — the HTML came from stdin, so access, redirects and content type were not measured.

## Text

Nothing to fix.

## Structure

Nothing to fix.

## Extras

Nothing to fix.

## What the reader will see

````
# Why measure matters

A. Writer

The measure of a line is the length a reader's eye can travel and still find the start of the next line without effort. Typographers settled on roughly sixty-five characters, and screens have not changed the eye.

## A short history

A second paragraph, long enough to be prose and not a caption, so the extractor is in no doubt that this page carries an article worth keeping.

![A line of text sixty-five characters wide.](https://check.example/diagram.png)

The measure, drawn.

```
let measure = 65; // characters
```
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