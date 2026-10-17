#!/usr/bin/env python3
"""Supply-chain and credential policy for .github/workflows (issue #602).

    python3 scripts/check_workflows.py            # check every workflow
    python3 scripts/check_workflows.py FILE...    # check these

Rules:

1. Every third-party action is pinned to a full commit SHA
   (`owner/repo@<40 hex>`, with the tag it stood for in a comment). A tag or
   branch can be moved under us; a SHA cannot.
2. `id-token: write` (the OIDC token that trusted publishing exchanges for a
   registry credential) appears only in a job that runs in the protected
   `release` environment, never at workflow level. So no pull-request job,
   and no job the owner has not approved, can mint a publishing credential.
   The one other job allowed an OIDC token is the GitHub Pages deploy (in
   the `github-pages` environment; actions/deploy-pages needs it). The
   crates.io and npm trusted publishers are configured to accept only the
   `release` environment, so its token cannot publish (RELEASING.md).
3. Secrets other than GITHUB_TOKEN are read only inside a `release`
   environment job.
4. No workflow uses `pull_request_target` (it runs with the base repo's
   secrets on a fork's code).
5. The release workflows (release.yml, release-fonts.yml) grant no write
   permission at workflow level, and every job in them declares its own
   `permissions:`.

The parser is deliberately small and line based: it relies on the
two-space layout every workflow here uses (jobs at column 2, job keys at
column 4), and fails loudly on a job it cannot read rather than guessing.
"""

import glob
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RELEASE_WORKFLOWS = ("release.yml", "release-fonts.yml")
OIDC_ENVIRONMENTS = ("release", "github-pages")
USES_RE = re.compile(r"^\s*(?:-\s+)?uses:\s*['\"]?([^\s'\"#]+)")
PINNED_RE = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+(?:/[^@\s]+)?@[0-9a-f]{40}$")
JOB_RE = re.compile(r"^  ([A-Za-z0-9_-]+):\s*(?:#.*)?$")
SECRET_RE = re.compile(r"\$\{\{\s*secrets\.([A-Za-z0-9_]+)")


def strip_comment(line):
    """Drop a trailing `# comment` that is not inside quotes."""
    out, quote = [], None
    for i, ch in enumerate(line):
        if quote:
            if ch == quote:
                quote = None
        elif ch in "'\"":
            quote = ch
        elif ch == "#" and (i == 0 or line[i - 1].isspace()):
            break
        out.append(ch)
    return "".join(out).rstrip()


def split_jobs(lines):
    """(top-level lines, {job: [lines]}). Lines keep their indentation."""
    top, jobs = [], {}
    in_jobs = False
    current = None
    for line in lines:
        bare = strip_comment(line)
        if bare and not bare.startswith(" "):
            in_jobs = bare.startswith("jobs:")
            current = None
            top.append(line)
            continue
        if in_jobs:
            m = JOB_RE.match(line)
            if m:
                current = m.group(1)
                jobs[current] = []
                continue
            if current is not None:
                jobs[current].append(line)
            elif bare:
                top.append(line)
        else:
            top.append(line)
    return top, jobs


def job_environment(body):
    for i, line in enumerate(body):
        bare = strip_comment(line)
        m = re.match(r"^    environment:\s*(.*)$", bare)
        if not m:
            continue
        value = m.group(1).strip().strip("'\"")
        if value:
            return value
        for nxt in body[i + 1:]:
            nb = strip_comment(nxt)
            if not nb.strip():
                continue
            if not nb.startswith("      "):
                break
            n = re.match(r"^\s+name:\s*['\"]?([^'\"]+)['\"]?$", nb)
            if n:
                return n.group(1).strip()
        return ""
    return None


def has_job_permissions(body):
    return any(re.match(r"^    permissions:", strip_comment(line)) for line in body)


def top_permissions(top):
    """The workflow-level permissions block as text ('' if absent)."""
    for i, line in enumerate(top):
        bare = strip_comment(line)
        if bare.startswith("permissions:"):
            value = bare[len("permissions:"):].strip()
            if value:
                return value
            block = []
            for nxt in top[i + 1:]:
                nb = strip_comment(nxt)
                if nb and not nb.startswith(" "):
                    break
                if nb.strip():
                    block.append(nb.strip())
            return "\n".join(block)
    return ""


def check_text(name, text):
    errors = []
    lines = text.splitlines()
    for n, line in enumerate(lines, 1):
        m = USES_RE.match(line)
        if m:
            ref = m.group(1)
            if ref.startswith("./") or ref.startswith("docker://"):
                continue
            if not PINNED_RE.match(ref):
                errors.append(f"{name}:{n}: action not pinned to a commit SHA: {ref}")
    if re.search(r"\bpull_request_target\b", "\n".join(strip_comment(x) for x in lines)):
        errors.append(f"{name}: uses pull_request_target")

    top, jobs = split_jobs(lines)
    top_text = "\n".join(strip_comment(x) for x in top)
    if re.search(r"id-token:\s*write", top_text):
        errors.append(f"{name}: id-token: write at workflow level")
    if re.search(r"^permissions:\s*write-all", top_text, re.M):
        errors.append(f"{name}: permissions: write-all")
    for s in SECRET_RE.findall(top_text):
        if s != "GITHUB_TOKEN":
            errors.append(f"{name}: secret {s} used at workflow level")

    if "jobs:" in top_text and not jobs:
        errors.append(f"{name}: could not read any job (unexpected layout)")
    for job, body in jobs.items():
        text_body = "\n".join(strip_comment(x) for x in body)
        env = job_environment(body)
        if re.search(r"id-token:\s*write", text_body) and env not in OIDC_ENVIRONMENTS:
            errors.append(f"{name}: job {job} has id-token: write outside the release environment")
        for s in SECRET_RE.findall(text_body):
            if s != "GITHUB_TOKEN" and env != "release":
                errors.append(f"{name}: job {job} reads secret {s} outside the release environment")

    if os.path.basename(name) in RELEASE_WORKFLOWS:
        perms = top_permissions(top)
        if perms not in ("{}", "read-all") and re.search(r"write", perms):
            errors.append(f"{name}: workflow-level permissions grant write ({perms!r})")
        if not perms:
            errors.append(f"{name}: no workflow-level permissions (set `permissions: {{}}` or read-only)")
        for job, body in jobs.items():
            if not has_job_permissions(body):
                errors.append(f"{name}: job {job} declares no permissions")
    return errors


def main(argv):
    files = argv or sorted(glob.glob(os.path.join(ROOT, ".github", "workflows", "*.yml")))
    errors = []
    for path in files:
        with open(path, encoding="utf-8") as f:
            errors += check_text(os.path.relpath(path, ROOT), f.read())
    for e in errors:
        print(e, file=sys.stderr)
    if errors:
        return 1
    print(f"{len(files)} workflows: actions pinned, OIDC and secrets confined to the release environment")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
