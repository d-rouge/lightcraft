# X-T1 colour candidate: preservation and qualification

> **Last updated:** 2026-10-23 · **Change:** patch (preserve opt-in research candidate independently of demosaic and Process V3)

## Retained candidate, not an automatic profile

The candidate in [`X-T1.json`](../assets/camera-profiles/X-T1.json) is original
camera-JPEG fitting work, 30 accepted files / 110976 colour pairs with a
72x5x5 HSV table. Its SHA-256 is
`f2abda4f6b4f315800bf3310c7cfbf6d2d22ebae28198be64d3037c571945eb9`.
It is unchanged from source branch `explore-feature` at `bd8c6501`.

The training manifest, exact invocation and camera-JPEG settings were not
retained; no disjoint validation set can be certified. Parsing and attribution
are not colour qualification. Do not claim chart calibration, converter parity
or automatic activation. Upstream already excludes X-T1 from `BUNDLED`:
this proposal adds an optional candidate, not a removal of an upstream default.
Eight automatic profiles remain unchanged.

The experiment is explicitly local:

1. Copy the candidate to a dedicated directory as `X-T1.json`.
2. Launch with `LIGHTCRAFT_CAMERA_PROFILES=/absolute/path/to/directory`.
3. Restart after changes: the loader caches profiles per process.

This substitutes source colour before develop-process interpretation; it is
independent of both RAW reconstruction and versioned NR/sharpening.
Without the opt-in, X-T1 keeps the guarded per-file fit/fallback.

## F1/F2: provenance and independent colour validation

F1 must precede F2. These are future deliverables, not implemented workflow
capabilities or evidence that this preserved candidate is shippable.

- **F1: inventory.** Recover the 30 training inputs if possible; record SHA-256,
  source/author, permission/licence, camera/firmware, film simulation, WB,
  dynamic-range mode, sharpening, NR and acquisition scene/session. Mark unknown
  history unknown. Group duplicates/bursts/related scenes before freezing
  development and final-test membership. Never certify an overlapping or
  unknown set as an independent hold-out.
- **F2: reproducible fitting.** Extend existing `calibrate` with manifest inputs
  and accepted/rejected-file reports. Record actual selected inputs, invocation,
  sampling/order/seeds where relevant, output hashes and aggregation tolerance.
  Evaluate on disjoint scenes against the current fallback, with per-image and
  worst-case errors, rejection rates and neutral/skin/saturated-colour behaviour.
  If inputs cannot be recovered, fit a fresh documented candidate instead.

A camera JPEG is a look target, not measured scene colour. Chart calibration
requires legitimate reference values, unclipped exposures, independent sessions
and stable documented illumination, ideally measured. Nominal temperature alone
does not establish spectral equivalence to a standard illuminant. The existing
JPEG-fit command does not yet perform that workflow.

Keep private photographs and non-redistributable references local. A public
manifest does not grant rights to the inputs. Freeze exact code, settings,
reference hashes and evaluation membership before comparing candidates.
Promotion requires independent evidence and a maintainer decision.

## Existing X-T1 proposal

As checked on 2026-10-22,
[#2160](https://github.com/storytold/lightcraft/pull/2160) proposes an X-T1
PROVIA profile fitted from 217 photographs, alongside other camera profiles.
It is a separate provenance, not this 30-file candidate. Coordinate comparison
and qualification there before proposing another automatic profile; recheck
the PR's current state before publication.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-23 | patch | Preserve candidate identity, opt-in behaviour, original missing provenance and F1/F2 qualification tasks; record existing proposal overlap. |
