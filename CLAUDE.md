# CLAUDE.md

This file is read automatically at the start of a Claude Code session in
this repo.

- **Project vision, priorities, and staged milestones:** see `INIT.md`.
- **Operational conventions** (build/test commands, commit hygiene,
  workflow, code quality bar): see `AGENTS.md`.
- **SDLC: issue → feature branch → PR**, not straight onto `main` — see
  `AGENTS.md`'s "Software development lifecycle" section before
  starting any nontrivial work. CI (`.github/workflows/ci.yml`) gates
  every PR on build/test/clippy/fmt; leave merging to a human.
- **All roadmap stages (1–20) are complete**, plus Stage 21 (standalone
  installability), Stage 22 (Korean/Japanese/Thai fonts), and Stage 23
  (gallery/site catch-up, `.ttc`, a second Korean face), all
  unplanned/post-roadmap (see `NOTES.md` for what shipped in each).
  **Read `HANDOFF.md` first when picking up new work** — it orients
  you in the architecture, lists the gotchas, and
  orders the remaining open-ended work (Gallery II, leftovers).
  `ROADMAP.md` has the full plan with per-task model routing.

Read both before writing any code. If this is the very first session on
this repo, `INIT.md`'s "First task" section is where to begin.

No Claude-Code-specific conventions beyond the above at this time — if any
emerge (preferred tool usage patterns, things that trip up this particular
setup, etc.), add them here rather than in `AGENTS.md`, which should stay
tool-agnostic.
