# selftest/ — the PS-native verification pass

```sh
./scripts/selftest.sh
```

Runs every library's `%%SelfTest` blocks, then strict-lints every
rendering driver in `drivers/`. `docs/SELFTEST.md` is the full writeup:
the convention, the assertion vocabulary, which defect classes this
covers, and — just as important — which it doesn't.

## Why drivers exist at all

A bare `lib/*.ps` file draws nothing when it's loaded; it only defines
procedures. `--lint` judges a *finished run*, so it has nothing to look
at without a program that actually calls into the library. Each file in
`drivers/` is that program for one library.

## The two rules a driver keeps

**1. One `showpage` per independently-checked scenario.**

`--lint`'s blank-page check is per-page. Two scenarios sharing a page
means the second one's ink hides whether the first drew anything at
all — so a real regression in the first scenario reports clean. That is
a mistake one deleted `showpage` away in ordinary editing, so it isn't
left to author discipline: every driver declares `%%Pages: N`, and
lint's `page-count` check fails the run when the page count doesn't
match.

**2. Nothing left behind.**

Strict lint gates all five checks, not just blank-page: a leaked
operand, an unmatched `gsave`, an unmatched `begin`, and the page count
are all fatal too. Wrap each scenario `gsave`/`grestore` and consume
what you push — several artkit procedures return a value the caller is
expected to drop (`tfblock`, `tfcols`, `tfflow`).

Note that this couples drivers to `lint::check`'s finding set as it
grows: a *new* lint check added later applies to every driver here
retroactively, and can turn a passing driver red without anyone having
touched it. That's the intended direction — a driver is supposed to be
exemplary PostScript — but it means adding a lint check means running
`./scripts/selftest.sh` too, not just `cargo test`.

## Headers a driver needs

```postscript
%%Pages: 9                 % checked against reality by lint
%%SelfTestPage: 200x200    % the --page scripts/selftest.sh renders at
```

`%%Pages:` is DSC's own header, not an invention here. `%%SelfTestPage:`
is local to this directory: the scenarios are laid out for a specific
canvas, and the script has to know which one.

## Choosing scenarios

Prefer geometries that have *actually* rendered blank under a real
defect over a pretty specimen sheet — the paintkit driver's first three
scenarios are the three that did (PR #76, rounds 3 and 4). A scenario
that produces only a handful of stray pixels is a weak test even when
it passes: it's one tuning change away from flaking. Render the driver
and look at the pages when you add one.

Drivers are checks, not gallery pieces. Nothing here is published, and
no PNGs are committed.
