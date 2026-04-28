# Lpdf Versioning

Lpdf uses `MAJOR.MINOR.PATCH` version numbers across all packages.  The
semantics differ from standard semver — this is intentional and explained
below.

---

## The core idea

**The minor version is the core version.**

Every adapter that ships core `1.3.0` has a version of `1.3.x`, where `x` is
the number of adapter-internal changes made since that core was first
published.  Reading `MAJOR.MINOR` from any adapter version tells you exactly
which core it contains — no runtime introspection or metadata lookup needed.

---

## Version positions

### Major — breaking epoch

A major bump signals a breaking change: removed or incompatible public API,
output format change, or equivalent.  Major bumps are rare and always
coordinated across all repos simultaneously.  All packages move to the same
new major version in the same release.

The project starts at `0.x` during development, moves to `1.0` at first
stable release, and is expected to stay at `1.x` for a long time.

### Minor — core release counter

A minor bump is issued whenever the Rust core produces a new artifact — a
bug fix, performance improvement, or new capability.  It does not mean "new
feature" in the standard semver sense.

Core releases are always `MAJOR.MINOR.0`.  The core never has patch releases.

When core bumps minor, every adapter publishes a new release at
`MAJOR.MINOR.0`, resetting its own patch counter to zero.

### Patch — adapter-internal changes

The patch counter is owned by each adapter independently.  It counts
adapter-internal changes since the last core update: bug fixes, dependency
bumps, internal refactors — anything that does not add, remove, or change the
adapter's public API.

Different adapters at the same minor version will usually have different patch
numbers.  This is expected and correct.

```
Example — core 1.3 shipped; adapters accumulate their own fixes over time:

  @lpdfio/lpdf     1.3.2   ← 2 js-internal fixes since core 1.3
  lpdfio/lpdf      1.3.5   ← 5 php-internal fixes since core 1.3
  lpdf (PyPI)      1.3.1   ← 1 python-internal fix since core 1.3
  Lpdf (NuGet)     1.3.0   ← no .NET-internal fixes yet

All four ship core 1.3.0.
```

---

## Adapter API changes

Adding, removing, or changing a public method in an adapter is always
coordinated across all adapters simultaneously.  It lands as a minor bump
driven by a corresponding core release.  Adapter APIs do not change
independently — if the PHP adapter adds a method, every other adapter gains it
in the same release.

This keeps adapters behaviourally consistent: the set of public methods in the
Node adapter matches the Python adapter at any given minor version.

---

## Semver range operators

Standard package manager range operators map onto this scheme cleanly:

| Constraint | Meaning |
|---|---|
| `~1.3.0` | Exactly core 1.3, any adapter patch |
| `^1.3.0` | Any 1.x core release, any adapter patch |

Use `~` to pin to a specific core version.  Use `^` to float across core
updates within the same major.

---

## Why not standard semver?

Standard semver allocates three positions to: breaking change / new feature /
bug fix.  For a multi-adapter project built on a compiled core this creates a
gap: a core bug fix is not a "new feature" but still requires all adapters to
republish to deliver the fix to users.  A separate "core patch" category needs
its own position, and semver has no fourth one.

This scheme repurposes the three positions as: breaking epoch / core release
counter / adapter-local work.  The core version is always readable directly
from the adapter version.  The standard range operators (`~` and `^`) still
behave usefully, just with different meanings attached to the positions.
