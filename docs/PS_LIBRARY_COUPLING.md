# PS_LIBRARY_COUPLING.md — reducing what a PS-only change must touch (issue #92)

A time-boxed architecture spike, same shape as #46's watercolor spike
(`docs/WATERCOLOR.md`), into the three coupling points issue #92 names:
the hand-maintained `src/capabilities.rs` catalog, the Rust integration
tests a new `lib/*.ps` primitive needs today, and the GHA `ci.yml` gate.
This document is the decision record #92 asks for. It recommends
architectures for three follow-up issues; it implements none of them —
same division of labor as #46 → #47, applied three times.

## Method

Read `src/capabilities.rs` (2237 lines) and its cross-check,
`tests/capabilities.rs` (342 lines); read `tests/paintkit.rs` (2532
lines) end to end for its actual verification techniques; read
`.github/workflows/ci.yml`, `tests/golden.rs`, `tests/corpus.rs`, and
`src/lint.rs`; read all seven rounds of Codex review on PR #76 (issue
#41, `pkribbon`) via `gh pr view 76 --comments`; pulled real diff stats
for issues #41–#45 via `git diff --stat` on their merge commits; pulled
a real CI run's per-step timings and log output via `gh run view
--json jobs` / `--log`. One empirical check ran against a locally
built release `pscat` binary (below, touchpoint 2). No prototype
implementation was built for any of the three recommendations — unlike
#46, this issue's acceptance criteria ask for a decision record and a
worked-example comparison, not a working prototype of the new
mechanism, and time-boxing this spike to research-and-measure (not
build) is itself a scope decision worth stating.

## Touchpoint 1 — `src/capabilities.rs`

**Finding: the doc-comment convention the issue points to is real, but
it's prose, not a grammar.** `lib/paintkit.ps` and friends do
consistently document each dict-driven procedure's keys in a
recognizable shape —

```
%   /Width       base width (default 6). Must be > 0.
%   /Pitch       walkpath sampling pitch (default Width*0.5, capped at
%                6 so wide ribbons don't facet). Must be > 0.
```

(`lib/paintkit.ps:43-45`) — but the text wraps across lines, contains
cross-references and caveats mid-sentence, and the per-procedure
top-level description above it is multi-paragraph prose (`pkribbon`'s
own header runs `lib/paintkit.ps:12-102`), not a one-line summary. The
Rust `ENTRIES` table's descriptions are hand-condensed from this prose,
not transcribed — compare `capabilities.rs:976`'s one-line "Treats the
current path as a centerline and fills a variable-width ribbon along
it, built on walkpath" against the header's ninety-line contract. A
generator that parsed `/Key description (default D)` lines directly
could recover names, defaults, and parameter text fairly reliably (the
`/Key` line shape is consistent); it cannot recover a good one-line
`description` without either accepting much longer text than the
catalog was built to carry, or requiring authors to write a *second*,
new, terse summary tag that doesn't exist in any `lib/*.ps` file today.

**Recommendation:** a follow-up issue should add one new, disciplined
tag — e.g. `%%Summary:` — to the doc-comment convention (retrofitting
existing files once), keep parsing `/Key ... (default D)` for
parameters, and derive the catalog by **`include_str!`-embedding each
`lib/*.ps` file and parsing it at build time**, not by reading it from
disk at runtime and not by committing a generated `.rs` file. Runtime
parsing was the first instinct here and is wrong for two concrete
reasons: `--capabilities`/`describe_art_capabilities` would become
fallible on a moved or missing `lib/` (today's static table can't fail
that way), and the wasm build (`web/`, `scripts/build_wasm.sh`) has no
filesystem to read from at all — `.claude/skills/work-issue/SKILL.md`
already documents this exact constraint for the playground picker,
which is why it requires self-contained `.ps` files. `include_str!`
keeps the parse entirely in the binary (wasm included) while still
meaning **zero hand-written Rust for a new procedure in an existing
file** — a new `pkfoo` in `lib/paintkit.ps` with a correct doc comment
needs a rebuild to pick it up, not a hand-edited `ENTRIES` row. **A
brand-new sibling file is a different case, caught by cross-model
review rather than shipped as an overclaim**: `include_str!` takes a
literal path at a Rust call site, so a genuinely new `lib/newkit.ps`
still needs one line added where the existing files are listed —
`include_str!` alone doesn't enumerate a directory. True zero-touch
even for a brand-new file needs a `build.rs` directory scan (emitting
an `OUT_DIR` manifest, tracking the directory for rebuilds) instead of
a fixed list of `include_str!` calls — a real design choice for the
follow-up, not a detail to gloss over. Either way, the *test-coverage*
gap from the round-2 finding above (a new file needing its own
hand-written cross-check function) still closes: a generic checker
walking whatever the embed step produced doesn't care whether that
list came from a fixed `include_str!` set or a `build.rs` scan. `tests/capabilities.rs`'s two-way cross-check should
survive essentially unchanged in spirit, but its `INTERNAL_*`
allowlists can likely shrink or disappear: instead of a name needing to
be either cataloged-by-hand or explicitly allowlisted, only a name
whose definition carries a `%%Summary:` block would be cataloged at
all, and everything else defaults to internal by construction — one
list instead of two, derived from the same source instead of
maintained in parallel with it.

**What this doesn't solve on its own:** the round-2 Codex finding on
PR #76 — `pkribbon` shipped without being registered in
`src/capabilities.rs` — is evidence for this touchpoint, but not for
the reason first assumed. `tests/capabilities.rs`'s cross-check
mechanism already existed at the time (added in PR #74, issue #39) —
it did not fail to exist. What actually happened: adding
`lib/paintkit.ps` as a *new sibling file* required hand-writing a
*second* thing beyond the catalog entries — a near-duplicate
`paintkit_names_match_the_catalog_exactly` test function
(`tests/capabilities.rs`, 19 lines, structurally identical to
`pagekit_names_match_the_catalog_exactly` just above it) — and that
step is exactly as easy to skip or defer as the catalog registration
itself. A generic, file-enumerating generator closes both gaps at
once: a brand-new `lib/*.ps` file gets covered by the same one
cross-check the moment it exists, with no new per-file test function
to remember either. That's a stronger claim than "a tagged proc can't
ship uncataloged" — it's "a new *file* can't ship unchecked," which is
the shape of the actual miss.

**Rejected:** full free-text parsing of the existing prose without a
new summary tag. The risk isn't that it fails loudly (a botched parse
would show up in review or a rendered `--capabilities` dump) — it's
that it silently produces a *worse* catalog than today's hand-curated
one and nobody notices, because there's no test that checks
description *quality*, only name existence.

## Touchpoint 2 — PS-native verification

**Finding: `tests/paintkit.rs` is not one mechanism, it's two, and they
have very different costs to replace.**

1. **Numeric pixel-property assertions** — `ink_count`, `column_height`,
   `luma` (`tests/paintkit.rs:47-68`) sample the rendered `Pixmap`
   directly. Tests like `closed_polygon_leaves_a_hole_in_the_middle`
   or `pressure_profiles_change_the_measured_width` fundamentally need
   to read back what got painted. PostScript has no operator for that
   today — replacing this tier needs one-time new Rust: a pixel/coverage
   -sampling operator exposed to PS.
2. **`ghostscript_accepts_*`** (seven of them across `paintkit.rs`, one
   per library file) — concatenates the library source plus a driver
   snippet, shells out to `gs`, asserts exit 0. No golden image, no
   pixel comparison, no pscat-specific state. This is a shell script
   wearing a `#[test]` attribute; it's in Rust purely because that's
   where the test suite lives, not because it needs to be. **This is
   an immediately-extractable win, not follow-up work** — a
   `scripts/gs_check.sh <file.ps> <driver>` invoked directly (from a
   PS-only CI path, or by hand) does the identical check with zero
   Rust.

**Classifying all 18 real defects Codex's seven review rounds on PR #76
found** (read from `gh pr view 76 --comments` in full, not just round
1), against what would actually have caught each one, sizes tier 2
honestly instead of asserting it. The `#` column below is this table's
own row number, not a GitHub issue number — this repo's issue #8 (PDF
metadata) is unrelated to row 8 below, and the two get referenced near
each other later in this document:

| # | Round | Finding | Catchable by |
|---|---|---|---|
| 1 | 1 | Closed loop fills solid, no hole | pixel-sample op (new) |
| 2 | 1 | Taper-overlap width discontinuity | pixel-sample op (new) |
| 3 | 1 | Closed-path epsilon breaks at scale | pixel-sample op (new) |
| 4 | 1 | Cap-degeneracy epsilon breaks at scale | pixel-sample op (new) |
| 5 | 2 | Non-procedure `/Pressure` not rejected | `stopped` — **today** |
| 6 | 2 | Unsupported cap values not rejected | `stopped` — **today** |
| 7 | 2 | `pkribbon` missing from catalog | solved by touchpoint 1 |
| 8 | 2 | Demo file missing `showpage` | **not covered — see below** |
| 9 | 3 | Short pointed stroke renders blank | `--lint`'s blank-page check — **today** |
| 10 | 3 | `xcheck` insufficient for `/Pressure` type | `stopped` — **today** |
| 11 | 4 | Exact-pitch-multiple stroke renders blank | `--lint`'s blank-page check — **today** |
| 12 | 4 | Undersampled closed path renders blank | `--lint`'s blank-page check — **today** |
| 13 | 4 | Coordinate coincidence misclassifies closure | pixel-sample op (new) |
| 14 | 5 | Packed-array `/Pressure` wrongly rejected | `stopped` — **today** |
| 15 | 5 | Out-of-range taper unvalidated | `stopped` — **today** |
| 16 | 6 | `atan(0,0)` crash on coincident endpoints | `stopped` — **today** |
| 17 | 6 | `pathforall` missing implicit moveto after `closepath` | **interpreter bug — Rust/gs-parity only** |
| 18 | 7 | Executable-array value options not rejected | `stopped` — **today** |

Ten of eighteen (5–6, 9–12, 14–16, 18) need **no new interpreter work
at all** — PostScript's own `stopped`/`errordict` already catches a
validation guard firing or not firing, and `--lint`'s existing
blank-page heuristic (issue #17, already shipped) already catches a
render producing genuinely no ink. **Verified directly, not assumed**:
built a release `pscat` and ran

```
{ newpath 0 0 moveto 10 10 lineto << /Pressure 5 >> pkribbon } stopped
{ (caught it) print } { (did not raise) print } ifelse
```

against real `lib/paintkit.ps` — prints `caught it`; the same driver
with a well-formed `<< /Width 8 >>` call does not trigger `stopped`,
confirming the mechanism actually discriminates instead of
always-catching. One (#7) disappears by construction once touchpoint 1
lands. One (#17) is a genuine interpreter defect that only Rust-level
(here, `tests/pathforall.rs`, gs-pinned) testing catches — direct
evidence for the issue's own acceptance criterion that
interpreter-level gs-parity testing must not go away. Five (1–4, 13)
are the geometry/rendering-correctness class that needs the new
pixel-sample operator.

**#8 was misclassified in a first pass and corrected by cross-model
review**: a demo that paints ink but never calls `showpage` is *not*
what `--lint`'s `check_blank_pages` flags. Its own logic
(`src/lint.rs`) deliberately treats a live trailing canvas that
contains ink as a legitimate final page — that's the right behavior
for lint's actual purpose (don't flag a program that simply forgot the
final `showpage`-then-exit boilerplate as having drawn nothing), but
it means "painted something, forgot `showpage`" and "painted nothing"
are different conditions and only the second is caught today. #8 is
genuinely uncovered by anything that exists now. Closing it doesn't
need a new interpreter primitive, though — it needs a *second*, narrower
lint check alongside the existing blank-page one: "the trailing canvas
has ink but the program never called `showpage`." That's small enough
to fold into Phase A rather than treat as its own gap, so the honest
tally is **11 of 18** need no new interpreter primitive (the 10 above
plus #8 once Phase A adds this one check), not the 10 that exist
today.

**Recommendation:** a follow-up issue, phased:
- **Phase A** (small, no new operator): a `%%SelfTest` doc-comment
  convention wrapping `stopped`-based assertions, plus a `pscat
  --selftest file.ps` CLI mode that runs a file's self-check blocks
  and exits non-zero on any failure, printing which assertion failed —
  plus one small addition to `--lint`'s existing blank-page heuristic:
  flag a trailing canvas that has ink but was never explicitly closed
  with `showpage` (today's heuristic deliberately doesn't flag this,
  since it exists to catch "painted nothing," a different condition —
  see #8 below). Together this covers 11 of 18 real defect classes
  found on the one PR this repo has the deepest review history for,
  entirely in PS/CLI, no gs and no new interpreter primitive required.
- **Phase B** (real new Rust, one-time): a pixel/coverage-sampling
  operator (e.g. `x y currentluma`, or a coverage-at-point query) so a
  `%%SelfTest` block can also assert "this region has ink" / "this
  region doesn't" / "measured width tracks pressure" — covering the
  remaining rendering-correctness class. One-time cost like touchpoint
  1's parser, not a per-feature one.
- Extract `ghostscript_accepts_*`'s pattern into a standalone script
  usable outside `cargo test` today, independent of Phases A/B.

**Rejected:** treating `--lint`'s existing heuristics as sufficient on
their own (they catch 3 of 18 today — a fourth, #8, needs the small
`--lint` addition Phase A above adds, not something `--lint` already
does — and none of the 18 are the geometry-correctness class) and
treating this as fully solved without Phase B (5 of 18
genuinely need pixel readback). **Not weakened:** cross-model review
stays required regardless — #17 (the `pathforall` bug) was found
*while implementing pkribbon*, not by any test shape, and nothing
proposed here reads code the way a second model does.

## Touchpoint 3 — the CI gate

**Finding, corrected after checking primary evidence: Ghostscript is
NOT absent from CI.** The first pass at this touchpoint inferred from
`ci.yml` having no `brew install ghostscript` step that `gs` must be
missing on the runner and every `gs`-dependent check silently skips.
That inference was wrong and got caught before writing it down — real
CI logs from PR #86 (`gh run view 32877967427 --log`) show `gs` is
present (macOS GHA images ship a large preinstalled Homebrew set that
includes it) and every `ghostscript_accepts_*`, `golden_spiral`, and
`corpus_round2_matches_ghostscript` test genuinely runs and passes on
every PR today, PS-only ones included. The issue's original framing —
CI needs Ghostscript regardless of what changed — is correct as
written; there is no free win here from "it's already skipped."

**What's real, measured from the same run's per-step timings**
(`gh run view --json jobs`): `cargo fmt --check` ≈0s (cached, nothing
to check), `cargo clippy` ≈7s, `cargo build` ≈8s, the full `cargo test`
run ≈58s, out of a ≈106s job total. `cargo test` is what's actually
expensive, and it's also the thing that — until touchpoint 2 Phase A/B
land — is the *only* mechanism verifying a PS-only change is correct.
Skipping it for a "PS-only diff" today wouldn't shrink the gate, it
would remove the gate. Skipping `clippy`+`fmt` for a genuinely
`.rs`-free diff is real and safe (they check Rust source that didn't
change) but saves ≈7s of ≈106s — a small slice, not the "shrink to
near zero" the issue is after.

**Recommendation:** two follow-ups, not one, sized honestly by what
each actually buys:
- **Small, independent, low-risk**: skip the `fmt`/`clippy` steps (as
  conditional steps inside the existing `test` job, per
  `SDLC.md`'s `required_status_checks` — a path-filtered *separate*
  job that never runs leaves a required check permanently pending and
  blocks every PS-only PR from merging, the opposite of the goal; base
  the diff check on the PR's merge base, and handle the plain
  `push: main` trigger, which has no base to diff at all) when the
  diff touches no `.rs`/`Cargo.toml`/`Cargo.lock`. Worth doing on its
  own merits even though the win is modest.
- **Larger, blocked on touchpoint 2**: once Phase A/B ship and are
  proven to catch what `cargo test` catches today, a narrower path
  becomes possible — but narrower than *today's* full gate, not as
  narrow as "skip `cargo build` entirely." Three corrections to that
  first draft, caught by cross-model review rather than shipped as
  written:
  - **Still needs a fresh `cargo build`, not a cached binary.**
    Touchpoint 1's catalog is `include_str!`-embedded — compile-time,
    not runtime. A cached `pscat` from before this PR still contains
    the *old* library source; `--selftest` run against it can't
    validate a new `%%Summary:` tag, a new procedure's catalog
    entry, or anything else this PR actually changed. The narrow path
    still pays `cargo build`'s ≈8s; what it skips is `clippy`
    (≈7s) and the Rust portion of `cargo test`.
  - **Still needs the extracted `ghostscript_accepts_*` check.**
    Touchpoint 2 says that check moves out of `cargo test` into a
    standalone script, not away — a PS change that `pscat` accepts but
    real `gs` rejects is exactly the class of bug this path must not
    stop catching. The narrow path is `cargo build` + `pscat
    --selftest` + the extracted `gs_check.sh`, not `--selftest` alone.
  - **Must cover dependent libraries, not just the changed file.**
    `lib/pagekit.ps`/`lib/paintkit.ps`/the style packs all depend on
    `lib/artkit.ps`; running `--selftest` only against a changed
    `artkit.ps` would miss a regression in one of its callers that
    today's full `cargo test` catches by running every test
    regardless of what changed. The narrow path needs to run every
    library's self-tests (simplest, and safe by construction) or a
    real dependency-closure computation (narrower, but its own design
    problem) — not just the changed files' own tests. Sizing that
    tradeoff is follow-up-3's work, not resolved here.

  With those three corrections, the win is real but smaller than the
  first draft claimed: `clippy` (≈7s) plus most of `cargo test`'s ≈58s
  minus whatever the self-test/gs-check pass itself costs — not the
  full ≈58s, and not `cargo build`'s ≈8s at all. Doing any of this
  before touchpoint 2 exists would still mean shipping a PS-only PR
  with *less* verification than it gets today, which the issue's own
  acceptance criteria rule out.

**Not rejected, not weakened:** `tests/golden.rs`/`tests/corpus.rs`
stay exactly as-is — they verify the interpreter's own gs-parity
against a fixed corpus, not any one feature, and nothing above touches
them.

## Worked example: `pkoil` (issue #45, PR #86)

Real diff, `git diff b4dcbe4 16eff60 --stat`:

| File | Lines | What |
|---|---|---|
| `lib/paintkit.ps` | 226 | the feature itself |
| `examples/paintkit_oil_demo.ps` | 109 | specimen page |
| `src/capabilities.rs` | 82 | hand-written catalog entry |
| `tests/paintkit.rs` | 50 | Rust pixel/render tests |
| **Total** | **467** | **335 PS / 132 Rust** |

CI for this PR: full `fmt`/`clippy`/`build`/`test` gate, ≈106s,
Ghostscript genuinely exercised (`ghostscript_accepts_paintkit` and
friends actually ran).

**Projected, once all three follow-ups above land** (not built — this
is what the recommended mechanisms predict, stated as a projection,
not a measurement):
- `lib/paintkit.ps` — 226 lines, unchanged. It's the feature; nothing
  proposed here touches library code itself.
- `examples/paintkit_oil_demo.ps` — 109 lines, unchanged, same reason.
- `src/capabilities.rs` — **0 hand-written lines.** A `%%Summary:` tag
  plus the existing `/Key (default D)` lines in `pkoil`'s own header
  comment are all that's needed; `--capabilities` picks it up from a
  rebuild, no `ENTRIES` row to write.
- `tests/paintkit.rs` — its actual 50 lines (`git diff b4dcbe4 16eff60
  -- tests/paintkit.rs`) are three tests, not a uniform block: 29 lines
  (`oil_validation_and_safety`, four malformed-input cases each
  asserting a specific error name) are Phase-A-expressible today, no
  new operator — the same `stopped`+error-name pattern verified above
  against `pkribbon`, since PostScript's `$error` dict already exposes
  which guard fired, not just that one did. The other 21 lines
  (`oil_renders_loaded_impasto`, `oil_determinism_fixed_seed`) call
  `ink_count` — a rendered-pixel measurement — and stay Rust-dependent
  until Phase B's pixel-sample operator exists. So for this real
  feature the honest split is **~29 lines → 0 Rust (Phase A alone)**,
  **~21 lines → 0 Rust only after Phase B**, not a uniform "delete all
  50" claim.
- CI: `fmt`/`clippy` skipped (diff touches no `.rs`); `cargo build`
  still runs (this diff changes `lib/paintkit.ps`'s embedded content,
  so a cached binary would validate against stale catalog/self-test
  source — corrected above); `cargo test`'s Rust suite replaced by
  `pscat --selftest` over *every* library's self-checks (not just
  `paintkit.ps` — it has no dependents among the sibling libraries
  today, but the narrow path can't assume that in general) plus the
  extracted `gs_check.sh` for gs-acceptance.

Net: the PS-only work itself (335 lines) is exactly as much PS as
before — this was never going to shrink, and shouldn't. Of the 132
Rust lines, 82 (the catalog entry) and 29 of the 50 test lines
(validation guards) reach zero after Phase A + touchpoint 1 alone; the
remaining 21 test lines (the two pixel-measurement checks) need Phase
B as well. So the realistic near-term floor for a feature shaped like
`pkoil` is **~111 of 132 Rust lines gone**, not all 132, until Phase B
ships — still the majority of the touchpoint-1/2 cost, with the exact
remainder identified rather than assumed away. None of this is built
yet; all three follow-ups below are what would have to land first.

## Answering the issue's questions to settle

- **Can `capabilities.rs` be generated from doc comments without
  losing the cross-check?** Yes, via `include_str!` + a new
  `%%Summary:` tag, not via runtime parsing (breaks wasm and adds a
  new failure mode) and not via committing generated Rust (still a
  per-feature `.rs` diff, just an automated one). The cross-check
  survives, and can likely simplify from two allowlists to one.
- **What does PS-native verification need, and is it one-time or
  recurring?** Phase A (a `%%SelfTest` convention + `--selftest` CLI
  mode) needs no new interpreter primitive and is genuinely one-time;
  it alone covers 11 of 18 real defects from this repo's
  best-documented review history. Phase B (a pixel-sample operator) is
  also one-time, not per-feature, and covers the remaining 5.
- **Can CI detect a PS-only diff and run narrower?** Yes for
  `fmt`/`clippy` today (small win, ≈7s of ≈106s, real and doable now).
  Not for `cargo build` — the embedded catalog/self-test content
  changes with the PS diff, so a fresh build stays required regardless
  of Phase A/B. `cargo test`'s Rust suite can shrink once Phase A/B
  exist and the narrow path runs self-tests for every library (not
  just the changed one, to cover dependents) plus the extracted
  gs-acceptance check — doing so before Phase A/B exist, or without
  those two corrections, would ship PS-only PRs with strictly less
  verification than today.
- **Where does Ghostscript remain necessary vs. default habit?**
  Necessary and *actually exercised in CI today* (corrected finding,
  above) for `tests/golden.rs`/`tests/corpus.rs` (interpreter-level
  parity) and for `ghostscript_accepts_*` (per-library gs-acceptance).
  The latter needs no pixel comparison and is extractable into a
  standalone script independent of the other two follow-ups.
- **Does "done" change for a PS-only issue in `SDLC.md`/
  `work-issue`?** Not yet — none of the three follow-ups exist. Once
  they do, `.claude/skills/work-issue/SKILL.md`'s step 5 quality gate
  could special-case a diff with no `.rs`/`Cargo.toml` changes to run
  `pscat --selftest` instead of the full `cargo` gate; that's a change
  to make when Phase A/B land, not now.

## Which touchpoints does this issue address directly?

None. All three need a follow-up: touchpoint 1 needs a new doc-comment
tag and a parser that doesn't exist; touchpoint 2 needs either a new
CLI mode (Phase A) or a new interpreter operator (Phase B); touchpoint
3's only piece with no dependency on the other two — skipping
`fmt`/`clippy` for a `.rs`-free diff — is real but measured at ≈7s of
a ≈106s job, and a required-status-check mistake in a CI change is
exactly the kind of hard-to-reverse-if-wrong risk not worth taking
inside a spike PR whose actual deliverable is this decision record.
Bug-catching value is not weakened by any of this: of the 18 real
defects tabulated above, 12 need no new interpreter operator — 3
already caught by `--lint` today, 7 by PostScript's own
`stopped`/`errordict` today, 1 (#7) by touchpoint 1's catalog
mechanism, and 1 (#8) by the small `--lint` addition Phase A adds — 5
more are covered once Phase B's pixel-sample operator lands, and 1
(`pathforall`'s interpreter bug) is permanently out of reach for
PS-native testing and stays exactly where it is today, in
`tests/pathforall.rs`, gs-pinned.

**Open question for whichever follow-up lands `%%Summary:`/
`%%SelfTest`:** `%%`-prefixed comments are DSC structured comments,
already meaningfully parsed by this repo (`%%Title:`/`%%For:` feed PDF
`/Info`, this repo's GitHub issue #8 — unrelated to row 8 in the
defect table above) and by Ghostscript's own DSC parser. Two new tags in
that same namespace is a real decision, not a free choice, and needs
picking deliberately — a non-DSC prefix (`%!Summary:`/`%!SelfTest`, or
`% @summary`/`% @selftest`) avoids the collision entirely and is
probably the safer default; noted here so the follow-up implementer
doesn't have to rediscover it.

## Follow-up issues

Three, mirroring #46 → #47's one-recommendation-to-one-issue shape,
sized by what's actually needed. Not filed as GitHub issues by this
spike — stating which follow-ups are needed is this issue's own
acceptance criterion; filing them is left to whoever picks this
document up next, same as #46 left #47's filing to a human/agent
follow-up rather than filing it itself.

1. **Doc-comment-driven capabilities catalog** (touchpoint 1): a new
   `%%Summary:` tag, `include_str!`-based build-time parsing, and a
   simplified `tests/capabilities.rs` cross-check.
2. **PS-native self-check convention** (touchpoint 2): Phase A
   (`%%SelfTest` + `--selftest`, no new operator) and Phase B (a
   pixel-sample operator), plus extracting `ghostscript_accepts_*`
   into a standalone script. Phase A is independently shippable and
   should land first — it's the larger share of the defect classes
   found (11 of 18) for the smaller cost.
3. **CI diff-shape detection** (touchpoint 3): the small `fmt`/`clippy`
   skip is independently shippable now; `cargo build` stays required
   regardless (the embedded catalog/self-test content changes with the
   PS diff); the `cargo test` Rust-suite replacement is blocked on
   issue 2's Phase A/B landing, must run every library's self-tests
   (not just the changed file, to cover dependents) plus the extracted
   `ghostscript_accepts_*` check, and should be scoped as part of that
   work, not before it.

## What's explicitly rejected

- Runtime (non-`include_str!`) parsing of `lib/*.ps` for the
  capabilities catalog — breaks on wasm, adds a new failure mode
  static data doesn't have.
- Treating `--lint`'s existing heuristics as a full replacement for
  `tests/paintkit.rs` — they cover 3 of 18 real defect classes found
  here today (a 4th, the missing-`showpage` case, needs a small
  addition to `--lint` that doesn't exist yet), and none of the 18 are
  the geometry-correctness class Phase B targets.
- Skipping `cargo build` for a PS-only diff, at any point — the
  embedded catalog/self-test content changes with the PS source, so a
  cached binary can't validate it.
- Narrowing CI's `cargo test` step to only the changed `.ps` file's
  own self-tests, or to skip the extracted gs-acceptance check — both
  would ship PS-only PRs with less verification than today (missing a
  dependent-library regression, or a `gs`-rejects-it defect `pscat`
  itself doesn't), which the issue's own acceptance criteria rule out.
  Narrowing it at all is blocked on a PS-native verification path
  existing to replace what it currently proves.
- Reducing or removing cross-model review for PS-only changes — it
  caught the one defect (#17, the `pathforall` interpreter bug) that
  no test shape proposed here would have found, and 4 more (closed-loop
  winding, taper overlap, epsilon scale bugs) before this spike's
  proposed pixel-sample operator would exist to catch them
  automatically.
- Removing or weakening `tests/golden.rs`/`tests/corpus.rs` — out of
  scope per the issue's own acceptance criteria, and untouched by
  anything recommended here.
