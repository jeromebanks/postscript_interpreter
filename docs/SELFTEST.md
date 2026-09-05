# SELFTEST.md — checking a PS-only change without writing Rust

Issue #95, Phase A of `docs/PS_LIBRARY_COUPLING.md`'s "Touchpoint 2".

Adding a primitive to `lib/*.ps` used to mean writing Rust: 50–624
lines of golden-image comparison and `ghostscript_accepts_*` driver per
feature. Two mechanisms here cover the larger share of the defect
classes that review actually finds, entirely in PostScript and the CLI:

```sh
./scripts/selftest.sh          # both of them, over every library and driver
```

Neither needs Ghostscript, a golden image, or a `.rs` file.

## Mechanism 1 — `%%SelfTest` blocks

A library carries its own validation-guard regression tests as doc
comments:

```postscript
%%SelfTest: pkribbon-rejects-undocumented-cap-values
%   { newpath 0 0 moveto 40 0 lineto << /StartCap /square >> pkribbon }
%   /pkribbon-startcap-must-be-round-flat-or-pointed
%   (/square as a start cap) mustguard
%%EndSelfTest
```

```sh
pscat --selftest lib/paintkit.ps
```

Because the body is comment text, the library stays completely inert
when a normal program `run`s it — the tests cost nothing at load time
and live next to the code they check.

### The vocabulary

| Assertion | Stack | Passes when |
|-----------|-------|-------------|
| `mustguard` | `{proc} /guardname (label)` | the proc raises `undefined` on exactly `/guardname` |
| `mustfail` | `{proc} /errorname (label)` | the proc raises exactly that interpreter error |
| `mustpass` | `{proc} (label)` | the proc raises nothing |
| `mustbe` | `bool (label)` | the boolean is true |

**There is deliberately no "assert *something* raised" form.** That
shape — `{ ... } stopped not { fail } if` — is unsound as a regression
test: a typo in the test body raises too, so the assertion stays green
forever while testing nothing. That is the exact silent drift this
whole mechanism exists to prevent, so it isn't in the vocabulary at
all.

`mustguard` is the primary form because it matches how this repo's
libraries actually reject malformed input: they invoke a
self-documenting undefined name (`pkribbon-width-must-not-be-a-
procedure`), which lands in `$error`'s `/command`. Matching the name
means a typo — which raises `undefined` under some *other* name —
fails the assertion instead of satisfying it.
`tests/selftest.rs::a_typo_in_a_test_body_does_not_satisfy_a_guard_assertion`
pins that property.

### Writing a block

- `%%SelfTest:` and `%%EndSelfTest` start at column 0; every line
  between them is a `%` comment. One space after the `%` is the
  comment marker; the rest is preserved, so indent freely.
- Prerequisites come from the file's existing `% @requires:` tag —
  the same one `build.rs`'s capability catalog reads. There is no
  second way to spell the load chain, so the two can't drift.
- Every malformation is a hard error, never a skipped block: an
  unclosed block, a bare code line inside one, a duplicate name, an
  empty body. A self-test that silently doesn't run reads as coverage
  that isn't there, which is worse than none.
- Each block runs in its own fresh interpreter, so one block can't
  affect another. Within a block, `mustguard`/`mustfail`/`mustpass`
  restore the operand and dictionary stacks after a caught error.
  PostScript has no `gsave`-depth query, so the runner checks that
  balance itself and fails the block on a leak.
- Placement matters in a tag-migrated file: put the block *above* the
  `% @kind:`/`% @summary:` tag block, never between it and the
  definition it documents — those tags must stay directly above their
  `def`. `build.rs` makes self-test regions invisible to its tag
  scanner (so PostScript starting a line with `@` doesn't fail the
  build) and rejects an unterminated block outright.

## Mechanism 2 — strict `--lint` over rendering drivers

`--lint`'s findings have been advisory since issue #17. `--lint-strict`
makes them fatal:

```sh
pscat --page 200x200 --lint-strict selftest/drivers/paintkit.ps
```

Plain `--lint` is unchanged — every existing caller keeps today's
behavior.

A bare `lib/*.ps` file draws nothing on load, so lint has nothing to
judge without a driver. `selftest/drivers/*.ps` are those drivers; see
`selftest/README.md` for the two rules they keep and why.

The class this catches is the one unit tests miss most easily: the call
returns cleanly, the page is the right size, and nothing is on it.
Three separate `pkribbon` defects had exactly that shape.

## What Phase A covers

Against `docs/PS_LIBRARY_COUPLING.md`'s classification of all 18 real
defects found across seven review rounds on PR #76:

| Row | Defect | Mechanism | Proved by |
|-----|--------|-----------|-----------|
| 5 | Non-procedure `/Pressure` not rejected | `mustguard` | `tests/selftest.rs::row_5_and_10_...` |
| 6 | Unsupported cap values not rejected | `mustguard` | `row_6_...` |
| 9 | Short pointed stroke renders blank | strict lint | `rows_9_11_12_...` |
| 10 | `xcheck` insufficient for `/Pressure` type | `mustguard` | `row_5_and_10_...` |
| 11 | Exact-pitch-multiple stroke renders blank | strict lint | `rows_9_11_12_...` |
| 12 | Undersampled closed path renders blank | strict lint | `rows_9_11_12_...` |
| 15 | Out-of-range taper unvalidated | `mustguard` | `row_15_...` |
| 16 | `atan(0,0)` on coincident endpoints | `mustpass` | `pkribbon-survives-a-degenerate-pointed-subpath` |
| 18 | Executable-array value options not rejected | `mustguard` | `row_18_...` |

Nine of eighteen, matching the decision record's estimate.

"Proved by" is the point. Each of those tests *reintroduces* the defect
and asserts the mechanism goes red — the acceptance criterion asked for
demonstration, not a claim. See `tests/selftest.rs`'s module comment for
why each proof asserts three states rather than two, and for the one
row where measuring that middle state contradicted the expectation.

## What Phase A does not cover

Stated explicitly, because a coverage mechanism that quietly omits
things is worse than one with a documented edge:

- **Row 14 — packed-array `/Pressure` wrongly rejected. Not coverable
  by this mechanism at all, ever.** `pscat` does not implement
  `setpacking`: `tests/paintkit.rs`'s own
  `setpacking_true_does_not_break_pressure_validation` confirms
  directly that `true setpacking { } type` stays `arraytype` here,
  never `packedarraytype`. A `%%SelfTest` block cannot produce the type
  the defect is about, however it is written. What covers it is the
  retained `ghostscript_accepts_*` checks run under real `gs`, six of
  which open with `true setpacking` for exactly this reason — see
  `docs/GS_CHECK_INVENTORY.md`. **Phase A does not replace those.**
- **Row 17 — `pathforall` missing implicit `moveto` after
  `closepath`.** A genuine interpreter defect, so only Rust/gs-parity
  testing (`tests/pathforall.rs`) reaches it. It was found *while
  implementing* `pkribbon`, by reading code, not by any test shape.
- **Row 8 — a demo file missing `showpage`.** Still uncovered, by
  design. `--lint`'s blank-page check deliberately treats a live
  trailing canvas containing ink as a legitimate final page, and
  `examples/sweep_demo.ps` and `examples/walkpath_demo.ps` are real
  committed programs that rely on exactly that. The two cases aren't
  distinguishable from render output — only from author intent — so a
  blanket rule would false-positive on both. Small and cosmetic;
  closing it safely needs an opt-in per-file marker, which is its own
  design problem.
  - Note the *drivers* under `selftest/drivers/` are not affected: they
    declare `%%Pages:` and the page-count check enforces it, which
    covers the same mistake for the files where the count is known.
- **Row 7 — a procedure missing from the catalog.** Closed by issue
  #94's tag-driven generator, not by anything here.
- **Rows 1–4 and 13 — geometry and measurement correctness.** These
  need pixel readback (does this region have ink? does measured width
  track pressure?), which PostScript has no operator for. That is
  Phase B's pixel-sample operator, tracked separately.
- **The `ghostscript_accepts_*` extraction itself.** Inventoried here
  (issue #95's acceptance criterion asks for the inventory "before any
  extraction"), not performed. `docs/GS_CHECK_INVENTORY.md` explains
  what reading all 25 in full turned up that makes a uniform extraction
  script insufficient.
- **Cross-model review.** Not weakened by any of this. Row 17 was found
  by a second model reading the code, and nothing here reads code the
  way a second model does.

## Adding a library to the self-test pass

1. Add `%%SelfTest` blocks to `lib/yourkit.ps` (above the `% @` tag
   block of whatever they cover, if the file is tag-migrated).
2. Add `selftest/drivers/yourkit.ps` with a `%%SelfTestPage: WxH`
   header, a `%%Pages:` count, and one `showpage` per scenario.
3. Nothing else. `scripts/selftest.sh` and `tests/selftest.rs` both
   discover files rather than listing them.
