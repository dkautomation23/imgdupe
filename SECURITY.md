# Security Policy

imgdupe reads image files you point it at and, optionally, writes a shell
script listing `rm` commands built from the paths it found — it never
deletes or moves anything itself. Both of those are the security surface:
decoding an untrusted image, and generating a shell script from filenames
that may contain something odd.

## What counts as a vulnerability here

- A crafted image file (`.jpg`, `.png`, `.webp`, `.gif`, `.bmp`, `.tiff`)
  that causes memory corruption, a crash beyond a clean handled error, or
  arbitrary code execution while decoding.
- A filename that, once written into a `--delete-script` output, breaks out
  of the `rm -- '...'` quoting and lets a different command run when the
  script is executed — for example via an embedded single quote or shell
  metacharacter that isn't escaped correctly.
- Any code path where imgdupe deletes, moves, or overwrites a file on its
  own. It is documented as read-only except for the CSV/script files you
  explicitly ask it to write; a violation of that is a security bug, not a
  feature bug.
- Unbounded memory or CPU use from a single small file (a decompression-bomb
  style image).

Report these.

## What is not a vulnerability

- Two genuinely different images being grouped as duplicates, or a rotated /
  cropped duplicate not being detected — dHash's documented precision limits.
- Slow performance on very large folders, or the O(n²) grouping step being
  the bottleneck past tens of thousands of images — documented in "Honest
  limits."
- The keeper-selection heuristic (resolution first, file size second)
  choosing a file you'd have picked differently. That's a product decision,
  not a bug.

## Reporting a vulnerability

Preferred: open a report through
[GitHub Private vulnerability reporting](https://github.com/dkautomation23/imgdupe/security/advisories/new)
on this repository.

Alternative: email **hello@dkautomation.dev** with `imgdupe` in the subject
line.

Please include:
- the imgdupe version (`imgdupe --version`) and OS,
- the exact command and flags you ran,
- a minimal reproducing file. For a decode crash, a synthetic image (see
  `bench/generate.py` for how test images are built) is preferred over a
  real photo; for a delete-script quoting issue, the exact filename that
  triggers it is enough — you don't need to attach the image itself.

**First response within 3 business days.** After triage we'll tell you the
expected timeline for a fix and credit you in the release notes, if you want
that.

## Supported versions

Only the latest release is supported. If you're on an older tag, please
upgrade before reporting — the issue may already be fixed.
