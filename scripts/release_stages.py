#!/usr/bin/env python3
"""The publishing stages of a pscat interpreter release (issue #602).

    python3 scripts/release_stages.py check-pin
    python3 scripts/release_stages.py bump-manifests 0.6.0
    python3 scripts/release_stages.py manifest --tag v0.6.0 --commit SHA \\
        --dist dist --crate target/package/pscat-0.6.0.crate
    python3 scripts/release_stages.py run --dist dist --crate FILE \\
        --stage draft --stage crates --stage npm --stage publish --stage plugin \\
        [--plan] [--fake DIR] [--report-dir DIR]
    python3 scripts/release_stages.py rehearse --dist dist --crate FILE

`.github/workflows/release.yml` drives these; docs/dev/RELEASING.md is the
procedure. Each stage checks what is already in its registry before it
changes anything:

- **absent** (a definite "not found" from the registry, never a failed
  call) -> publish it;
- **present and identical** (same digest, or same contents and the same
  source commit) -> skip it;
- **present and different, or present but unverifiable** -> halt (exit 3)
  for a human to reconcile. Nothing here ever overwrites, deletes or
  re-uploads a published version.

So a release that failed half way is resumed by running it again: stages
that already finished skip, and the rest run.

Stages, in release order:

- `draft`: a draft GitHub Release for the tag holding the archives,
  SHA256SUMS and release-manifest.json (the manifest goes last, so a draft
  without one is known to be incomplete).
- `crates`: `cargo publish` to crates.io.
- `npm`: `npm publish` of npm/; enabled only when npm/package.json exists.
- `publish`: makes the draft GitHub Release public, after re-checking it.
- `plugin`: checks every Claude Code plugin manifest carries the release
  version; enabled only when a .claude-plugin/ manifest exists.

`--plan` runs the same checks without changing anything and reports what
each stage would do. `--fake DIR` swaps every registry for a directory
(tests, and the release workflow's dry run). `rehearse` runs the
end-to-end, partial-failure and mismatch scenarios against fresh fake
registries with real artifacts.

Exit status: 0 done, 1 error (network, auth, a tool failed: safe to
retry), 3 halted (a mismatch: do not retry until a human looks).
"""

import argparse
import glob
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CRATE = "pscat"
MANIFEST_SCHEMA = 1
MANIFEST_NAME = "release-manifest.json"
SUMS_NAME = "SHA256SUMS"
ARCHIVE_EXTS = (".tar.gz", ".zip")
SEMVER_RE = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
SHA1_RE = re.compile(r"^[0-9a-f]{40}$")
USER_AGENT = "pscat-release (https://github.com/jeromebanks/postscript_interpreter)"
STAGES = ("draft", "crates", "npm", "publish", "plugin")

EXIT_OK = 0
EXIT_ERROR = 1
EXIT_HALT = 3


class Halt(Exception):
    """A registry holds something that is not this release, or that cannot
    be shown to be. A human has to reconcile it; retrying will not help."""


class StageError(Exception):
    """Something failed that may work on a retry (network, auth, a tool)."""


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def run(cmd, cwd=None, env=None, check=True):
    """Run a command, returning its stdout. A non-zero exit is a StageError
    carrying the command's stderr."""
    try:
        p = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True)
    except OSError as e:
        raise StageError(f"cannot run {cmd[0]}: {e}") from e
    if check and p.returncode != 0:
        raise StageError(f"{' '.join(cmd)} failed ({p.returncode}): {p.stderr.strip() or p.stdout.strip()}")
    return p


# --------------------------------------------------------------------------
# Repository facts: version, pin, manifests
# --------------------------------------------------------------------------


def cargo_version(root=ROOT):
    """The [package] version in Cargo.toml."""
    in_package = False
    with open(os.path.join(root, "Cargo.toml"), encoding="utf-8") as f:
        for line in f:
            s = line.strip()
            if s.startswith("["):
                in_package = s == "[package]"
            elif in_package:
                m = re.match(r'^version\s*=\s*"([^"]+)"', s)
                if m:
                    return m.group(1)
    raise SystemExit("no [package] version in Cargo.toml")


def pinned_index(root=ROOT):
    """(tag, sha256) from src/fontpacks/pinned.rs, or None while unpinned."""
    with open(os.path.join(root, "src/fontpacks/pinned.rs"), encoding="utf-8") as f:
        text = f.read()
    if re.search(r"PINNED_INDEX:\s*Option<Pinned>\s*=\s*None\s*;", text):
        return None
    tag = re.search(r'tag:\s*"([^"]+)"', text)
    sha = re.search(r'sha256:\s*"([0-9a-f]{64})"', text)
    if not (tag and sha):
        raise SystemExit("src/fontpacks/pinned.rs: cannot read PINNED_INDEX")
    return tag.group(1), sha.group(1)


def plugin_manifests(root=ROOT):
    """Every Claude Code plugin manifest in the repo (#569 adds them)."""
    out = []
    for pattern in (".claude-plugin/*.json", "**/.claude-plugin/*.json"):
        for p in glob.glob(os.path.join(root, pattern), recursive=True):
            rel = os.path.relpath(p, root)
            if rel.split(os.sep)[0] in ("target", "node_modules", ".git"):
                continue
            if os.path.basename(p) in ("plugin.json", "marketplace.json") and p not in out:
                out.append(p)
    return sorted(out)


def npm_manifest(root=ROOT):
    p = os.path.join(root, "npm", "package.json")
    return p if os.path.exists(p) else None


def json_versions(path):
    """The version fields of a JSON manifest, as (key path, value) pairs:
    the top-level `version`, marketplace `metadata.version` and
    `plugins[].version`, and MCP Registry `packages[].version`."""
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    found = []
    if isinstance(data.get("version"), str):
        found.append(("version", data["version"]))
    meta = data.get("metadata")
    if isinstance(meta, dict) and isinstance(meta.get("version"), str):
        found.append(("metadata.version", meta["version"]))
    for key in ("plugins", "packages"):
        for i, item in enumerate(data.get(key) or []):
            if isinstance(item, dict) and isinstance(item.get("version"), str):
                found.append((f"{key}[{i}].version", item["version"]))
    return data, found


def set_json_versions(path, version):
    data, found = json_versions(path)
    if not found:
        return False
    if "version" in data and isinstance(data["version"], str):
        data["version"] = version
    meta = data.get("metadata")
    if isinstance(meta, dict) and isinstance(meta.get("version"), str):
        meta["version"] = version
    for key in ("plugins", "packages"):
        for item in data.get(key) or []:
            if isinstance(item, dict) and isinstance(item.get("version"), str):
                item["version"] = version
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
        f.write("\n")
    return True


def set_cargo_version(root, version):
    path = os.path.join(root, "Cargo.toml")
    with open(path, encoding="utf-8") as f:
        lines = f.readlines()
    in_package = False
    for i, line in enumerate(lines):
        s = line.strip()
        if s.startswith("["):
            in_package = s == "[package]"
        elif in_package and re.match(r'^version\s*=\s*"', s):
            lines[i] = re.sub(r'"[^"]+"', f'"{version}"', line, count=1)
            break
    else:
        raise SystemExit("no [package] version in Cargo.toml")
    with open(path, "w", encoding="utf-8") as f:
        f.writelines(lines)


def bump_manifests(root, version):
    """Set every manifest's version; returns the files changed (relative)."""
    if not SEMVER_RE.match(version):
        raise SystemExit(f"{version} is not X.Y.Z")
    set_cargo_version(root, version)
    changed = ["Cargo.toml"]
    if set_lock_version(root, version):
        changed.append("Cargo.lock")
    return changed + bump_manifests_json_only(root, version)


def set_lock_version(root, version):
    """pscat's own entry in Cargo.lock, edited in place: no network, no
    re-resolution, so nothing else in the lockfile moves (CI builds with
    --locked and would catch a stale entry)."""
    path = os.path.join(root, "Cargo.lock")
    if not os.path.exists(path):
        return False
    with open(path, encoding="utf-8") as f:
        text = f.read()
    new, n = re.subn(r'(\[\[package\]\]\nname = "pscat"\nversion = )"[^"]+"', rf'\1"{version}"', text)
    if n != 1:
        raise SystemExit(f"Cargo.lock: expected one pscat package entry, found {n}")
    with open(path, "w", encoding="utf-8") as f:
        f.write(new)
    return True


def bump_manifests_json_only(root, version):
    """The plugin, npm and MCP Registry manifests that exist (none before
    #566/#569 add them)."""
    changed = []
    others = plugin_manifests(root)
    if npm_manifest(root):
        others.append(npm_manifest(root))
    server = os.path.join(root, "server.json")
    if os.path.exists(server):
        others.append(server)
    for p in others:
        if set_json_versions(p, version):
            changed.append(os.path.relpath(p, root))
    return changed


# --------------------------------------------------------------------------
# Artifacts: the release manifest and crate identity
# --------------------------------------------------------------------------


def archives_in(dist):
    names = sorted(n for n in os.listdir(dist) if n.endswith(ARCHIVE_EXTS))
    if not names:
        raise SystemExit(f"no release archives (*.tar.gz, *.zip) in {dist}")
    return names


# Members of a .crate that cargo writes at packaging time rather than copying
# from the source tree. Their bytes depend on the cargo version (the
# normalized Cargo.toml gains fields between releases), so the content digest
# leaves them out: Cargo.toml.orig is the source manifest, verbatim, and the
# VCS info's sha1 is compared on its own.
GENERATED_CRATE_FILES = ("Cargo.toml", ".cargo_vcs_info.json")


def crate_identity(path):
    """(content digest, vcs sha1 or None) of a .crate. The digest covers each
    source file's path and SHA-256, not tar metadata (mtimes) and not the
    files cargo generates, so two packagings of the same commit agree even
    when their bytes do not, or when different cargo versions made them."""
    lines = []
    sha1 = None
    has_orig = False
    try:
        with tarfile.open(path, "r:gz") as tar:
            for m in tar.getmembers():
                if not m.isfile():
                    continue
                f = tar.extractfile(m)
                data = f.read() if f else b""
                top, _, rel = m.name.partition("/")
                if rel == ".cargo_vcs_info.json":
                    try:
                        sha1 = json.loads(data).get("git", {}).get("sha1")
                    except (ValueError, AttributeError):
                        sha1 = None
                if rel == "Cargo.toml.orig":
                    has_orig = True
                if rel in GENERATED_CRATE_FILES:
                    continue
                lines.append(f"{m.name}\0{sha256_bytes(data)}\n")
    except (tarfile.TarError, OSError, EOFError) as e:
        raise Halt(f"{os.path.basename(path)} is not a readable .crate: {e}") from e
    if not has_orig:
        raise Halt(f"{os.path.basename(path)} has no Cargo.toml.orig; its source manifest cannot be compared")
    lines.sort()
    return sha256_bytes("".join(lines).encode()), sha1


def build_manifest(tag, commit, dist, crate_path):
    version = tag[1:] if tag.startswith("v") else tag
    if not SEMVER_RE.match(version) or tag != f"v{version}":
        raise SystemExit(f"{tag} is not vX.Y.Z")
    if not SHA1_RE.match(commit):
        raise SystemExit(f"{commit} is not a full commit SHA")
    assets = []
    for name in archives_in(dist):
        p = os.path.join(dist, name)
        assets.append({"name": name, "sha256": sha256_file(p), "size": os.path.getsize(p)})
    content, sha1 = crate_identity(crate_path)
    if sha1 != commit:
        raise SystemExit(f"{os.path.basename(crate_path)} was packaged from {sha1}, not {commit}")
    manifest = {
        "manifest_schema": MANIFEST_SCHEMA,
        "name": CRATE,
        "version": version,
        "tag": tag,
        "commit": commit,
        "assets": assets,
        "crate": {
            "name": os.path.basename(crate_path),
            "sha256": sha256_file(crate_path),
            "content_sha256": content,
        },
    }
    return manifest


def sums_text(assets):
    return "".join(f"{a['sha256']}  {a['name']}\n" for a in assets)


def write_manifest(manifest, dist):
    with open(os.path.join(dist, MANIFEST_NAME), "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
        f.write("\n")
    with open(os.path.join(dist, SUMS_NAME), "w", encoding="utf-8") as f:
        f.write(sums_text(manifest["assets"]))


def load_manifest(dist):
    with open(os.path.join(dist, MANIFEST_NAME), encoding="utf-8") as f:
        m = json.load(f)
    if m.get("manifest_schema") != MANIFEST_SCHEMA:
        raise SystemExit(f"{MANIFEST_NAME}: unknown manifest_schema {m.get('manifest_schema')}")
    return m


def release_files(manifest):
    """Asset upload order: archives, SHA256SUMS, then the manifest last."""
    return [a["name"] for a in manifest["assets"]] + [SUMS_NAME, MANIFEST_NAME]


def changelog_notes(version, root=ROOT):
    path = os.path.join(root, "CHANGELOG.md")
    body = ""
    if os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            text = f.read()
        m = re.search(rf"^## \[{re.escape(version)}\][^\n]*\n(.*?)(?=^## \[|\Z)", text, re.S | re.M)
        if m:
            body = m.group(1).strip() + "\n\n"
    return (
        f"{body}Each archive carries the licences (LICENSE-MIT, LICENSE-APACHE, LICENSE-OFL), "
        "THIRD_PARTY_NOTICES.md and the dependency notices (THIRD_PARTY_LICENSES.html). "
        f"{SUMS_NAME} has the SHA-256 of every archive; {MANIFEST_NAME} records the commit "
        "they were built from. Install guide: "
        "https://github.com/jeromebanks/postscript_interpreter/blob/main/docs/guides/getting-started.md"
    )


# --------------------------------------------------------------------------
# Backends: the real registries and a fake one
# --------------------------------------------------------------------------


class RealGitHub:
    """GitHub Releases through the gh CLI. Drafts never come back from a
    get-by-tag lookup, and a tag can have several, so releases are listed
    and matched on tag_name."""

    def __init__(self, repo):
        if not repo:
            raise SystemExit("set GITHUB_REPOSITORY or pass --repo")
        self.repo = repo

    def _api(self, *args):
        return run(["gh", "api", *args]).stdout

    def tag_commit(self, tag):
        p = run(["gh", "api", f"repos/{self.repo}/commits/{tag}", "--jq", ".sha"], check=False)
        if p.returncode != 0:
            if "No commit found" in p.stderr or "HTTP 404" in p.stderr or "HTTP 422" in p.stderr:
                return None
            raise StageError(f"cannot read tag {tag}: {p.stderr.strip()}")
        return p.stdout.strip()

    def releases(self, tag):
        out = self._api(
            "--paginate",
            f"repos/{self.repo}/releases?per_page=100",
            "--jq",
            ".[] | {id, tag_name, draft, assets: [.assets[] | {id, name, digest}]} | tojson",
        )
        found = []
        for line in out.splitlines():
            if not line.strip():
                continue
            r = json.loads(line)
            if r["tag_name"] == tag:
                found.append(r)
        return found

    def create_draft(self, tag, title, notes):
        out = self._api(
            "-X", "POST", f"repos/{self.repo}/releases",
            "-f", f"tag_name={tag}", "-f", f"name={title}", "-f", f"body={notes}", "-F", "draft=true",
        )
        return json.loads(out)["id"]

    def upload(self, release_id, path, name):
        self._api(
            "-X", "POST", "-H", "Content-Type: application/octet-stream",
            f"https://uploads.github.com/repos/{self.repo}/releases/{release_id}/assets?name={name}",
            "--input", path,
        )

    def asset_bytes(self, asset):
        p = subprocess.run(
            ["gh", "api", "-H", "Accept: application/octet-stream", f"repos/{self.repo}/releases/assets/{asset['id']}"],
            capture_output=True,
        )
        if p.returncode != 0:
            raise StageError(f"cannot download {asset['name']}: {p.stderr.decode(errors='replace').strip()}")
        return p.stdout

    def asset_sha256(self, asset):
        digest = asset.get("digest") or ""
        if digest.startswith("sha256:"):
            return digest[len("sha256:"):]
        return sha256_bytes(self.asset_bytes(asset))

    def publish(self, release_id):
        self._api("-X", "PATCH", f"repos/{self.repo}/releases/{release_id}", "-F", "draft=false")


class RealCrates:
    """crates.io: the HTTP API for lookups (not the sparse index, which lags
    after a publish), `cargo publish` to publish."""

    API = "https://crates.io/api/v1/crates"
    STATIC = "https://static.crates.io/crates"

    def _get(self, url):
        req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                return r.status, r.read()
        except urllib.error.HTTPError as e:
            return e.code, e.read()
        except (urllib.error.URLError, OSError) as e:
            raise StageError(f"GET {url}: {e}") from e

    def version(self, name, version):
        """{'checksum': ...} or None only on a definite 404."""
        status, body = self._get(f"{self.API}/{name}/{version}")
        if status == 404:
            return None
        if status != 200:
            raise StageError(f"crates.io answered {status} for {name} {version}")
        try:
            v = json.loads(body)["version"]
            return {"checksum": v["checksum"], "yanked": v.get("yanked", False)}
        except (ValueError, KeyError, TypeError) as e:
            raise StageError(f"crates.io: unreadable answer for {name} {version}: {e}") from e

    def download(self, name, version, dest):
        url = f"{self.STATIC}/{name}/{name}-{version}.crate"
        for attempt in range(6):  # the CDN can trail the API for a moment after a publish
            status, body = self._get(url)
            if status == 200:
                with open(dest, "wb") as f:
                    f.write(body)
                return
            time.sleep(5 * (attempt + 1))
        raise StageError(f"cannot download {url} ({status})")

    def publish(self, repo_root, commit, crate_path):
        head = run(["git", "rev-parse", "HEAD"], cwd=repo_root).stdout.strip()
        if head != commit:
            raise Halt(f"the checkout is at {head}, not the release commit {commit}")
        run(["cargo", "publish", "--locked"], cwd=repo_root)
        return os.path.join(repo_root, "target", "package", os.path.basename(crate_path))


class RealNpm:
    def view(self, name, version):
        p = run(["npm", "view", f"{name}@{version}", "dist.integrity", "--json"], check=False)
        if p.returncode == 0:
            out = p.stdout.strip()
            if not out:  # npm prints nothing for a version that does not exist
                return None
            return {"integrity": json.loads(out)}
        text = p.stdout + p.stderr
        if "E404" in text:
            return None
        raise StageError(f"npm view {name}@{version} failed: {text.strip()}")

    def pack_integrity(self, pkg_dir):
        out = run(["npm", "pack", "--dry-run", "--json"], cwd=pkg_dir).stdout
        return json.loads(out)[0]["integrity"]

    def publish(self, pkg_dir):
        run(["npm", "publish", "--provenance", "--access", "public"], cwd=pkg_dir)


class Fake:
    """A directory standing in for GitHub, crates.io and npm, with the
    semantics the stages rely on: drafts are found only by listing, a
    published release is immutable, a published crate or npm version is
    immutable, and each publish is logged. `fail-once` holds an operation
    name ("github.publish", "crates.publish", ...) that fails one time."""

    def __init__(self, root):
        self.root = root
        for d in ("github/assets", "crates", "npm"):
            os.makedirs(os.path.join(root, d), exist_ok=True)

    # -- plumbing
    def _path(self, *p):
        return os.path.join(self.root, *p)

    def _load(self, rel, default):
        p = self._path(rel)
        if not os.path.exists(p):
            return default
        with open(p, encoding="utf-8") as f:
            return json.load(f)

    def _save(self, rel, data):
        with open(self._path(rel), "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2)

    def _maybe_fail(self, op):
        p = self._path("fail-once")
        if os.path.exists(p):
            with open(p, encoding="utf-8") as f:
                want = f.read().strip()
            if want == op:
                os.remove(p)
                raise StageError(f"injected failure: {op}")

    def log(self, event):
        with open(self._path("publish.log"), "a", encoding="utf-8") as f:
            f.write(event + "\n")

    def events(self):
        p = self._path("publish.log")
        if not os.path.exists(p):
            return []
        with open(p, encoding="utf-8") as f:
            return [line.strip() for line in f if line.strip()]

    # -- GitHub
    def set_tag(self, tag, commit):
        tags = self._load("github/tags.json", {})
        tags[tag] = commit
        self._save("github/tags.json", tags)

    def tag_commit(self, tag):
        return self._load("github/tags.json", {}).get(tag)

    def releases(self, tag):
        out = []
        for r in self._load("github/releases.json", []):
            if r["tag_name"] == tag:
                out.append({"id": r["id"], "tag_name": tag, "draft": r["draft"],
                            "assets": [{"id": a["id"], "name": a["name"], "digest": None} for a in r["assets"]]})
        return out

    def create_draft(self, tag, title, notes):
        self._maybe_fail("github.create")
        rels = self._load("github/releases.json", [])
        rid = 1 + max([r["id"] for r in rels] or [0])
        rels.append({"id": rid, "tag_name": tag, "title": title, "draft": True, "assets": []})
        self._save("github/releases.json", rels)
        self.log(f"github.create {tag}")
        return rid

    def upload(self, release_id, path, name):
        self._maybe_fail("github.upload")
        rels = self._load("github/releases.json", [])
        r = next(r for r in rels if r["id"] == release_id)
        if not r["draft"]:
            raise StageError(f"fake: release {release_id} is published and immutable")
        if any(a["name"] == name for a in r["assets"]):
            raise StageError(f"fake: asset {name} already exists")
        aid = 1 + max([a["id"] for x in rels for a in x["assets"]] or [0])
        shutil.copyfile(path, self._path("github/assets", str(aid)))
        r["assets"].append({"id": aid, "name": name})
        self._save("github/releases.json", rels)
        self.log(f"github.upload {name}")

    def asset_bytes(self, asset):
        with open(self._path("github/assets", str(asset["id"])), "rb") as f:
            return f.read()

    def asset_sha256(self, asset):
        return sha256_bytes(self.asset_bytes(asset))

    def publish(self, release_id):
        self._maybe_fail("github.publish")
        rels = self._load("github/releases.json", [])
        r = next(r for r in rels if r["id"] == release_id)
        if not r["draft"]:
            raise StageError("fake: already published")
        r["draft"] = False
        self._save("github/releases.json", rels)
        self.log(f"github.publish {r['tag_name']}")

    # -- crates.io
    def version(self, name, version):
        p = self._path("crates", f"{name}-{version}.crate")
        return {"checksum": sha256_file(p), "yanked": False} if os.path.exists(p) else None

    def download(self, name, version, dest):
        shutil.copyfile(self._path("crates", f"{name}-{version}.crate"), dest)

    def publish_crate(self, crate_path):
        self._maybe_fail("crates.publish")
        dest = self._path("crates", os.path.basename(crate_path))
        if os.path.exists(dest):
            raise StageError("fake: crate version already exists")
        shutil.copyfile(crate_path, dest)
        self.log(f"crates.publish {os.path.basename(crate_path)}")
        return dest

    # -- npm
    def npm_view(self, name, version):
        return self._load(f"npm/{name}-{version}.json", None)

    def npm_publish(self, name, version, integrity):
        self._maybe_fail("npm.publish")
        self._save(f"npm/{name}-{version}.json", {"integrity": integrity})
        self.log(f"npm.publish {name}@{version}")


class FakeCrates:
    def __init__(self, fake):
        self.fake = fake

    def version(self, name, version):
        return self.fake.version(name, version)

    def download(self, name, version, dest):
        self.fake.download(name, version, dest)

    def publish(self, repo_root, commit, crate_path):
        return self.fake.publish_crate(crate_path)


class FakeNpm:
    """Integrity of the local package is the SHA-256 of package.json, which
    is all a test needs to tell two packages apart."""

    def __init__(self, fake):
        self.fake = fake

    def view(self, name, version):
        return self.fake.npm_view(name, version)

    def pack_integrity(self, pkg_dir):
        return "sha256-" + sha256_file(os.path.join(pkg_dir, "package.json"))

    def publish(self, pkg_dir):
        with open(os.path.join(pkg_dir, "package.json"), encoding="utf-8") as f:
            pkg = json.load(f)
        self.fake.npm_publish(pkg["name"], pkg["version"], self.pack_integrity(pkg_dir))


# --------------------------------------------------------------------------
# Stages
# --------------------------------------------------------------------------


class Ctx:
    def __init__(self, manifest, dist, crate_path, repo_root, github, crates, npm, plan):
        self.m = manifest
        self.dist = dist
        self.crate_path = crate_path
        self.root = repo_root
        self.github = github
        self.crates = crates
        self.npm = npm
        self.plan = plan
        self.local = {a["name"]: a["sha256"] for a in manifest["assets"]}
        self.local[SUMS_NAME] = sha256_file(os.path.join(dist, SUMS_NAME))
        self.local[MANIFEST_NAME] = sha256_file(os.path.join(dist, MANIFEST_NAME))


def check_provenance(remote, m, where):
    for key in ("name", "version", "tag", "commit"):
        if remote.get(key) != m[key]:
            raise Halt(f"{where}: {key} is {remote.get(key)!r}, this release is {m[key]!r}")


def the_release(ctx):
    """The one release (draft or not) for the tag, or None. Two -> halt."""
    rels = ctx.github.releases(ctx.m["tag"])
    if len(rels) > 1:
        raise Halt(f"{len(rels)} GitHub releases exist for {ctx.m['tag']} (ids "
                   f"{', '.join(str(r['id']) for r in rels)}); delete the extra drafts by hand")
    return rels[0] if rels else None


def check_tag(ctx):
    commit = ctx.github.tag_commit(ctx.m["tag"])
    if commit is None:
        raise Halt(f"tag {ctx.m['tag']} does not exist on GitHub; push it first")
    if commit != ctx.m["commit"]:
        raise Halt(f"tag {ctx.m['tag']} points at {commit}, the artifacts were built from {ctx.m['commit']}")


def reference_digests(ctx, rel):
    """What each asset of an existing release must hash to. A release that
    already carries a manifest is judged by that manifest (after its
    provenance is checked against this release): a re-run whose fresh
    builds differ bit-for-bit from the draft adopts the draft rather than
    halting on it. Otherwise, by this run's own artifacts."""
    by_name = {a["name"]: a for a in rel["assets"]}
    if MANIFEST_NAME in by_name:
        raw = ctx.github.asset_bytes(by_name[MANIFEST_NAME])
        try:
            remote = json.loads(raw)
        except ValueError as e:
            raise Halt(f"the release's {MANIFEST_NAME} is not JSON: {e}") from e
        check_provenance(remote, ctx.m, f"release {ctx.m['tag']} {MANIFEST_NAME}")
        ref = {a["name"]: a["sha256"] for a in remote.get("assets", [])}
        ref[SUMS_NAME] = sha256_bytes(sums_text(remote.get("assets", [])).encode())
        ref[MANIFEST_NAME] = sha256_bytes(raw)
        return ref, True
    return dict(ctx.local), False


def verify_release_assets(ctx, rel, allow_missing):
    """Check every asset of `rel`; returns the names still to upload.
    Halts on an unexpected, different or (unless allowed) missing asset."""
    ref, adopted = reference_digests(ctx, rel)
    by_name = {a["name"]: a for a in rel["assets"]}
    extra = sorted(set(by_name) - set(ref))
    if extra:
        raise Halt(f"release {ctx.m['tag']} has assets this release does not: {', '.join(extra)}")
    for name, asset in sorted(by_name.items()):
        got = ctx.github.asset_sha256(asset)
        if got != ref[name]:
            raise Halt(f"release {ctx.m['tag']}: {name} has SHA-256 {got}, expected {ref[name]}")
    missing = [n for n in release_files_from(ref) if n not in by_name]
    if missing and not allow_missing:
        raise Halt(f"release {ctx.m['tag']} is missing {', '.join(missing)}")
    for name in missing:
        if ref[name] != ctx.local.get(name):
            raise Halt(f"release {ctx.m['tag']} is missing {name}, and this run's copy differs "
                       "from the one its manifest records; reconcile the draft by hand")
    return missing, adopted


def release_files_from(ref):
    names = [n for n in ref if n not in (SUMS_NAME, MANIFEST_NAME)]
    return sorted(names) + [SUMS_NAME, MANIFEST_NAME]


def stage_draft(ctx):
    check_tag(ctx)
    rel = the_release(ctx)
    if rel is None:
        if ctx.plan:
            return "would create", f"a draft release {ctx.m['tag']} with {len(release_files(ctx.m))} assets"
        rid = ctx.github.create_draft(ctx.m["tag"], f"pscat {ctx.m['version']}", changelog_notes(ctx.m["version"], ctx.root))
        for name in release_files(ctx.m):
            ctx.github.upload(rid, os.path.join(ctx.dist, name), name)
        return "published", f"draft release {ctx.m['tag']} (id {rid})"
    missing, adopted = verify_release_assets(ctx, rel, allow_missing=rel["draft"])
    state = "draft" if rel["draft"] else "published release"
    if not missing:
        how = "its manifest's provenance and digests" if adopted else "this run's digests"
        return "skipped", f"{state} {ctx.m['tag']} already complete; matches {how}"
    if ctx.plan:
        return "would upload", f"{', '.join(missing)} to the draft {ctx.m['tag']}"
    for name in missing:
        ctx.github.upload(rel["id"], os.path.join(ctx.dist, name), name)
    return "published", f"completed the draft {ctx.m['tag']}: uploaded {', '.join(missing)}"


def check_crate_matches(ctx, remote_checksum, local_path, what):
    """Same bytes, or same contents from the same commit; else halt."""
    if remote_checksum == sha256_file(local_path):
        return "same .crate bytes"
    with tempfile.TemporaryDirectory() as d:
        dl = os.path.join(d, "remote.crate")
        ctx.crates.download(CRATE, ctx.m["version"], dl)
        if sha256_file(dl) != remote_checksum:
            raise Halt(f"crates.io served a .crate whose SHA-256 is not its recorded checksum ({what})")
        r_content, r_sha1 = crate_identity(dl)
    l_content, _ = crate_identity(local_path)
    if r_sha1 is None:
        raise Halt(f"{CRATE} {ctx.m['version']} on crates.io has no .cargo_vcs_info.json; "
                   "its source commit cannot be verified")
    if r_sha1 != ctx.m["commit"]:
        raise Halt(f"{CRATE} {ctx.m['version']} on crates.io was published from {r_sha1}, "
                   f"not {ctx.m['commit']}")
    if r_content != l_content:
        raise Halt(f"{CRATE} {ctx.m['version']} on crates.io has different contents from this release's .crate")
    return f"same contents and source commit {r_sha1[:12]}"


def stage_crates(ctx):
    v = ctx.crates.version(CRATE, ctx.m["version"])
    if v is not None:
        how = check_crate_matches(ctx, v["checksum"], ctx.crate_path, "existing version")
        return "skipped", f"{CRATE} {ctx.m['version']} already on crates.io ({how})"
    if ctx.plan:
        return "would publish", f"{CRATE} {ctx.m['version']} to crates.io"
    uploaded = ctx.crates.publish(ctx.root, ctx.m["commit"], ctx.crate_path)
    v = ctx.crates.version(CRATE, ctx.m["version"])
    if v is None:
        raise StageError(f"published {CRATE} {ctx.m['version']}, but crates.io does not list it yet; re-run to verify")
    if v["checksum"] != sha256_file(uploaded):
        raise Halt(f"crates.io's checksum for {CRATE} {ctx.m['version']} is not the .crate cargo uploaded")
    l_content, _ = crate_identity(ctx.crate_path)
    u_content, u_sha1 = crate_identity(uploaded)
    if (u_content, u_sha1) != (l_content, ctx.m["commit"]):
        raise Halt("the .crate cargo published differs from the one the release workflow verified")
    return "published", f"{CRATE} {ctx.m['version']} to crates.io (sha256 {v['checksum'][:12]})"


def stage_npm(ctx):
    pkg = npm_manifest(ctx.root)
    if not pkg:
        return "disabled", "no npm/package.json (the npm channel is not enabled)"
    with open(pkg, encoding="utf-8") as f:
        meta = json.load(f)
    if meta.get("version") != ctx.m["version"]:
        raise Halt(f"npm/package.json is version {meta.get('version')}, this release is {ctx.m['version']}")
    pkg_dir = os.path.dirname(pkg)
    local = ctx.npm.pack_integrity(pkg_dir)
    remote = ctx.npm.view(meta["name"], ctx.m["version"])
    if remote is not None:
        if remote["integrity"] != local:
            raise Halt(f"{meta['name']}@{ctx.m['version']} on npm has integrity {remote['integrity']}, "
                       f"this release packs {local}")
        return "skipped", f"{meta['name']}@{ctx.m['version']} already on npm (same integrity)"
    if ctx.plan:
        return "would publish", f"{meta['name']}@{ctx.m['version']} to npm"
    ctx.npm.publish(pkg_dir)
    return "published", f"{meta['name']}@{ctx.m['version']} to npm"


def stage_publish(ctx):
    check_tag(ctx)
    rel = the_release(ctx)
    if rel is None:
        if ctx.plan:
            return "would publish", f"the draft release {ctx.m['tag']} (after the draft stage creates it)"
        raise Halt(f"no release exists for {ctx.m['tag']}; run the draft stage first")
    missing, _ = verify_release_assets(ctx, rel, allow_missing=ctx.plan and rel["draft"])
    if not rel["draft"]:
        return "skipped", f"release {ctx.m['tag']} is already published and matches"
    if ctx.plan:
        return "would publish", f"the draft release {ctx.m['tag']}"
    ctx.github.publish(rel["id"])
    return "published", f"release {ctx.m['tag']} is public"


def stage_plugin(ctx):
    files = plugin_manifests(ctx.root)
    if not files:
        return "disabled", "no .claude-plugin/ manifest (the plugin channel is not enabled)"
    wrong = []
    for p in files:
        _, found = json_versions(p)
        for key, value in found:
            if value != ctx.m["version"]:
                wrong.append(f"{os.path.relpath(p, ctx.root)} {key}={value}")
    if wrong:
        raise Halt("plugin manifests do not carry the release version (run scripts/release.sh): " + "; ".join(wrong))
    return "skipped", f"{len(files)} plugin manifest(s) already at {ctx.m['version']}"


STAGE_FUNCS = {
    "draft": stage_draft,
    "crates": stage_crates,
    "npm": stage_npm,
    "publish": stage_publish,
    "plugin": stage_plugin,
}


def run_stages(ctx, stages):
    """Run stages in order, stopping at the first halt or error. Returns
    (exit status, results)."""
    results = []
    status = EXIT_OK
    for name in stages:
        try:
            outcome, detail = STAGE_FUNCS[name](ctx)
        except Halt as e:
            results.append({"stage": name, "result": "halted", "detail": str(e)})
            status = EXIT_HALT
            break
        except StageError as e:
            results.append({"stage": name, "result": "error", "detail": str(e)})
            status = EXIT_ERROR
            break
        results.append({"stage": name, "result": outcome, "detail": detail})
    for name in stages[len(results):]:
        results.append({"stage": name, "result": "not run", "detail": ""})
    return status, results


def report(manifest, results, status, plan, backend):
    return {
        "tag": manifest["tag"],
        "version": manifest["version"],
        "commit": manifest["commit"],
        "mode": "plan" if plan else "publish",
        "backend": backend,
        "status": {EXIT_OK: "ok", EXIT_ERROR: "error", EXIT_HALT: "halted"}[status],
        "stages": results,
        "assets": manifest["assets"],
        "crate": manifest["crate"],
    }


def report_markdown(rep):
    lines = [
        f"## Release {rep['tag']} ({rep['mode']}, {rep['backend']}): {rep['status']}",
        "",
        f"Commit `{rep['commit']}`",
        "",
        "| Stage | Result | Detail |",
        "|---|---|---|",
    ]
    for s in rep["stages"]:
        detail = s["detail"].replace("|", "\\|").replace("\n", " ")
        lines.append(f"| {s['stage']} | {s['result']} | {detail} |")
    lines += ["", "| Asset | SHA-256 |", "|---|---|"]
    for a in rep["assets"]:
        lines.append(f"| {a['name']} | `{a['sha256']}` |")
    lines.append(f"| {rep['crate']['name']} | `{rep['crate']['sha256']}` |")
    return "\n".join(lines) + "\n"


def make_ctx(args, fake_dir=None):
    manifest = load_manifest(args.dist)
    if fake_dir:
        fake = Fake(fake_dir)
        github, crates, npm = fake, FakeCrates(fake), FakeNpm(fake)
    else:
        github = RealGitHub(args.repo or os.environ.get("GITHUB_REPOSITORY"))
        crates, npm = RealCrates(), RealNpm()
    return Ctx(manifest, args.dist, args.crate, args.repo_root, github, crates, npm, args.plan)


# --------------------------------------------------------------------------
# Rehearsal: the acceptance scenarios against fake registries
# --------------------------------------------------------------------------


def altered_crate(src, dest):
    """A copy of a .crate with one file's contents changed: the same
    version, a different artifact."""
    with tarfile.open(src, "r:gz") as tin, tarfile.open(dest, "w:gz") as tout:
        changed = False
        for m in tin.getmembers():
            data = tin.extractfile(m).read() if m.isfile() else None
            if data is not None and not changed and m.name.endswith("/Cargo.toml.orig"):
                data += b"\n# not the released crate\n"
                m.size = len(data)
                changed = True
            tout.addfile(m, io.BytesIO(data) if data is not None else None)


def rehearse(dist, crate_path, repo_root, log=print):
    """Run every scenario; returns a list of (name, ok, detail)."""
    m = load_manifest(dist)
    outcomes = []
    all_stages = list(STAGES)

    def ctx_for(d, plan=False):
        fake = Fake(d)
        return fake, Ctx(m, dist, crate_path, repo_root, fake, FakeCrates(fake), FakeNpm(fake), plan)

    def results_of(results):
        return {r["stage"]: r["result"] for r in results}

    def expect(name, cond, detail):
        outcomes.append((name, bool(cond), detail))
        log(f"{'PASS' if cond else 'FAIL'}  {name}: {detail}")

    with tempfile.TemporaryDirectory() as tmp:
        # 1. End to end: plan, then publish, then a re-run that skips everything.
        d = os.path.join(tmp, "e2e")
        fake, ctx = ctx_for(d, plan=True)
        fake.set_tag(m["tag"], m["commit"])
        st, res = run_stages(ctx, all_stages)
        expect("plan changes nothing", st == EXIT_OK and fake.events() == [], f"exit {st}, {results_of(res)}")
        fake, ctx = ctx_for(d)
        st, res = run_stages(ctx, all_stages)
        r = results_of(res)
        expect("end-to-end publish", st == EXIT_OK and r["draft"] == r["crates"] == r["publish"] == "published",
               f"exit {st}, {r}")
        before = fake.events()
        st, res = run_stages(ctx, all_stages)
        r = results_of(res)
        expect("re-run of a finished release skips every stage",
               st == EXIT_OK and fake.events() == before and "published" not in r.values(), f"exit {st}, {r}")

        # 2. Partial failure: crates.io succeeds, publishing the release fails; a re-run finishes.
        d = os.path.join(tmp, "partial")
        fake, ctx = ctx_for(d)
        fake.set_tag(m["tag"], m["commit"])
        with open(os.path.join(d, "fail-once"), "w", encoding="utf-8") as f:
            f.write("github.publish")
        st, res = run_stages(ctx, all_stages)
        r = results_of(res)
        expect("partial failure stops after crates.io", st == EXIT_ERROR and r["crates"] == "published"
               and r["publish"] == "error", f"exit {st}, {r}")
        st, res = run_stages(ctx, all_stages)
        r = results_of(res)
        crate_pubs = [e for e in fake.events() if e.startswith("crates.publish")]
        expect("re-run completes without republishing", st == EXIT_OK and r["crates"] == "skipped"
               and r["publish"] == "published" and len(crate_pubs) == 1, f"exit {st}, {r}, crate publishes {len(crate_pubs)}")

        # 3. Mismatch: a different .crate already sits at this version.
        d = os.path.join(tmp, "mismatch")
        fake, ctx = ctx_for(d)
        fake.set_tag(m["tag"], m["commit"])
        altered_crate(crate_path, os.path.join(d, "crates", f"{CRATE}-{m['version']}.crate"))
        st, res = run_stages(ctx, all_stages)
        r = results_of(res)
        expect("different crate at the version halts", st == EXIT_HALT and r["crates"] == "halted"
               and r["publish"] == "not run", f"exit {st}, {r}")

        # 4. Mismatch: a published release for the tag with a different archive.
        d = os.path.join(tmp, "gh-mismatch")
        fake, ctx = ctx_for(d)
        fake.set_tag(m["tag"], m["commit"])
        rid = fake.create_draft(m["tag"], "x", "x")
        other = os.path.join(tmp, "other-archive")
        with open(other, "wb") as f:
            f.write(b"some other build")
        fake.upload(rid, other, m["assets"][0]["name"])
        fake.publish(rid)
        st, res = run_stages(ctx, all_stages)
        r = results_of(res)
        expect("different archive in a published release halts", st == EXIT_HALT and r["draft"] == "halted",
               f"exit {st}, {r}")

        # 5. Provenance: a draft whose manifest says another commit.
        d = os.path.join(tmp, "provenance")
        fake, ctx = ctx_for(d)
        fake.set_tag(m["tag"], m["commit"])
        rid = fake.create_draft(m["tag"], "x", "x")
        bad = dict(m, commit="0" * 40)
        badp = os.path.join(tmp, MANIFEST_NAME)
        with open(badp, "w", encoding="utf-8") as f:
            json.dump(bad, f)
        fake.upload(rid, badp, MANIFEST_NAME)
        st, res = run_stages(ctx, all_stages)
        r = results_of(res)
        expect("draft built from another commit halts", st == EXIT_HALT and r["draft"] == "halted", f"exit {st}, {r}")

        # 6. The tag moved: it no longer points at the commit the artifacts came from.
        d = os.path.join(tmp, "moved-tag")
        fake, ctx = ctx_for(d)
        fake.set_tag(m["tag"], "f" * 40)
        st, res = run_stages(ctx, all_stages)
        r = results_of(res)
        expect("tag pointing elsewhere halts before anything is created",
               st == EXIT_HALT and r["draft"] == "halted" and fake.events() == [], f"exit {st}, {r}")
    return outcomes


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)

    sub.add_parser("check-pin", help="fail while src/fontpacks/pinned.rs has PINNED_INDEX = None")

    b = sub.add_parser("bump-manifests", help="set every manifest's version")
    b.add_argument("version")

    mf = sub.add_parser("manifest", help="write release-manifest.json and SHA256SUMS into --dist")
    mf.add_argument("--tag", required=True)
    mf.add_argument("--commit", required=True)
    mf.add_argument("--dist", required=True)
    mf.add_argument("--crate", required=True)

    for name in ("run", "rehearse"):
        p = sub.add_parser(name)
        p.add_argument("--dist", required=True, help="directory with the archives and the manifest")
        p.add_argument("--crate", required=True, help="the .crate built from the release commit")
        p.add_argument("--repo-root", default=ROOT)
        if name == "run":
            p.add_argument("--stage", action="append", choices=STAGES, required=True)
            p.add_argument("--plan", action="store_true", help="check only; change nothing")
            p.add_argument("--fake", metavar="DIR", help="use a fake registry in DIR")
            p.add_argument("--repo", help="owner/name (default: $GITHUB_REPOSITORY)")
            p.add_argument("--report-dir", help="write release-report.json/.md here")

    args = ap.parse_args(argv)

    if args.cmd == "check-pin":
        pin = pinned_index()
        if pin is None:
            print("src/fontpacks/pinned.rs has PINNED_INDEX = None: publish and pin the first fonts-* "
                  "release before an interpreter release (docs/dev/RELEASING-FONTS.md).", file=sys.stderr)
            return EXIT_ERROR
        print(f"pinned font index: {pin[0]} sha256 {pin[1]}")
        return EXIT_OK

    if args.cmd == "bump-manifests":
        for f in bump_manifests(ROOT, args.version):
            print(f)
        return EXIT_OK

    if args.cmd == "manifest":
        m = build_manifest(args.tag, args.commit, args.dist, args.crate)
        write_manifest(m, args.dist)
        print(json.dumps(m, indent=2))
        return EXIT_OK

    if args.cmd == "rehearse":
        outcomes = rehearse(args.dist, args.crate, args.repo_root)
        failed = [n for n, ok, _ in outcomes if not ok]
        print(f"{len(outcomes) - len(failed)}/{len(outcomes)} scenarios behaved")
        return EXIT_ERROR if failed else EXIT_OK

    ctx = make_ctx(args, args.fake)
    if cargo_version(args.repo_root) != ctx.m["version"] and not args.fake:
        print(f"Cargo.toml is {cargo_version(args.repo_root)}, the manifest is {ctx.m['version']}", file=sys.stderr)
        return EXIT_HALT
    status, results = run_stages(ctx, args.stage)
    rep = report(ctx.m, results, status, args.plan, "fake" if args.fake else "real")
    md = report_markdown(rep)
    print(md)
    if args.report_dir:
        os.makedirs(args.report_dir, exist_ok=True)
        with open(os.path.join(args.report_dir, "release-report.json"), "w", encoding="utf-8") as f:
            json.dump(rep, f, indent=2)
        with open(os.path.join(args.report_dir, "release-report.md"), "w", encoding="utf-8") as f:
            f.write(md)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(md + "\n")
    return status


if __name__ == "__main__":
    sys.exit(main())
