# VM.md — save/restore design (Stage 8 task 1)

Written 2026-07-16, before implementation, per the task's
[opus+review] tag. This is the feature flagged since Stage 1 as the
one that could reach into the object model; the framing lives in
`ARCHITECTURE.md` ("Known-hard things"). Everything here was pinned
against gs first (snippets below) and the PLRM second.

## What the feature actually is

`save` captures the state of composite-object *contents* in VM and
the graphics state; `restore` rolls both back. The stacks themselves
are **not** restored — only checked (see deviations). Two facts
discovered by pinning that shape the whole design:

1. **Strings are exempt.** PLRM 3rd ed. §3.7.3.2 resets "all
   composite objects *except strings*", and gs agrees:
   `/s (abc) def save s 0 88 put restore s ==` → `(Xbc)`.
   This removes the scariest part of the mutation audit — the string
   write paths (`readstring`, `cvs`, `putinterval` on buffers, Type 1
   `RD`) need no interception at all.
2. **Arrays and dicts do roll back**, including entries `def`d after
   the save (`/s2 save def s1 restore` then `s2` → `undefined` in gs,
   because userdict itself rolled back).

## The decision: object-granularity copy-on-write journaling

Three candidate designs, judged against this codebase:

- **Deep VM snapshot at save** — walk everything reachable from the
  dict stack and copy it. Rejected: needs an identity map to handle
  cycles (systemdict references itself by design), copies large font
  dicts (hundreds of charstrings) on every save, and costs O(VM) even
  for a save/restore pair that touches three objects.
- **Per-slot undo journal** — log `(target, index, old value)` for
  every mutation. Rejected: a loop that `put`s 100k times into one
  array inside a save context grows a 100k-entry journal for what is
  one object's rollback; the log is unbounded relative to VM size.
- **Object-granularity COW journal** — while at least one save is
  live, the *first* mutation of a given array storage or dict at the
  current save level snapshots that whole object's contents into the
  journal; later mutations of the same object at the same level are
  free. Chosen: journal size is bounded by (objects touched ×
  save depth), zero overhead when no save is live (the common path —
  the Stage 8 perf yardstick stays honest), no cycle problems
  (snapshots are shallow clones; `Object` clones are `Rc` handle
  copies).

### Data model (`src/interp.rs`)

```rust
enum JEntry {
    Array { data: Rc<RefCell<Vec<Object>>>, old: Vec<Object> },
    Dict  { dict: Rc<RefCell<Dict>>,        old: Dict },
}
struct SaveRecord {
    handle: Rc<SaveHandle>,   // the savetype object points here
    journal_mark: usize,      // journal length at save time
    seen: HashSet<usize>,     // storage ptrs already snapshotted at this level
    gfx_depth: usize,         // gsave-stack depth at save time
    gfx_state: Box<GraphicsState>, // the save-time graphics state
}
// on Interp:
journal: Vec<JEntry>,
save_stack: Vec<SaveRecord>,
```

`SaveHandle` is `{ valid: Cell<bool> }` behind an `Rc`; the operand
object is a new `Value::Save(Rc<SaveHandle>)` variant (`savetype`,
prints as `-save-`, `eq` by `Rc::ptr_eq`). This is the object-model
touch the task was flagged for — one variant, no change to existing
representations.

Snapshots capture the **whole backing store** (`Vec<Object>` /
`Dict`), not the view window: array views share storage, so restoring
the store restores every view onto it. The `seen` key is the storage
pointer; ABA reuse is impossible because the journal entry itself
keeps the `Rc` alive.

### Write barriers

`Interp::journal_array(&PsArray)` / `journal_dict(&Rc<RefCell<Dict>>)`
called *before* mutating, from every operator that writes program-
visible array/dict contents: `def`, `put`, `putinterval`, `astore`,
`copy` (array/dict destinations), `undef`, `store`, the matrix
operators that fill a matrix operand (`currentmatrix`,
`identmatrix`, `defaultmatrix`, `invertmatrix`, `concatmatrix`,
matrix-form `transform` variants), and `definefont`'s FID write.
Both helpers are a no-op when `save_stack` is empty. Interpreter-
internal writes to `$error` are journaled too (they're VM dicts);
`errordict` likewise.

Nested saves: only the **top** record's `seen` gates logging — an
object mutated at an outer level and again after an inner save gets
one entry per level, each holding that level's pre-state. Inner
restore truncates to the inner mark; the outer entry survives.

### restore

1. Pop the operand; must be `Value::Save` whose handle is still
   valid **and** still present in `save_stack` — else
   `invalidrestore`.
2. Invalidate every newer record's handle (restoring an outer save
   discards inner ones; a later `restore` on those → `invalidrestore`,
   per PLRM).
3. Undo the journal in reverse down to `journal_mark`:
   `*data.borrow_mut() = old` (self-referential contents are safe:
   displaced objects with remaining handles are skipped by `Object`'s
   iterative `Drop`).
4. Graphics: truncate the gsave stack to `gfx_depth` and reinstate
   `gfx_state` — i.e. `grestoreall` to the save point. This reuses the
   Type 3 glyph-context snapshot mechanism (`glyph_snapshot` /
   `restore_glyph_snapshot`) rather than pushing an implicit `gsave`:
   the boundary state lives in the record, not on the gsave stack, so
   the program's own unbalanced `grestore`s can't pop it. Pinned:
   `save gsave gsave 5 setlinewidth restore currentlinewidth` → `1.0`
   in gs.
5. Truncate `save_stack`.

`grestoreall` (new operator) restores the innermost save's boundary
state (which stays available for that save's restore), or pops to the
bottom of the gsave stack when no save is live. (gs appears to differ
in the no-save case only because its job server wraps every program
in an implicit save.)

### vmstatus

`level used max` with `level = save_stack.len()` and fixed
`used/max = 1000000/16000000` — real byte accounting is meaningless
here; programs use `vmstatus` for the level or for "is there free
VM", and 15 MB of headroom answers the latter. Documented constant.

## Deliberate deviations (all safe under Rc)

- **No invalidrestore stack scan.** gs errors when the operand/dict/
  exec stacks hold composites created after the save
  (`save (x) exch restore` → invalidrestore; `save 3 dict begin
  restore` likewise). Detecting "created after" needs creation
  stamps on every composite; under `Rc` such objects simply remain
  valid and reachable, nothing dangles. Programs that would hit this
  error on Adobe run on here instead — the found-file philosophy
  (render, don't error). Recorded per-op in the module docs.
- **Files/fonts:** restore does not close files opened since the
  save, and does not undo `definefont` registry side effects beyond
  the font dict's own contents (the FID write *is* journaled).
- **`grestore` at a save boundary** pops through it (PLRM says stop);
  balanced gsave/grestore pairs — i.e. real programs — never notice.
  `grestoreall` does respect the boundary.
- **Strings are exempt by spec**, not deviation — noted here because
  it surprises people (pinned: gs leaves `(Xbc)` mutated).
- save/restore inside a Type 3 BuildChar or image data procedure is
  untested territory (the glyph machinery snapshots graphics state
  around glyphs); nothing guards it yet.

## gs pins used above

```
/s (abc) def save s 0 88 put restore s ==        % (Xbc)  strings exempt
/a [1 2 3] def save a 0 99 put restore a ==      % [1 2 3]
/d 2 dict def save d /k 1 put restore d /k known == % false
save 5 setlinewidth restore currentlinewidth ==  % 1.0
save gsave gsave 5 setlinewidth restore currentlinewidth == % 1.0 (grestoreall)
/s1 save def /s2 save def s1 restore s2          % undefined (userdict rolled back)
save (newstring) exch restore                    % invalidrestore (we deviate)
save 3 dict begin restore                        % invalidrestore (we deviate)
save dup type ==                                 % savetype
save save restore restore                        % ok (LIFO)
vmstatus                                         % level used max (3 ints)
```
