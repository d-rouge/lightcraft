# Process versions

Every photo's develop settings record the **rendering process** they are interpreted by
(`DevelopSettings::process`, a `ProcessVersion` in `crates/develop/src/settings.rs`). Lightroom calls
this the process version. It lets the look of LightCraft improve (a new default tone curve, issue #146;
stronger Light sliders, issue #196) without changing a single photo someone has already edited.

| Process | What it changes |
|---|---|
| **V1** | LightCraft's rendering from before process versions existed. Frozen. |
| **V2** | Highlights and Shadows keep fine detail (issue #632): their base layer (`local::tone_base`) is the edge-aware filter of a separable median of log luminance (radius 0.15 % of the long edge, at most 24 px, `local::BASE_DETAIL`) instead of log luminance itself, so twigs, lines and texture take their surroundings' shift and keep their contrast; region edges stay sharp. CPU (`local::prepare`) and GPU prepare branch on `local::tone_base_detail` (0 = no median under V1); the base plane is cached per process (`local::BaseKey`). Renders without Highlights or Shadows (global or local) are the same as V1's. Tone map unchanged. Frozen. |
| **V3** (the latest) | Luminance NR protects coherent texture and edge transitions (`local::guided_nr`): a blurred Scharr structure tensor identifies a dominant direction, and a tangential average replaces part of the self-guided result there. NR Detail controls directional protection; NR Contrast retains edge contrast above a fixed gradient-energy threshold, not a calibrated noise floor. Positive lattice weights avoid bilinear interpolation blurring fine diagonals. NR radii, blend and preview scaling come from `local::nr_params`; CPU/GPU implement the same operation. Sharpening consumes Radius and Detail (`ctl:detail.sharpenRadius`, `ctl:detail.sharpenDetail`): Radius controls an independent Gaussian band, never the Texture/local Noise/Defringe band; Detail controls the halo clamp (0.20 at Detail 0 to 1.60 at Detail 100) and micro-contrast response. Process participates in `Plan::pre_nr_key`; the two detail bands are cached by their own radii. Colour NR and tone are unchanged from V2; V1/V2 retain legacy fixed sharpening and self-guided NR. Camera-profile additions are separate from process versions. |

V3 blends in cell-centred gradients in reduced previews so pixel-wide stripes still have a
detectable orientation. Its draft renders recompute wide planes after an NR change rather than
reusing V1/V2's approximate stand-ins; the more directional changes otherwise leave stale clarity.

Review correction (2026-10-20): the initial Radius implementation blended its radius into the
shared Texture band. A local sharpening mask could consequently change Texture outside the mask.
CPU and GPU now use independent planes; tests assert exact outside-mask pixels and fresh/cached
agreement. V1/V2 use the old radius in both bands and share the buffer when the radii agree.
The V3 sharpening radius is `max(radius * 0.0006 * px_per_long, 0.6)`; Texture remains
`max(0.0018 * px_per_long, 0.6)`. These are resolution-relative widths, not fixed native pixels.

## The rule

**A change that alters how existing settings render ships as a new process. V1 and V2 are frozen.**

- A deliberate change of look (tone model, slider response, default curve, how a stage interprets a
  value) goes into a new process. Photos stay on the process they have until someone runs Update to
  Current Process (or Reset).
- Bug fixes that make output correct where it was broken (a crash, NaN or black output, a decoder
  error, CPU and GPU disagreeing) are not looks and can land in place, as before.
- Changes to how a file is decoded or which camera colour it gets change the source, not how settings
  are interpreted; the process doesn't reach that stage today.

**Neutral display tone map** (JPEG, PNG and other display-referred sources with Contrast, Whites and
Blacks at 0) is the identity up to white: the SDR shoulder no longer pulls 1.0 down to 0.9875 (8-bit
255 to 254, issue #1945). It landed in place as a bug fix, since the tone map promised identity at
neutral settings; edited curves keep the shoulder and their V1 look.

## Adding a process

1. Add a variant to `Process`, append it to `Process::ALL` (oldest first), give it its number in
   `Process::version` and make it `Process::LATEST`.
2. The compiler then points at every `match` on `Process`. The base tone map is chosen in one place,
   `lightcraft_pipeline::finish::base_tone` (both the CPU and the GPU renderer take it from there): give
   the new process its own arm and leave the old ones alone. A process that changes another stage branches there the same
   way, on `s.process.process()`; if that stage is cached (`Plan::lin_key`, the planes in `local.rs`),
   add the process to its key.
3. Keep `settings_saved_before_process_versions_load_as_v1` (`crates/develop/src/tests_process.rs`)
   passing: settings saved without the field must stay V1, not become the new latest. That is the
   field's `#[serde(default = "ProcessVersion::legacy")]`; the struct-level `Default` gives new
   settings the latest instead.
4. Render a few procedural scenes on both processes and compare, and update `docs/parity-checklist.md`.

## What gets which process

| | Process |
|---|---|
| A newly imported, merged or Lightroom-imported photo | the latest |
| Settings saved without the field (catalogs and their snapshots, versions and history, `lc:settings` in XMP sidecars, browser libraries: everything from before process versions), or with a damaged value | V1 |
| Reset (`develop.reset`) | the latest |
| Reset of a section or a slider | unchanged |
| Update to Current Process (`develop.updateProcess`) | the latest, for photos on an older one; one undo step. The menu item follows the selection; a call with `ids` / `id` acts on those photos whatever is selected |
| Virtual copy, duplicate | the source's |
| Copy / paste / sync / Auto Sync / presets | unchanged (see below) |
| `crs:` edits (XMP from other raw developers, Lightroom catalogs, imported presets) | unchanged; a new photo has the latest |
| Restoring a version or history step | the one it had |

**Copy, paste, sync and presets** never carry the process: it is in no settings group, so the photos
they change keep their own and interpret the pasted values with it. (Lightroom offers the process
version as a separate checkbox when syncing; LightCraft's groups are coarser and keep it out.) Partial
settings JSON that names `process` itself (`develop.merge`, a hand-written preset) sets it.

**Lightroom edits** are re-interpreted by LightCraft's mapper (`crates/engine/src/crs.rs`), which is
written against the current rendering. `crs:ProcessVersion` numbers Adobe's renderer, not ours, and is
ignored; a photo imported with Lightroom edits is on LightCraft's latest process, and an existing
LightCraft photo that receives them keeps its own.

**A number this build doesn't know** (a library or sidecar written by a newer LightCraft) loads, is
kept when saved, and renders with the newest process this build has. Update to Current Process never
moves a photo to an older process; Reset does move it to this build's latest.

**A damaged value** (anything but a whole number from 0 to 2^32 - 1; `2.0` counts as 2) reads as V1,
like a missing field, and costs nothing else: the rest of the settings load from the catalog, a
snapshot, the op log or an `lc:settings` sidecar as usual. The damaged value isn't kept: the photo is
saved as V1.

## Storage

The process is saved as `"process": <number>` in the settings JSON, except V1, which is left out.
Settings made before process versions existed are therefore saved exactly as before, and their
preview-cache keys (`DevelopSettings::hash64`) don't change. `develop.get` (MCP `get_develop`) always
states the process.
