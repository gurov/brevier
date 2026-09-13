# Russian hyphenation dictionary

`ru.standard.bincode` is the Russian `Standard` hyphenation dictionary as shipped
by the [`hyphenation`](https://crates.io/crates/hyphenation) crate (v0.8.4),
copied here verbatim from its `dictionaries/` directory.

**Why bundled, not embedded via a feature.** The crate embeds either US English
alone (`embed_en-us`, ~89 KB) or every language it knows (`embed_all`, ~2.2 MB).
Brevier needs only English and Russian, so it embeds English through the feature
and loads this one file at runtime with `Standard::from_reader`. That is 131 KB
instead of 2.2 MB in `brevier-ui`, for the same result — the same reason the
program ships its own fonts rather than trust the machine to have them.

Adding a language later means adding its `<lang>.standard.bincode` here the same
way (or switching to `embed_all`).

**License.** The `hyphenation` crate is MIT OR Apache-2.0, and its bundled
dictionaries are distributed under that license — compatible with Brevier's own
MIT OR Apache-2.0. The dictionaries are built from the TeX `hyph-utf8` patterns.
