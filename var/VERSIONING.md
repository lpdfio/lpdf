# Lpdf Versioning

Lpdf uses `MAJOR.MINOR.PATCH` version numbers across all packages.  The
semantics differ from standard semver — this is intentional and explained
below.

---

## The core idea

**`MAJOR.MINOR` is the engine.  `PATCH` belongs to each package.**

The engine is the Rust core.  Every package that ships core `0.22.0` has a
version of `0.22.x`, where `x` counts that package's own changes since core
`0.22.0` was published.  Reading `MAJOR.MINOR` from any package version tells
you exactly which engine it contains — no runtime introspection or metadata
lookup needed.

---

## Packages

Six packages follow this scheme.  One is the engine, five ship it:

| Package | Repo | Published to |
|---|---|---|
| Core (the engine) | `lpdfio/lpdf` | GitHub Releases (wasm, WASI binary) |
| JS SDK | `lpdfio/lpdf-js` | npm `@lpdfio/lpdf` |
| .NET SDK | `lpdfio/lpdf-dotnet` | NuGet `Lpdfio.Lpdf` |
| PHP SDK | `lpdfio/lpdf-php` | Packagist `lpdfio/lpdf` |
| Python SDK | `lpdfio/lpdf-python` | PyPI `lpdfio-lpdf` |
| VS Code extension | `lpdfio/lpdf-vscode` | VS Code Marketplace `lpdfio.lpdf` |

---

## Two kinds of release

### Core release — `vX.Y.0`, everything ships

Publish `vX.Y.0` in `lpdfio/lpdf` through the Releases UI, with typed notes.
Core's release workflow publishes the engine, then dispatches to the five
other repos.  Each one publishes its own `X.Y.0` automatically, running the
new engine.  Nobody releases the SDKs or the extension by hand.

Every core release updates every package — none skips one, and none waits
for its own schedule.

Each core release is preceded by a **release candidate**, `vX.Y.0-rc.N`, on
the same commit.  It publishes the engine as a GitHub pre-release and has the
five repos build and test against it (their `rc.yml`), publishing nothing.
Core refuses `vX.Y.0` unless an RC tag points at its commit.  RC tags exist
in core only.  No package ever carries an `-rc` version.

### Package release — `vX.Y.Z`, one package ships

Publish `vX.Y.Z` (Z ≥ 1) in **one** SDK repo or the extension repo, through
that repo's Releases UI, with typed notes.  Only that package publishes.  It
still runs engine `X.Y.0`.  The other packages don't move.

```
Example:

  core 0.22.0 released
    → all six packages at 0.22.0

  two PHP-only fixes, then one extension-only change
    → PHP 0.22.1, PHP 0.22.2, extension 0.22.1
    → JS, .NET and Python stay at 0.22.0

  core 0.23.0 released
    → all six packages at 0.23.0, every patch counter back to 0
```

---

## Version positions

### Major — breaking epoch

A major bump signals a breaking change: removed or incompatible public API,
output format change, or equivalent.  Major bumps are rare and always
coordinated across all repos simultaneously.  All packages move to the same
new major version in the same release.

The project starts at `0.x` during development, moves to `1.0` at first
stable release, and is expected to stay at `1.x` for a long time.

The package major is not `LPDF_MAJOR_VERSION` in `license.rs`.  That number
decides which license keys the engine accepts, and bumping it invalidates
every issued key.  Neither number moves as a side effect of the other.

### Minor — core release counter

A minor bump is issued whenever the Rust core produces a new artifact — a
bug fix, performance improvement, or new capability.  It does not mean "new
feature" in the standard semver sense.

Core releases are always `MAJOR.MINOR.0`.  The core never has patch releases.
An engine fix, however small, is a new minor.

When core bumps minor, every package publishes a new release at
`MAJOR.MINOR.0`, resetting its own patch counter to zero.

### Patch — package-only changes

The patch counter is owned by each package independently.  It counts that
package's own changes since the last core release.

- **SDKs:** bug fixes, dependency bumps, internal refactors — anything that
  does not add, remove, or change the SDK's public API (see "Adapter API
  changes" below).
- **VS Code extension:** it has no public API, so every extension-only change
  is a patch, whether it's a fix or a new extension feature (a walkthrough,
  snippets, a new command).

Different packages at the same minor version will usually have different
patch numbers.  This is expected and correct.

```
Example — core 1.3 shipped; packages accumulate their own changes over time:

  @lpdfio/lpdf (npm)        1.3.2   ← 2 js-only fixes since core 1.3
  lpdfio/lpdf (Packagist)   1.3.5   ← 5 php-only fixes since core 1.3
  lpdfio-lpdf (PyPI)        1.3.1   ← 1 python-only fix since core 1.3
  Lpdfio.Lpdf (NuGet)       1.3.0   ← no .NET-only fixes yet
  lpdfio.lpdf (VS Code)     1.3.3   ← 3 extension-only changes since core 1.3

All five ship core 1.3.0.
```

---

## Rules the release workflows rely on

- **A package release never changes the engine.**  The workflows derive the
  engine from the tag: package `vX.Y.Z` downloads core `vX.Y.0`.  Getting a
  new engine into a package always means a core release.
- **Package releases go on the current minor only.**  A package is built from
  the tagged commit, normally `main`.  Tagging PHP `0.22.3` after core `0.23.0`
  exists would pair today's PHP code with the older engine.  There are no
  backports to an older minor.
- **Publish through the Releases UI, not with a bare `git push` of the tag.**
  The release's typed notes are what the changelog step writes to
  `CHANGELOG.md`.  A package tag pushed without a release fails that step
  (`changelog-plan.md`, case 3).
- **The engine download never falls back.**  If core `vX.Y.0` can't be
  downloaded after three tries, the package's `release.yml` fails instead of
  taking another engine.  So a package tag whose core doesn't exist (a typo,
  or tagging before core is out) stops the release rather than shipping a
  different engine than its version says.  Only `ci.yml` falls back to the
  latest core, because CI builds without a release tag.  (Fixed 2026-09-18.
  Before that, `release.yml` fell back silently.)

---

## Adapter API changes

Adding, removing, or changing a public method in an SDK is always
coordinated across all SDKs simultaneously.  It lands as a minor bump
driven by a corresponding core release.  SDK APIs do not change
independently — if the PHP SDK adds a method, every other SDK gains it
in the same release.

This keeps the SDKs behaviourally consistent: the set of public methods in the
Node SDK matches the Python SDK at any given minor version.

---

## Semver range operators

Standard package manager range operators map onto this scheme cleanly:

| Constraint | Meaning |
|---|---|
| `~1.3.0` | Exactly core 1.3, any package patch |
| `^1.3.0` | Any 1.x core release, any package patch |

Use `~` to pin to a specific core version.  Use `^` to float across core
updates within the same major.

**While Lpdf is `0.x`, `^` behaves like `~`.**  npm and Composer read
`^0.22.0` as `>=0.22.0 <0.23.0`, so users get package patches automatically
but a new engine only when they change the range.

---

## Why not standard semver?

Standard semver allocates three positions to: breaking change / new feature /
bug fix.  For a multi-package project built on a compiled core this creates a
gap: a core bug fix is not a "new feature" but still requires all packages to
republish to deliver the fix to users.  A separate "core patch" category needs
its own position, and semver has no fourth one.

This scheme repurposes the three positions as: breaking epoch / core release
counter / package-local work.  The core version is always readable directly
from the package version.  The standard range operators (`~` and `^`) still
behave usefully, just with different meanings attached to the positions.
