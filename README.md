# pscat

A PostScript interpreter, written in Rust, for watching PostScript programs
draw live — built for personal fun rather than as a Ghostscript
replacement, though it's meant to grow toward broad spec coverage over
time.

## Status

Early stage — project scaffolding in progress. See `INIT.md` for the full
project vision and staged roadmap, and `AGENTS.md` for how the project is
being worked on.

This section should be kept up to date as implementation lands — replace
this paragraph with an accurate description of current capabilities once
there are any.

## Building & running

_To be filled in once there's something to build and run._

## Why

I used to write raw PostScript by hand in college and send it straight to
a LaserWriter. This is a from-scratch Rust interpreter built to relive
that — watching a hand-written recursive PostScript program draw itself,
live, without depending on a decades-old C codebase to do it.
