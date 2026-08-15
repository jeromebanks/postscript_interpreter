# CAPABILITIES.md — the machine-readable art catalog

Issue #39. `pscat --capabilities` (CLI) and `describe_art_capabilities`
(the `pscat-mcp` tool) both print the same JSON payload: a structured
inventory of what this build's creative toolkit actually has installed
— fonts, the Type 3 program faces, artkit's mood palettes, the page
templates, and artkit's/the style packs' major procedures — so an
agent can discover real capabilities instead of guessing names from
prose documentation, which drifts. (It already had: `psart`'s
`SKILL.md` was behind artkit's paragraph-flow, hyperbolic-geometry,
noise/flow, and gradient sections by the time this catalog was built —
see NOTES.md's issue #39 entry.)

## Payload shape

```json
{
  "pscat_version": "0.5.1",
  "capabilities": [
    {
      "name": "httile",
      "kind": "procedure",
      "description": "Breadth-first-reflection generator for regular {p,q} hyperbolic tessellations.",
      "parameters": [],
      "source": "lib/artkit.ps",
      "example": "cx cy r p q depth {gen proc} httile -",
      "availability": "library"
    }
  ]
}
```

`pscat_version` is the drift signal: an agent that caches a catalog
dump can treat a version change as "re-fetch," without needing to diff
the whole payload itself. `capabilities` is sorted by (name, kind) so
two dumps of the same release compare byte-for-byte regardless of
directory-scan or hashmap iteration order upstream.

`kind` is one of `font`, `type3_face`, `palette`, `template`,
`procedure`. `availability` says how durable an entry is:
- `builtin` — compiled into the binary, resolves identically
  everywhere (wasm included).
- `catalog (desktop/bundle only; ...)` — a font-catalog face or an
  alias to one; depends on `fonts/catalog/` being present, so absent
  on wasm and possibly absent on an incomplete install.
- `library` — defined in a `.ps` file loaded via `run`; always
  available once that source is loaded, no runtime filesystem
  negotiation the way catalog fonts need.

`parameters` is populated only where PostScript actually has named,
defaulted arguments — the five page templates, whose content dict has
optional keys with real defaults (see `lib/pagekit.ps`). Procedures
take positional stack arguments with no such structure, so their
calling convention lives in `example` instead (the stack-effect
comment already written at the proc's definition site, e.g.
`x0 y0 v1 v2 n1 n2 {x y ...} lattice -`) and `parameters` stays empty.
Fonts and Type 3 faces likewise carry no `parameters`.

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
registration time.

## What keeps the hand-maintained part honest

`tests/capabilities.rs` actually loads each `.ps` source into a real
`Interp` and checks the name set both ways:

- **Forward** — every name `ENTRIES` claims exists (a palette in
  `Palettes`, a template/procedure in `userdict`, a Type 3 face in
  `FontDirectory`) really is defined there after loading its source.
  A rename or removal in the `.ps` file fails this immediately.
- **Reverse** — every top-level name a source file actually defines is
  accounted for: either cataloged in `ENTRIES`, or listed in
  `capabilities::ARTKIT_INTERNAL` / `PAGEKIT_INTERNAL` (scratch
  helpers and internal state — `apseg`, `tfdrawline`, `Palettes` the
  dict itself, and so on — that are real names but not part of the
  public API). A newly added public palette or procedure that nobody
  registered fails this instead of just being silently missing from
  `--capabilities`.

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
   stack-effect comment as `example` (or, for a template, real
   `Param`s with defaults), the source path, and `availability`
   (almost always `LIB` for anything in `lib/`).
3. If it's a deliberately private helper instead (an internal scratch
   name, not meant to be called by art code), add its name to
   `ARTKIT_INTERNAL` or `PAGEKIT_INTERNAL` instead of step 2. Style
   packs currently need no internal allowlist — every top-level name
   they define is public API — but if that stops being true, extend
   `tests/capabilities.rs`'s style-pack test the same way.
4. `cargo test --test capabilities` — it fails until one of steps 2/3
   is done, which is the point.

## Scope cuts (deliberate, not oversights)

- **`graph.ps`/`dataviz.ps`/`etching.ps`/`hangul.ps` are not
  cataloged.** Issue #39's "What" section names fonts, Type 3 faces,
  palettes, style packs, page templates, and *artkit* procedures
  specifically; these are separate sibling libraries with their own
  independent APIs (graph.ps and dataviz.ps by design share nothing
  with artkit, per NOTES.md's issues #13/#14 entries). A reasonable
  follow-up, not silently dropped.
- **No `--capabilities <kind>` filter.** The CLI always prints the
  full payload; an agent that wants just palettes filters the JSON
  itself. Simpler for a first version — revisit if the payload size
  becomes a real problem in practice.
- **Procedures get no structured `parameters`.** See "Payload shape"
  above — PostScript's positional stack arguments don't map cleanly
  onto named/defaulted `Param`s the way a template's content dict
  does, and forcing a fit would mean inventing parameter names the
  source doesn't actually have.
