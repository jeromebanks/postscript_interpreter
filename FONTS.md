# FONTS.md — font architecture (Stage 6; catalog addendum, Stage 18)

The `ROADMAP.md` Stage 6 item 1 writeup: how fonts plug into the object
model, the execution machine, and the rendering pipeline. Like
`ARCHITECTURE.md`, this documents the decisions that are expensive to
reverse; cheap-to-change details are marked as deferrals.

## The shape of the problem

PostScript fonts are *dictionaries*. Programs do `/Helvetica findfont
12 scalefont setfont`, but they also do `font /FontMatrix get`, copy
font dicts, swap `/Encoding` arrays wholesale, and define brand-new
fonts whose glyphs are PostScript procedures (`Type 3`). So a font
cannot be an opaque Rust value: **the font dict the program sees must be
a real `Dict`**, and the interpreter must treat its contents as the
source of truth wherever the PLRM says programs may replace them
(`Encoding`, most of all).

At the same time, glyph *outlines* for the standard names come from
font files, which are a Rust-side parsing concern the object model
should never see. The design problem is the seam between those two
worlds.

## Decision 1 — outline source: bundled Liberation TTFs via `ttf-parser`

Per the roadmap: bundled open fonts mapped to the standard names is v1;
Type 1 parsing is its own later line item (Stage 6 task 6).

- **Liberation Sans / Serif / Mono** (SIL OFL 1.1) are *metrically
  compatible* with Helvetica, Times, and Courier — the same advance
  widths, which is what layout code depends on. Twelve faces cover the
  regular/bold/italic/bold-italic variants of all three families —
   12 of the standard 13 base fonts.
- The TTFs live in `fonts/` in the repo (~2.8 MB total, license file
  alongside) and are compiled in with `include_bytes!`, so the binary
  stays self-contained — no runtime font discovery, no system font
  dependency, reproducible rendering everywhere.
- **`ttf-parser`** does the parsing: pure Rust, zero-copy, no
  rasterizer (we already have one), outlines delivered through an
  `OutlineBuilder` callback that maps 1:1 onto our path segment types.
- **Symbol has no metric-compatible open TTF**; it is *not* mapped in
  v1. `findfont` on `/Symbol` (or any unknown name) substitutes
  Liberation Sans rather than raising `invalidfont` — Ghostscript-style
  substitution keeps found files running, which is this project's
  priority. Documented deviation; a real Symbol source (URW Standard
  Symbols, Type 1) can ride in with the Type 1 work. *(Superseded in
  Stage 18: the runtime catalog maps `/Symbol` and `/ZapfDingbats` to
  the URW OTFs — see the addendum below.)*

## Decision 2 — the font dict / Rust seam is `FID`

`definefont` in real PostScript installs a `FID` (fontID-type) entry in
the dict. We use the same slot as our seam:

- A **font registry** owns the Rust-side glyph sources. v1: a static
  table of the 12 bundled faces (name, style, `&'static [u8]`).
  `FID` in the font dict is an **integer index into the registry**.
- The registry hands out glyph outlines and advances through one
  interface (`GlyphSource`), so Type 1 (parsed from program data) and
  Type 42 sources slot in later as new registry entry kinds without
  touching `show`.
- `ttf_parser::Face` borrows the byte data it parses, which makes
  storing it alongside owned bytes a self-referential struct. We
  **re-parse the `Face` per `show`/`stringwidth` call** instead —
  parsing is table-directory validation, microseconds — and let the
  Stage 6 glyph-cache task (#7) measure whether a per-glyph path cache
  makes even that irrelevant.

Deviation, documented: `FID` has type `integertype`, not the PLRM's
`fonttype`. Nothing in found code inspects the type; adding a distinct
`Value` variant for it would touch the object model for zero behavior.
If a conformance suite ever cares, that's the escalation path.

## Decision 3 — what `setfont` caches vs. reads live

`setfont` validates the dict and snapshots what the PLRM treats as
fixed at set-time; `show` reads the rest live:

- **Cached in `FontState` at `setfont`:** the dict handle, the `FID`,
  and the composed `FontMatrix` (as an `[f64; 6]`). The PLRM says
  mutating `FontMatrix` after `setfont` has no defined effect, and
  caching it keeps the per-glyph transform arithmetic allocation-free.
- **Read live from the dict at each `show`:** the `/Encoding` array.
  Re-encoding a font — the single most common font-dict surgery in
  found files — therefore just works, even when done after `setfont`.

`FontState` lives in `GraphicsState` (new `font: Option<FontState>`
field), so `gsave`/`grestore` snapshot the current font for free and
`currentfont` is a field read. `FontState` is defined in a new
`src/font.rs`; `gfx.rs` gains an import of it (module-level cycles are
fine within a crate, and `gfx` stays ignorant of everything except the
type's existence).

## Decision 4 — glyphs render through the existing path pipeline

`show` per glyph:

1. byte → `Encoding[byte]` (live dict read) → glyph name.
2. glyph name → `GlyphId`: try the face's `post`/CFF name table
   (`glyph_index_by_name`); fall back to an Adobe Glyph List subset
   (name → Unicode → `cmap`). Missing glyphs render as `.notdef`
   (conventionally blank) and still advance.
3. Outline points go **glyph units → font space → user space →
   device space**: scale by 1000/upem into the Type 1 convention
   (so bundled font dicts carry the familiar
   `FontMatrix [0.001 0 0 0.001 0 0]`), then through the composed
   FontMatrix, then translate to the current point, then the CTM.
   One affine composition per glyph; the outline callback feeds
   device-space segments straight into a `PsPath`.
4. The glyph path is filled with the current color through the normal
   paint path (clip mask included), **without disturbing the current
   path**; the current point advances by the glyph's transformed
   advance vector. Rotated/sheared text falls out of the CTM step;
   `makefont` with a shear matrix falls out of the FontMatrix step.

`stringwidth` runs steps 1–3 accumulating advances only. `charpath`
runs 1–3 but *appends* to the current path instead of painting —
the "text as clipping/stroking geometry" idiom. (v1 treats the
`bool` operand's false/true variants identically — outline fonts have
no separate stroke-path form worth distinguishing; documented.)

## Decision 5 — `FontDirectory`, `findfont`, and the operator set

- `FontDirectory` is a real dict in systemdict. The 12 built-in names
  (plus aliases) are **materialized lazily**: `findfont` on a known
  built-in name builds the font dict on first use (constructing 13×256
  encoding arrays at interpreter startup would be pure waste for
  non-text programs), then installs it in `FontDirectory` so repeated
  `findfont` returns the *same* dict, per the PLRM.
- Built-in font dicts contain: `FontType 42`(TrueType outlines — hidden
  behind the registry either way), `FontName`, `FontMatrix
  [0.001 0 0 0.001 0 0]`, `Encoding` (a fresh 256-element
  StandardEncoding array per font dict — each font must own its
  Encoding so re-encoding one font can't poison another), `FontBBox`,
  and `FID`.
- `scalefont`/`makefont` make a **shallow copy** of the dict (entries
  shared — including the `Encoding` array object, per the PLRM; only
  `FontMatrix` is replaced) and compose the matrix: glyph space → old
  FontMatrix → supplied matrix. `scalefont s` ≡ `makefont
  [s 0 0 s 0 0]`, and is implemented as exactly that. The
  "each font owns its Encoding" rule above is about *distinct built-in
  fonts*; scaled copies of one font sharing their parent's Encoding is
  spec behavior.
- `definefont` adds `FID` (registry passthrough if the dict already
  came from a built-in — the re-encode-and-redefine idiom — or a
  substitute FID otherwise), installs into `FontDirectory`, returns
  the dict. Type 3 dicts (`BuildChar` present, no outline source) are
  accepted and stored but glyphs error `invalidfont` at `show` time
  until task 5 lands.
- Operator set for tasks 2–4: `findfont scalefont makefont setfont
  currentfont definefont selectfont show ashow widthshow awidthshow
  stringwidth charpath` plus the `StandardEncoding` /
  `ISOLatin1Encoding` systemdict arrays. (`selectfont` is Level 2 but
  ubiquitous in found files and one line here.)

## Decision 6 — the show family is an execution-stack frame

*(Task 5 implemented this; tasks 2–4 originally shipped `show` as a
synchronous operator, which the frame subsumed.)*

Every show variant (`show` family, `stringwidth`, `charpath`, `kshow`)
pushes a **`Frame::Show`** onto the execution stack and returns; the
machine then processes **one glyph per step**:

- **Outline glyphs** paint synchronously within a step (the per-glyph
  engine from tasks 2–3, unchanged).
- **Type 3 glyphs** open a *sealed glyph context* — a full
  graphics-state snapshot plus gsave-stack watermark, the CTM set to
  glyph space at the pen, a fresh path — and yield to the font's
  `BuildGlyph` (preferred, gets the encoded name) or `BuildChar` (gets
  the code) as an ordinary procedure frame. When it finishes, the
  context is restored no matter what the procedure did to the graphics
  state, and the pen advances by the `setcachedevice`/`setcharwidth`
  width through FontMatrix and CTM. Unwinding (`stop`, an uncaught
  error) seals open contexts too; `exit` can't cross a show
  (`invalidexit`).
- **`kshow`** yields to its proc between character pairs with both
  codes pushed; the pen is re-read from the current point afterwards,
  so the proc can kern (that's the point) — or even change the font.

Why frames make everything easy: the frame runs to completion *before
the token after the operator*, so `stringwidth` pushing its results at
frame-pop preserves program order exactly; nested shows inside
BuildChar are just frames above frames; and the live window gets
glyph-by-glyph rendering for free.

`stringwidth`/`charpath` on Type 3 fonts execute the glyph procedure
with **painting suppressed** (a counter on `Gfx`, since shows nest) —
that's where a Type 3 width comes from, per the PLRM. `charpath` on
Type 3 advances without contributing outlines (capturing BuildChar's
paints as paths is not attempted; documented deviation).

## What "done" looks like for tasks 2–4

- `(Hello, LaserWriter) show` paints through the live window and
  `--png`, in all 12 faces, at any size/rotation/shear, clipped and
  colored like any other fill.
- `stringwidth` returns Helvetica/Times/Courier-compatible metrics.
- Re-encoding idiom from the PLRM (copy dict, swap `/Encoding`,
  `definefont`) renders the remapped glyphs.
- `charpath` feeds `clip`/`stroke`/`pathbbox`.
- Tests: eval tests for dict plumbing and metrics, pixel tests for
  `show` ink coverage and re-encoding, a golden-style example. The gs
  cross-check compares *coverage*, not glyph shapes — Liberation and
  gs's URW faces are metric-compatible, not shape-identical.

## Deferred, explicitly

- **Type 1** (task 6) — waits on `eexec`/charstring parsing (Stage 7's
  file machinery helps).
- **Glyph cache** (task 7) — measure first; the re-parse-per-show
  choice above is the thing it would obsolete. For Type 3, a cache
  keyed on (font, size, glyph) is what would make `setcachedevice`'s
  bbox meaningful.
- **`cshow`** — trivial atop the ShowFrame now; take it with Type 0
  work if ever.
- **`charpath` capturing Type 3 outlines** — see Decision 6.
- **FontID as a distinct object type** — see Decision 2.
- **Symbol** — substituted, not mapped; revisit with Type 1.
- **`rootfont`/composite (Type 0) fonts, CID fonts** — Level 2/3
  machinery, far beyond found-file needs.

## Stage 18 addendum — the runtime catalog

Stage 18 widened the registry without touching the seam: `findfont`
still hands `show` an `FID`, but the integer space now extends past
the twelve builtins into a **runtime catalog** loaded from
`fonts/catalog/` on first use.

- **Resolution order** (`font::resolve`): builtin names → an alias
  table (the rest of the standard 35 — Palatino, Bookman,
  NewCenturySchlbk, AvantGarde, ZapfChancery, Helvetica-Narrow,
  Symbol, ZapfDingbats — plus a few shorthands like `/Garamond`) →
  a case-insensitive file-stem scan of `fonts/catalog/*/`, with a
  `-Regular` fallback so a bare family name means its regular cut →
  the Stage 6 substitution, unchanged. wasm builds have no
  filesystem and stop at the builtins.
- **Never compiled in.** Catalog bytes are read at findfont time,
  leaked (`Box::leak`) so the parsed `Face<'static>` matches the
  builtins' shape, and registered once per process — bounded,
  process-lifetime, the systemdict-cycle doctrine. The binary and
  the ~5.5 MB wasm are unchanged by however many faces the catalog
  holds.
- **CFF just works**: the TeX Gyre and URW faces are CFF-flavored
  OTFs; `ttf-parser` outlines them through the same
  `outline_glyph`/`PathSink` pipeline as the builtins' glyf tables.
- **Encodings**: catalog dicts get StandardEncoding except
  StandardSymbolsPS and D050000L, which carry SymbolEncoding and
  DingbatsEncoding (dumped from Ghostscript into
  `src/encodings.rs`) so `(a)` in `/Symbol` is α, per the PLRM.
- **Discovery**: `$PSCAT_ROOT`, then exe-relative, then the
  build-time checkout, then the cwd (`font::catalog_root`).
  `pscat --fonts` lists everything reachable.

`fonts/catalog/README.md` is the curated manifest with licenses;
`examples/font_catalog.ps` renders the specimen sheets. The library
of *programmatic* faces lives on in `lib/fonts/` (seven now: Neon,
Marquee, Constellation, Lapidary, Circuitry, Stitchwork, Confetti).

## Post-roadmap addendum — Unicode-mode catalog faces (Korean, Japanese, Thai)

Every face above — builtins and catalog alike — is byte-oriented:
`show` reads one byte, looks it up in a 256-entry `Encoding` array to
get a glyph name, and resolves that name to a glyph id. That ceiling
is structural, not incidental (`ShowCtx`'s codes were, until now,
literally the string's raw bytes). Thai's ~90 codepoints fit under it;
Hangul (11,172 precomposed syllables) and Japanese kanji (thousands)
do not, by two orders of magnitude. Real, general support for those
scripts is Type 0/composite/CID fonts — still explicitly out of scope
(Decision 5, Deferred list above).

Instead, three catalog faces — **Noto Sans KR, Noto Sans JP, Noto Sans
Thai** (SIL OFL 1.1, `google/fonts`) — get a **documented, narrow
deviation**: a new `CatalogEncoding::Unicode` variant (`src/font.rs`),
assigned by file stem in `load_catalog_face` alongside the existing
Symbol/Dingbats special cases. When the font in effect at `begin_show`
is Unicode-mode (`font::is_unicode_font`, keyed on `FID` — so derived
dicts from `scalefont`/`makefont`/the re-encoding idiom stay
Unicode-mode too, since FID is the seam per Decision 2), `ShowCtx`
decodes the string as UTF-8 into Unicode scalars instead of raw bytes,
and each scalar maps straight to a glyph via the face's `cmap`
(`unicode_glyph`), skipping Encoding/glyph-name resolution entirely.
Byte-mode fonts are untouched — same bytes, same `Encoding` reads,
same `outline_glyph` path (the two now share the actual paint/measure
math via `paint_resolved_glyph`; only glyph-id resolution differs).

Consequences, by design:

- **`(가나다) show` just works** in a UTF-8-saved `.ps` file on a
  Unicode-mode font — no per-document byte-remapping, no `uniXXXX`
  Encoding surgery. Latin text in the same string works too (Noto Sans
  covers it), so scripts can mix within one `show` call as long as the
  one active font covers all of them.
- **`/Encoding` is vestigial on these three dicts.** It's still a
  well-formed `StandardEncoding` array (dict-shape consistency,
  `dup length dict copy` idioms keep working structurally), but `show`
  never reads it for a Unicode-mode font — the classic re-encoding
  idiom (copy dict, swap `/Encoding`, `definefont`) has no effect here.
- **`kshow`'s proc** now receives the actual Unicode scalars of the
  surrounding pair, not byte-truncated values — strictly additive for
  existing byte-mode text (values under 256 are unaffected).
  `widthshow`/`awidthshow`'s `char` operand stays capped at `u8`
  (`pop_char_code`, `src/ops/font.rs`): its one real use — widening
  space (code 32) to justify a line — is well under 256 regardless of
  font, so there was no case for admitting codepoints past 255 there.
- **No shaping.** Glyph advance is still the bare `hor_advance` used
  everywhere else — no GPOS/mark-attachment. Thai's above/below
  combining vowel and tone marks stack using their own advance widths,
  not shaped attachment; visually adequate for short strings, not a
  substitute for a real shaper.
- **Variable-font pinning.** Noto Sans KR/JP ship only as variable
  TTFs whose *default* named instance is Thin (wght 100), not Regular
  — confirmed via the `fvar` table, not assumed. `load_catalog_face`
  pins `wght` to 400 (clamped to the face's axis range) for any
  variable-font catalog entry, and uses the file stem as `FontName`
  for Unicode-mode faces rather than the name table's PostScript name
  (which stays "…-Thin" regardless of the pinned instance — it's a
  static string, not recomputed from variation coordinates). This is a
  general policy, not special-cased to today's three files: any future
  variable-font catalog addition gets the same Regular default.
- **`.ttc` still isn't a recognized catalog extension** (`.ttf`/`.otf`
  only) — irrelevant to what's bundled today, but matters if a future
  addition ships as a TrueType Collection.

Tests: `tests/catalog.rs`'s Unicode-mode section — resolution by file
stem, ink-coverage rendering for Hangul/kana-kanji/Thai, `stringwidth`
scaling, ASCII+Hangul mixed in one string, and a `kshow` test asserting
the proc sees Unicode scalars rather than byte-truncated codes. The
existing byte-mode suite (this file's tasks 2–4, Stage 18's addendum)
is the regression gate — it must keep passing unchanged, since it's
what would catch a mistake that let UTF-8 decoding leak into
StandardEncoding/ISOLatin1Encoding text.

`examples/international_text.ps` is the specimen for this addendum.
