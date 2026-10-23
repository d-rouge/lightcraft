# Process V3: research context and proposal overlap

> **Last updated:** 2026-10-23 · **Change:** patch (retain original ablations, evidence limits and integration constraints)

This restores downstream research from `explore-feature` at `bd8c6501`, not a
new campaign. Current behaviour is specified in
[process-versions.md](process-versions.md). The full original notes and local
inputs are preserved separately in a checksum-verified local archive.
No private photo, reference-render media or proprietary implementation is shipped.

## Controlled ablation: source versus downstream softness

The primary private X-T1 portrait was the ISO 3200 frame ending `_1594.RAF`,
not the secondary ISO 6400 `_1954.RAF`. Its imported Portrait profile at 100%
contributed -6 Texture in addition to the displayed -5: effective Texture was
-11. For that profile/amount only, displayed +6 neutralized Texture. This is
not a general recommendation or a change to profile semantics.

Five 3264x4896 renders froze process-2 colour/tone, as-shot WB, exposure -0.14,
contrast 39, highlights -66, shadows 52, whites 8, blacks -4 and vibrance 18.
All used the same refined RAW source; A means no *additional* NR/sharpening.

| Variant | Luma NR | Colour NR | Sharpening | Effective Texture | Hat gradient / A | Hair gradient / A | Wall gradient / A |
|---|---:|---:|---:|---:|---:|---:|---:|
| A: spatial baseline | 0 | 0 | 0 | 0 | 1.000 | 1.000 | 1.000 |
| B: NR only | 61 | 25 | 0 | 0 | 0.675 | 0.655 | 0.413 |
| C: NR + sharpening | 61 | 25 | 32 | 0 | 0.831 | 0.812 | 0.413 |
| D: earlier look | 61 | 25 | 32 | -11 | 0.746 | 0.730 | 0.396 |
| E: lower NR | 30 | 25 | 32 | 0 | 1.103 | 1.102 | 0.573 |

Lossless PNGs were converted from sRGB to linear luminance. Half-open regions:
hat (1820,2030)-(1940,2150), hair (2060,2400)-(2120,2500), skin
(1940,2300)-(1980,2340), wall (96,128)-(352,384). Energy includes noise;
values above A are not recovered ground-truth detail.

Neutralizing Texture (D to C) raised hat/hair gradient about 11%, with wall
gradient +4.4% and wall luminance-Laplacian +3.5%. E raised wall gradient about
45% relative to D. The settings-only trial identified a downstream contributor;
it did not establish a better demosaicer, converter parity or a new default.

## Directional NR qualification

The original known-signal campaign at NR 61 / Detail 87 used 0.3-stop waves
plus 0.03-stop Gaussian noise, periods 4/8/16 and seven directions including
15/30/60/75 degrees. All 21 cases retained 95.7-98.8% contrast with squared
error 20.0-47.7% of noisy input. The frozen filter failed the first case at
64.4% contrast and 6.73 times input error.

White-noise residual variance stayed 0.06345 of input. Correlated noise at
sigma 1/2 gave 0.11726/0.31368 versus 0.11692/0.30111 before; tests cap
increases relative to legacy flat-noise variance at 5%. Preview checks at
1/2, 1/4 and 1/8 require less than 0.01-stop RMS versus resized full output.
Tests also cover weak/crossed texture, constant/tiny/empty planes, disabled NR,
legacy branches and process/NR cache transitions, including drafts.

NR Contrast was added after the initial directional-NR comparison and uses
fixed gradient energy, not measured sensor noise. The retained T2 heuristic
uses `0.005 * nr.eps`; correlated-noise residual rose 0.22300 to 0.24840
(+11.39%), with texture/error gains. Rejecting it does not prove the current
factor optimal. Earlier +22.4% and preview-failure claims were not reproduced
by the retained variant.

## Versioned real-photo observations and corrections

An earlier NR-only V3/V2 comparison held the C settings fixed. Hat gradient
increased 4.23%, hair 13.57%, skin 1.41% and wall 5.23% in the primary frame;
corresponding Laplacian increases were 2.90/10.79/1.32/3.10%. This is a
detail/noise proxy, not fidelity or qualification of later V3 sharpening edits.
The historical CPU/GPU check differed by at most one LSB.

The first Radius implementation reused the Texture/local Noise/Defringe plane
and leaked a local sharpening change outside its mask. Separate radius-keyed
planes fixed it; exact outside-mask and radius-independence tests plus GPU
cache transitions are the retained regression contracts.

The old converter collage mixed Adobe RGB values with sRGB and later still
changed profile, Texture, NR and sharpening together, with unknown reference
settings. Converter-superiority and +79.4% FFT texture claims were withdrawn.
Integer translation alone does not establish subpixel/lens registration.

Three historical GPU-enabled CLI trials had median total times 0.665 s (V2)
and 0.666 s (then-V3), but submission overlap and machine load preclude a
throughput claim. A 0.70 s 16 MP export does not verify 1 s per 24 MP.
Independent matched references, Bayer/non-RAW coverage and measured budgets
remain open; no performance or readiness increase is claimed by this recovery.

## Existing proposals: coordinate before publication

The following was checked on GitHub on 2026-10-22, not revalidated by this
documentation-only preservation. It records overlap, not interchangeable code.

| PR | Overlap | Required coordination |
|---|---|---|
| [#762](https://github.com/storytold/lightcraft/pull/762) | Radius/Detail, independent sharpening band and CPU/GPU support | Compare native-pixel scaling with this branch's resolution-relative radius and mask/cache tests; avoid presenting the controls as previously unaddressed. |
| [#2332](https://github.com/storytold/lightcraft/pull/2332) | Process V3 native-pixel sharpening, plus a RAW base look | Different implementation/scope; this branch does not change the RAW base look. |
| [#1349](https://github.com/storytold/lightcraft/pull/1349) | Luminance NR with wavelets, line protection, Detail/Contrast, CPU/GPU | Alternative to the structure-tensor/tangential filter; compare on independent quality/performance data, not titles. |
| [#1714](https://github.com/storytold/lightcraft/pull/1714) | Alternative conventional V3 NR, Contrast and edge-aware chroma | Comparison draft with shared legacy/cache concerns; this branch leaves colour NR unchanged. |
| [#1259](https://github.com/storytold/lightcraft/pull/1259) | Honour luminance Contrast | Narrower overlap; CPU/GPU and cache agreement remain necessary. |
| [#1672](https://github.com/storytold/lightcraft/pull/1672) | Closed proposal for V3 sharpening, Masking and colour NR | Historical context, not an active dependency or proof of merged behaviour. |

Several proposals independently call their process V3. The number is provisional
until maintainers coordinate ordering/composition; sharing the enum name does not
make rendering changes compatible. Do not silently change a process already
stored by another accepted renderer. Recheck heads/status before publishing.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-23 | patch | Restore controlled downstream evidence, rejected-claim caveats and competing-PR/process-numbering context without changing rendering. |
