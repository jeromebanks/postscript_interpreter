# CAPABILITIES.md — the machine-readable art catalog

Issue #39. `pscat --capabilities` (CLI) and `describe_art_capabilities`
(the `pscat-mcp` tool) both print the same JSON payload: a structured
inventory of what this build's creative toolkit actually has installed
— fonts, the Type 3 program faces, artkit's mood palettes, the page
templates, and artkit's/the style packs'/handscript's/hangul's major
procedures — so an agent can discover real capabilities instead of
guessing names from prose documentation, which drifts. (It already
had: `psart`'s `SKILL.md` was behind artkit's paragraph-flow,
hyperbolic-geometry, noise/flow, and gradient sections by the time
this catalog was built, and the first draft of this very catalog
omitted the `HandScript`/`HangulScript` Type 3 faces — see NOTES.md's
issue #39 entry, cross-model review on PR #74.)

## Payload shape

```json
{
  "pscat_version": "0.5.1",
  "catalog_signature": "217ad22059b2c0b2",
  "capabilities": [
    {
      "name": "pgcertificate",
      "kind": "template",
      "description": "Formal award: cream background inside a double rule, ...",
      "parameters": [
        { "name": "Title", "description": "Certificate heading", "default": "(Certificate of Achievement)" }
      ],
      "source": "lib/pagekit.ps",
      "load": "(lib/artkit.ps) run (lib/pagekit.ps) run",
      "example": "x y w h << /Title (...) ... >> pgcertificate  leftover",
      "availability": "library"
    }
  ]
}
```

`pscat_version` and `catalog_signature` are both drift signals; an
agent caching a dump should re-fetch when *either* changes.
`pscat_version` only moves on a new binary release. `catalog_signature`
is a hash over every entry's (name, kind, availability) — it also
moves when the *filesystem-backed* font section changes without a
version bump (a different `PSCAT_ROOT`, or a catalog install updated
in place; a cross-model review's finding, PR #74 — `pscat_version`
alone under-signals exactly that case). `capabilities` is sorted by
(name, kind) so two dumps of the same release/install compare
byte-for-byte regardless of directory-scan or hashmap iteration order
upstream.

`kind` is one of `font`, `type3_face`, `palette`, `template`,
`procedure`. `availability` says how durable an entry is:
- `builtin` — compiled into the binary, resolves identically
  everywhere (wasm included).
- `catalog (desktop/bundle only; ...)` — a font-catalog face or an
  alias to one; depends on `fonts/catalog/` being present, so absent
  on wasm and possibly absent on an incomplete install. Three
  cross-model review findings (PR #74) sharpened what counts as
  "installed" here: a stem is listed only if its file is actually
  readable and parses (not merely present with a matching extension —
  a corrupt file doesn't get advertised); a curated `ALIASES` entry is
  listed only if its target file is actually present; and every
  catalog file literally named `<Name>-Regular.<ext>` also gets an
  *implicit* alias for the bare `<Name>` (`catalog_fid`'s own
  `-Regular` fallback makes that name resolve too, even when it's not
  in `ALIASES` — 37 such names on this repo's own catalog, previously
  missing from `--capabilities` entirely).
- `library` — defined in a `.ps` file loaded via `run`; always
  available once that source is loaded, no runtime filesystem
  negotiation the way catalog fonts need.

`load` is the exact `run` sequence needed before `example` will work —
not always just `source` itself: a page template or style-pack
procedure errors `undefined: Palettes` unless `lib/artkit.ps` loads
first, a dependency `source` alone doesn't show (another cross-model
review finding, same PR). Empty for fonts, which need no `run` at all.

`parameters` is populated only where PostScript actually has named,
defaulted arguments — the five page templates, whose content dict has
optional keys with real defaults, and the four handwriting-family
procedures (`hs-write`/`hs-linecount`/`hg-write`/`hg-linecount`),
whose calling convention is likewise one options dict with named keys.
Most procedures take positional stack arguments with no such
structure, so their calling convention lives in `example` instead (the
stack-effect comment already written at the proc's definition site,
e.g. `x0 y0 v1 v2 n1 n2 {x y ...} lattice -`) and `parameters` stays
empty. Fonts and Type 3 faces likewise carry no `parameters`.

## How it's built (`src/capabilities.rs`)

Fonts are the one kind built *dynamically*: `capabilities::catalog()`
calls `font::catalog_entries()` — the same function `--fonts` and
`findfont` resolution are built from — so the font section can't
independently drift from what the interpreter can actually reach.
There's no static font list here to go stale.

Everything else (Type 3 faces, palettes, templates, procedures) is
hand-maintained in `capabilities.rs`'s `ENTRIES` table, because
PostScript has no docstring convention this module could parse for
descriptions or calling conventions — those are prose, written once at
registration time. `load` is the one derived field among these: it's
computed from `(kind, source)` by `load_sequence`, not duplicated per
entry, so a page template and a style-pack procedure both get the
right `(lib/artkit.ps) run ...` prefix from one place.

Two Type 3 faces — `HandScript` (`lib/handscript.ps`, Stage 13) and
`HangulScript` (`lib/hangul.ps`, issue #6) — predate the `lib/fonts/`
convention established at Stage 15 and live one level up instead;
they're cataloged the same as the other seven, just named explicitly
rather than found by the `lib/fonts/` directory scan (see
`tests/capabilities.rs`'s comment on why the scan isn't just widened
to all of `lib/*.ps`).

## What keeps the hand-maintained part honest

`tests/capabilities.rs` actually loads each `.ps` source into a real
`Interp` and checks the name set both ways:

- **Forward** — every name `ENTRIES` claims exists (a palette in
  `Palettes`, a template/procedure in `userdict`, a Type 3 face in
  `FontDirectory`) really is defined there after loading its source.
  A rename or removal in the `.ps` file fails this immediately.
- **Reverse** — every top-level name a source file actually defines is
  accounted for: either cataloged in `ENTRIES`, or listed in one of the
  `capabilities::*_INTERNAL` allowlists (`ARTKIT_INTERNAL`,
  `PAGEKIT_INTERNAL`, `HANDSCRIPT_INTERNAL`, `HANGUL_INTERNAL` —
  scratch helpers and internal state, like `apseg`/`tfdrawline`/
  `Palettes` the dict itself for artkit, or the `definefont` template
  dicts `HandScriptDict`/`HangulDict` and layout-state dicts
  `HSLayout`/`HGLayout` for the two handwriting families — real names,
  not part of the public API). For Type 3 faces specifically, the
  reverse check is a directory scan of `lib/fonts/*.ps` (plus the two
  named historical outliers) rather than a `userdict` diff, since a
  face's own name lives in `FontDirectory`, not `userdict`. A newly
  added public palette, procedure, or face that nobody registered
  fails this instead of just being silently missing from
  `--capabilities` — confirmed for real on this catalog's first PR: a
  cross-model review found `HandScript` missing from the Type 3 list
  (the reverse check for that kind was still a hardcoded count at the
  time, not yet an actual directory comparison — fixed alongside).

The reverse direction is what a purely forward-checking test would
miss: nothing stops someone from adding `Palettes /copper [...] put`
to a style pack and forgetting to catalog it. `userdict { pop } forall`
(procedures/templates) and `Palettes { pop } forall` (palettes),
called through `Interp::run_str`, are the mechanism — each leaves
every key on the operand stack in one pass, read back in Rust as a
set and compared.

## Registering a new capability

1. Add the code to its `.ps` file as normal.
2. Add an `Entry` to `capabilities.rs`'s `ENTRIES`, in the section
   matching its source file and kind — name, one-line description, the
   stack-effect comment as `example` (or, for a template or an
   options-dict procedure like `hs-write`, real `Param`s with
   defaults), the source path, and `availability` (almost always `LIB`
   for anything in `lib/`). `load` needs no per-entry work — it's
   derived from `(kind, source)` — unless the new source is a genuinely
   new prerequisite chain, in which case add a branch to
   `load_sequence`.
3. If it's a deliberately private helper instead (an internal scratch
   name, not meant to be called by art code), add its name to the
   matching `*_INTERNAL` list instead of step 2. Style packs currently
   need no internal allowlist — every top-level name they define is
   public API — but if that stops being true, extend
   `tests/capabilities.rs`'s style-pack test the same way.
4. `cargo test --test capabilities` — it fails until one of steps 2/3
   is done, which is the point.

## Scope cuts (deliberate, not oversights)

- **`graph.ps`/`dataviz.ps`/`etching.ps` are not
  cataloged.** Issue #39's "What" section names fonts, Type 3 faces,
  palettes, style packs, page templates, and *artkit* procedures
  specifically; these are separate sibling libraries with their own
  independent APIs (graph.ps and dataviz.ps by design share nothing
  with artkit, per NOTES.md's issues #13/#14 entries). `hangul.ps`
  *is* cataloged despite being another sibling, since a cross-model
  review specifically flagged its face/procedures as "an installed
  agent-facing font" the catalog was silently omitting — the same
  reasoning doesn't extend to graph/dataviz/etching, which aren't font
  or handwriting families. A reasonable follow-up, not silently
  dropped.
- **No `--capabilities <kind>` filter.** The CLI always prints the
  full payload; an agent that wants just palettes filters the JSON
  itself. Simpler for a first version — revisit if the payload size
  becomes a real problem in practice.
- **Most procedures get no structured `parameters`.** See "Payload
  shape" above — PostScript's positional stack arguments don't map
  cleanly onto named/defaulted `Param`s the way a template's content
  dict (or an options-dict procedure's) does, and forcing a fit would
  mean inventing parameter names the source doesn't actually have.
