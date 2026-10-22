# RAW parity: cameras, formats, demosaic, colour and lens profiles

> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-21 · **Change:** minor (Minolta MRW decoded, 6 bodies verified; lossy JPEG XL DNG tiles with a non-linear declared transfer curve decode; Canon ColorData Kelvin; X-Trans CMOS 4 bodies take the X-T3's spectral matrices; X-Trans work: versioned shared diagnostics correct HF/chroma blind spots and timing, unsafe noise inputs refused, unqualified X-T1 colour candidate opt-in, historical metric limits corrected, follow-up qualification plan recorded) · **Target:** Adobe Lightroom Classic 15.5 / Lightroom 9.6 (Camera Raw 18.7)

Raw processing is the deepest single dimension of Lightroom parity: a photographer whose camera
does not decode, or decodes with the wrong colour, cannot switch. This document measures it model by
model. The numbers feed [target-app-parity.md](target-app-parity.md); individual shortfalls are
ranked in [gaps.md](gaps.md). Details per format: [cr3.md](cr3.md),
[raf-compression.md](raf-compression.md), [sony-lens-corrections.md](sony-lens-corrections.md),
[sony-ilce7rm6-lens-corrections.md](sony-ilce7rm6-lens-corrections.md), [nikon-lens-corrections.md](nikon-lens-corrections.md),
[camera-preview-colour.md](camera-preview-colour.md), [denoise.md](denoise.md).

## Headline

| Measure | Value | Kind |
|---|---:|---|
| Adobe-listed camera models | **1,446** | measured: [raw/adobe-cameras.tsv](raw/adobe-cameras.tsv) |
| Decoded from sensor data and **verified** on a real file | **551** (38%) | measured: [raw/evidence.tsv](raw/evidence.tsv) |
| Decoder path should handle it, **unverified** | **656** (45%) | measured from the format rules (631 of them DNG, mostly phones) |
| **Preview-only** (embedded JPEG) | **201** (14%) | measured |
| **Unsupported** (not imported, refused or decodes wrongly) | **38** (3%) | measured |
| Models added since 2017 (Camera Raw 10+): verified / unverified / preview / unsupported | 182 / 542 / 22 / 8 of 754 | measured |
| **Share of photographers whose camera fully works** (decodes from sensor data), use-weighted | **~94%** | estimated: measured classes × written usage weights × per-format confidence |
| …of which on a body we have verified | **~85%** | estimated, same weights |
| Camera colour: Adobe profiles vs ours | 1,500 Adobe Standard profiles, 481 models with Camera Matching profiles vs **8 bundled profiles** + per-file fits + 52 spectral matrices (plus same-sensor aliases); X-T1 candidate is opt-in only | measured (bundle listing, Adobe list, `crates/engine/src/camera_profiles.rs`) |
| Lens profiles: Adobe vs ours | **3,635 vs 0** (in-file corrections only) | measured |
| **RAW readiness** (decoding 60%, colour 25%, lens 15%) | **~60%** (range 54–64%) | estimated, see [Scores](#scores) |

Re-measure with `python3 docs/raw/coverage.py` (standard library only): it classifies every Adobe
model from the format rules in the script (which mirror `crates/raw/src`) plus the per-model evidence,
rewrites [raw/coverage.tsv](raw/coverage.tsv) and prints the tables below; `--check` fails when the
committed table is stale.

Folder discovery, desktop file pickers, CLI and MCP share
`lightcraft_engine::import::EXTENSIONS`. The list includes formats that may open only from an
embedded preview or report a decoder error; an extension is not a promise of sensor decoding.
The 2026-10-12 release CLI check of the public DSC-R1 SR2 and DSC-F828 SRF samples found the
same photos and decode status through recursive and direct import. DSC-F828 moves from
unsupported to preview-only in the counts; sensor-decoding readiness is unchanged.

## How it was measured

- **Adobe's list**: helpx "Camera Raw supported cameras" (the live page refuses scripted requests, so
  the Wayback Machine capture of 2026-10-03 was parsed): 1,446 camera rows from 40 makers after
  dropping profile-only rows; maker, model, extension(s), Camera Matching profile, minimum Camera Raw
  version. The year a model was added is approximated from that Camera Raw major version.
- **What we decode**: the decoders and their refusals in `crates/raw/src` (`lib.rs` routing, `vendor/*.rs`
  coding variants, `crx.rs` CRX limits, `orf.rs` / `srw.rs` compressed refusals), the import list
  `lightcraft_engine::import::EXTENSIONS`, and `KNOWN_UNSUPPORTED` in `crates/raw/tests/corpus.rs`. Doc
  claims were not used.
- **Verified** means a real file from that body decoded from sensor data in: the corpus test over the
  `RAW_SAMPLES` manifest (`xtask/src/main.rs`, 105 CC0 files; bodies whose sample is in
  `KNOWN_UNSUPPORTED` excluded), `crates/raw/tests/cr3_corpus.rs` (3 bodies exact against an external decoder;
  the EOS R5 Mark II pinned, with a QP-map check of 33,665 private files; 22 more bodies pinned on one CC0
  file each and checked against the camera JPEG, [cr3.md](cr3.md#further-bodies-one-cc0-file-each)), the RAF reference
  check ([raf-compression.md](raf-compression.md)), and the raw.pixls.us bulk checks recorded in issue
  #535 (all 100 Sony bodies there; 87 decode) and PR #215 (every Panasonic / Leica RW2 / RWL / RAW body
  there, 178 files from 118 bodies), and a recorded check of 38 private Leica Q3 DNGs (2026-10-11: every file
  decoded and developed at full size, 9520 × 6336 and 6336 × 9520; mean CIE76 ΔE to the camera JPEG 6.3–13.9
  at 400 px; the files are not committed). Bulk-check body lists were matched to Adobe names through the
  raw.pixls.us folder names. Checks without a body list (≈ 60 NEFs, 35 CR2 bodies for the CFA layout)
  are not counted as verified.
- **2026-10-12 public sweep:** 30 further models on 40 CC0 files, independently downloaded and
  decoded on main `25ef65c8`; [raw/coverage-sweep.tsv](raw/coverage-sweep.tsv) records the URLs,
  SHA-256, dimensions, FNV-1a sensor hashes and render/reference luminance correlations (0.6801
  to 0.9998). The hashes pin LightCraft's own output. The regression test is
  `crates/raw/tests/coverage_sweep_corpus.rs`; downloads are in `RAW_SAMPLES`. Ten references are
  embedded TIFF images; the rest are JPEGs. FFF TIFF previews are not display-ready and check
  image structure only. Eleven candidate models were excluded for identity, provenance or
  missing/mismatched reference images. D6 / Z 6 / Z 7 verification covers compressed NEF only;
  uncompressed remains preview-only (#671), as do Z 8 HE / HE* files. Own-matrix phone, drone
  and other DNGs can decode correctly while rendering darker than their camera JPEG (#1277).
  Verified means sensor data decoded, not colour parity.
- **Usage weights** ([raw/usage-weights.tsv](raw/usage-weights.tsv)): maker buckets from 2025 market
  data (Nikkei production: Canon ~36%, Sony ~29%, Nikon ~14%, Fujifilm ~12% of mirrorless; BCN retail:
  Sony 29.9%, Canon 27.4%, Nikon 15.1%), adjusted for the DSLR installed base and for the makers named
  in this repo's 325 user issues (Sony 33, Nikon 31, Canon 26, phones 23, Fujifilm 9, Panasonic / Leica
  8, OM / Olympus 6, Pentax / Ricoh 6). Inside each bucket, 80% goes to the popular bodies of the last
  ~8 years (listed in the file), 20% to the bucket's other Adobe models.
- **Per-format confidence** for unverified bodies (the chance their files decode correctly): DNG, CR2,
  RAF, RW2 0.95; NEF, ARW, PEF 0.9; ORF, SRW, SR2 0.8; **CR3 0.6** (the CRX decoder refuses several
  coding variants; 26 bodies are verified, every CC0 CR3 on raw.pixls.us decodes).

## By maker (measured)

| Maker | Adobe models | verified | unverified | preview | unsupported | Main reason for preview / unsupported |
|---|---:|---:|---:|---:|---:|---|
| Sony | 149 | 94 | 43 | 10 | 2 | DSLR-A200…A390 and A850 12-bit compressed ARW (preview, #535); A100 remains classified unsupported in the recorded evidence; DSC-F828 SRF imports as preview-only; DSC-V3 preview not verified |
| Canon | 134 | 95 | 16 | 21 | 2 | CRW (2000–2004 bodies) preview; EOS-1D / 1Ds TIFF raws not imported |
| Samsung | 128 | 10 | 109 | 9 | 0 | compressed SRW (NX1, NX30, NX300, NX500, NX2000, NX3000, NX3300, NX mini, Galaxy NX) |
| Apple | 113 | 4 | 109 | 0 | 0 | — (DNG / ProRAW) |
| Panasonic | 104 | 93 | 11 | 0 | 0 | — |
| Nikon | 99 | 77 | 19 | 0 | 3 | COOLPIX 5700 / 8400 / 8800 decode wrongly (two stacked half-height fields, CMYG read as RGB); HE / HE★ is a per-shot setting, preview-only |
| Fujifilm | 97 | 65 | 9 | 23 | 0 | pre-X FinePix RAF without a raw IFD (estimated split) |
| Google | 88 | 5 | 83 | 0 | 0 | — |
| Olympus | 71 | 17 | 3 | 51 | 0 | **compressed ORF** |
| Leica | 61 | 27 | 34 | 0 | 0 | — |
| Xiaomi, Oppo, LG, Huawei, OnePlus, Motorola, Nokia | 139 | 0 | 139 | 0 | 0 | — (DNG) |
| Pentax / Ricoh | 60 | 40 | 20 | 0 | 0 | — |
| Hasselblad | 34 | 7 | 2 | 25 | 0 | Hasselblad-compressed 3FR / FFF |
| Phase One / Leaf / Mamiya | 59 | 1 | 0 | 43 | 15 | IIQ, MOS preview; TIFF raws not imported (MEF decodes) |
| Casio | 26 | 0 | 26 | 0 | 0 | — (DNG) |
| DJI | 21 | 4 | 17 | 0 | 0 | DNG |
| OM Digital Solutions | 7 | 0 | 0 | 7 | 0 | **compressed ORF** |
| Kodak, Konica Minolta, Epson, Sigma, GoPro, Contax, others | 56 | 12 | 16 | 13 | 15 | DCR, GPR not imported; KDC, X3F preview (Epson ERF and Minolta MRW decode) |
| **All** | **1,446** | **551 (38%)** | **656 (45%)** | **201 (14%)** | **38 (3%)** | |

## By use (estimated)

| Bucket | Weight | Camera fully works | On a verified body | Why |
|---|---:|---:|---:|---|
| Canon | 33% | 95% | 93% | 18 of the 22 popular bodies write CR3; 26 CR3 bodies verified (raw.pixls.us has a CC0 file for each), the rest at 0.6 |
| Sony | 27% | 98% | 92% | bulk-checked |
| Nikon | 16% | 98% | 89% | decoders cover every NEF coding but HE; 77 bodies verified |
| Fujifilm | 10% | 94% | 92% | |
| Panasonic | 4% | 100% | 98% | bulk-checked |
| OM / Olympus | 3% | 6% | 5% | compressed ORF |
| Phones | 3% | 93% | 4% | DNG |
| Pentax / Ricoh | 1.5% | 99% | 80% | |
| Leica | 1% | 99% | 72% | the Q3 verified, 13 more bodies pinned on CC0 files |
| DJI | 1% | 96% | 19% | |
| Medium format and others | 0.5% | 29% | 8% | IIQ, 3FR, X3F |
| **Weighted** | 100% | **94%** | **85%** | |

About 19 in 20 raw photographers' cameras decode, and about 5 in 6 use a body we have actually
checked (the per-model CC0 sweeps of October 2026); compressed ORF (OM / Olympus) is the largest gap left.

## Variant matrix (depth per format)

✅ decoded and checked on real files · 🟡 decoded, partial or unverified · ⬜ preview-only or refused.

| Format | Variant | Status | Evidence |
|---|---|---|---|
| 2026-10-12 | minor | Mamiya MEF decoded (`vendor/mef.rs`): ZD verified; 241 verified, 35 unsupported |
| 2026-10-12 | minor | Epson ERF decoded (`vendor/erf.rs`): R-D1, R-D1s, R-D1x verified; 240 verified, 229 preview (merged after the Leica Q3 and Hasselblad Lunar; duplicated summary rows removed) |
| CR2 | lossless JPEG slices | ✅ | 8 corpus bodies; CFA phase from `CR2CFAPattern` (#85, 35 bodies); masked-column black level; white level from the active area only (#1272: masked rows at 16383 hid the image plateau on the EOS 50D, 60D, 450D (Rebel XSi), 500D (Rebel T1i), 550D (Rebel T2i), 1200D (Rebel T5) and 100D (Rebel SL1), clipped highlights at 81–98%) |
| CR2 | sRAW / mRAW (YCbCr) | ✅ | `vendor/cr2/sraw.rs`, 31 samples from 16 models |
| CR3 | CRX lossless, version 0x100 | ✅ | M50, R100, R8 exact against a reference; R5 Mark II (odd-height planes) pinned |
| CR3 | C-RAW 0x100 (horizontal tiles), 0x200 (single 14-bit tile, adaptive QP incl. the QP-map escape) | ✅ | same 3 bodies exact; R5 Mark II pinned, QP maps of 33,665 files decode exactly, matches the camera JPEG |
| CR3 | other CRX variants (vertical tiles, partial subbands, other plane flags, 0x200 multi-tile) | ⬜ | refused in `crx.rs`, falls back to the preview; which bodies write them is unmeasured |
| CR3 | sRAW / mRAW | ⬜ | 4-plane Bayer only |
| CR3 | HEIF / HDR PQ | ⬜ | HEVC tracks recognised, not decoded |
| CR3 | dual-pixel raw | ⬜ | never substituted |
| CRW | all | ⬜ | preview |
| NEF | uncompressed, lossless, lossy type 1 / 2, lossy after split | ✅ | D5100, D7000, D7500, Z f corpus; ~60 NEFs by hand; #633; D4, D90, D5000, D600 and D610 image area measured against the camera JPEG (white level too for the D4, D90 and D5000) |
| NEF | packed 14-bit stored under a compressed tag (Z 6II uncompressed) | ⬜ | #769; fix in PR #671 (not merged) |
| NEF | High Efficiency / HE★ (JPEG XS) | ⬜ | named, preview (#193); needs a JPEG XS decoder |
| ARW | uncompressed, cRAW (ARW2), lossless LJ92 (L / M / S) | ✅ | 86 bodies (#535); M/S black level 1024; the Hasselblad Lunar, a NEX-7 under Make HASSELBLAD, routed by Sony's raw coding (#1275) |
| ARW | packed 12-bit (A900) | ✅ | one CC0 file; A850 unmeasured |
| ARW | ARW6 Compressed / HQ (compression 32766; ILCE-7RM6) | ✅ | 8 CC0 files; 12-bit mode unsupported; `docs/arw6-compression.md` |
| ARW | 12-bit compressed of DSLR-A200…A390 | ⬜ | preview (#535) |
| ARW | A100 ARW | ⬜ | recorded as refused (#702); not rechecked here |
| SRF | DSC-F828; DSC-V3 | ⬜ | discovered during folder import; F828 imports as preview-only, V3 has not been checked on a real file |
| ARW | ARQ pixel shift | ⬜ | no code |
| SR2 | DSC-R1 | ✅ | CC0 3221 sample; folder and direct import both decode sensor data |
| RAF | uncompressed, lossless and lossy compressed; Bayer and X-Trans; GFX 14/16-bit | ✅ | 24 files from 13 bodies exact |
| RAF | pre-X FinePix without a raw IFD | ⬜ | `raf.rs` refuses (26 Adobe models, estimated split) |
| ORF | uncompressed 16-bit, 12-bit packed, two-field 12-bit, 12.8 bpp | ✅ | E-1, E-400, XZ-2 corpus; #651 |
| ORF | **compressed** (every Olympus / OM body since ~2008) | ⬜ | `orf.rs` refuses; the E-M1 and E-M10 III corpus files are `KNOWN_UNSUPPORTED` |
| RW2 / RWL / RAW | formats 2, 4, 5, 6, 7, 8 and 16-bit words | ✅ | 118 bodies (#215) |
| PEF | uncompressed, Huffman, packed 12-bit | ✅ | K10D, K-3, K-5 IIs; files without area tags lose their masked, empty and saturated border lines (#1271: KP, KF, K-3 Mark III, K-70, K-1, K-1 Mark II, K-S1, K-S2, 645Z) |
| PEF / DNG | Pentax pixel shift | ⬜ | frames not combined |
| SRW | uncompressed, 12-bit packed | ✅ | EX1 corpus; NX10, NX20 |
| SRW | compressed 32770 / 32772 / 32773 | ⬜ | refused |
| DNG | uncompressed, LJ92, lossy JPEG / Smart Preview, Deflate incl. float, LinearRaw, tiles / strips, row / column interleave | ✅ | 6 corpus DNGs; 4 ILCE-6400 JXL DNGs; 38 Leica Q3 DNGs (lossless JPEG, 14-bit, private check) |
| DNG | JPEG XL (DNG 1.7, lossless and lossy) | ✅ | synthetic bit-exact; real lossless and lossy files; lossy tiles keep their declared transfer curve (Lightroom Classic 15.1 LinearRaw DNGs label linear data with the sRGB curve); PQ / HLG tiles preview-only |
| DNG | Apple ProRAW (incl. semantic mattes) | ✅ | iPhone 12 Pro corpus; ProfileGainTableMap read but off (ΔE00 19 against Lightroom with it on) |
| DNG | phone DNGs (Pixel, Galaxy, Xiaomi…) | 🟡 | Pixel 2 XL corpus; 13 further Apple, Google and Samsung models in the pinned sweep; white-level pile-up fix #548 |
| DNG | GoPro GPR (VC-5) | ⬜ | refused |
| 3FR / FFF | DNG-like layouts | 🟡 | six Hasselblad models verified on nine files; read by `vendor::hasselblad`: an FFF has no colour matrix of its own and now takes the camera's from the original 3FR header it embeds in tag `0xC51B` (it used to develop with the neutral matrix); RGGB checked against the camera preview on the CC0 H4D-40 FFF and CFV 3FR; the embedded preview is the uncompressed 8-bit RGB image (no JPEG in these files), handed on as a baseline TIFF |
| 3FR / FFF | Hasselblad compression | ⬜ | preview |
| ERF | 12-bit in 16-byte blocks (R-D1, R-D1s, R-D1x) | ✅ | 3 CC0 corpus files; crop registered and WB measured against the embedded preview (no WB tag found); `vendor/erf.rs` |
| MRW | 12-bit packed, MSB first, or 12-bit in big-endian 16-bit words; RGGB and GBRG | ✅ | 4 CC0 corpus files (DiMAGE 7Hi, A1, A200, Dynax 7D) plus DiMAGE 7 and A2 checked against their previews; `vendor/mrw.rs` |
| IIQ, MOS, KDC, X3F | all | ⬜ | preview; an uncompressed MOS, whose IFD0 is the mosaic itself with no Make tag, is recognised by Leaf's data block `0x8606` (#1275) |
| MEF | uncompressed 12-bit (Mamiya ZD) | ✅ | 1 CC0 corpus file; crop and WB from its uncompressed RGB preview (no JPEG, no WB tag); `vendor/mef.rs` |
| DCR, MFW, DXO, GPR, TIFF raws | all | ⬜ | not imported |

### Compressed ORF: a clean-room starting point

Two Olympus patents by Takashi Ishikawa describe the *method* of lossless RAW compression in prose; neither names ORF
and neither gives the file layout. See the closed #240 and the open #360 for why a decoder has to be derived from the
files and prose rather than from dcraw-family code.

- [US7333035B2](https://patents.google.com/patent/US7333035): lossless RAW compression for SLR cameras:
  median-edge-detector prediction from the left, upper and upper-left neighbours, a separately coded sign, Golomb-Rice
  codes whose parameter `k` comes from flatness run counters, counters reset per line.
- [US7715639B2](https://patents.google.com/patent/US7715639B2/en): a sibling with predictor variants, `k` derived from
  neighbouring positions, raw low-order bits and a correction filter.

They give the algorithm's shape; constants, the bit layout and the stream header still have to be established from CC0
sample files. A pointer for contributors, not legal advice.

## Demosaic, levels, crops and lens metadata

| Stage | Target | LightCraft | Status |
|---|---|---|---|
| Bayer demosaic | Adobe's own, plus AI "Raw Details" | AHD (default), PPG, bilinear (`crates/raw/src/demosaic`) | 🟡 quality unmeasured against Lightroom; no Raw Details |
| X-Trans demosaic | ✅ | in-house orthogonal directional green interpolation; noise-modelled Anscombe/linear 8×8 DCT refinement with 2 σ nonnegative-garrote shrinkage on **every AC coefficient** (DC preserved; unmodelled soft-threshold fallback unchanged); directional chroma guides, 7×7 ridge fit of colour ratio on green with green variance and a relative regularization floor, averaged coefficients, exact guarded-ratio inverse and complete border windows; isolated-spike suppression | 🟡 Six X-Trans generations (X-Pro1 I 16 MP, X-T1 II 16, X-T2 III 24, X-T3 IV 26, X-H2S V HS 26, X-T5 V HR 40) on Imaging Resource still lifes, ISO 200–12800: historical `xtrans_quality` flat green Laplacian −37…−51 % below branch `77285628`; strong-edge gradient 0.0/−0.3 % at ISO 6400 on the 16 MP bodies and −0.6/−1.0/−0.8/−2.0 % on III/IV/V-HS/V-HR. These are scene-dependent proxies, not recovered-detail or fidelity scores. The old chroma, “Nyquist” and two-pixel diagnostics have blind spots and cannot support their former interpretation or threshold recommendations; see the diagnostic correction below. Known-signal texture/maze tests and matching JPEG crops were checked, but do not establish sensor-family parity. Production stays at 2 σ for every body; no new tuning or Bayer validation is claimed |
| Foveon | ✅ (older bodies) | — | ⬜ |
| Monochrome / LinearRaw | ✅ | handled, no demosaic | ✅ |
| Pixel shift (Sony ARQ, Pentax, OM high-res, Nikon) | ✅ (Sony, Pentax, Nikon) | — | ⬜ |
| Black and white levels | per model, calibrated | read from each file (masked columns, maker notes, SR2SubIFD, CMP1) | 🟡 per-body bugs keep surfacing: A450–A700 SR2 black level and M/S ARW pedestal (#535, fixed); 5D Mark IV CR2 renders dark (#455, open); white-level pile-up in phone DNGs (#548, fixed); CR2 white level taken from saturated masked rows (#1272, fixed) |
| Crop masks and aspect | ✅ | DefaultCrop, Sony recorded size, Canon AspectInfo, Panasonic in-camera crops, Nikon CropArea | 🟡 7RM3 / 7RM4 crop origin 8–32 px off (cosmetic) |
| Black and white levels | per model, calibrated | read from each file (masked columns, maker notes, SR2SubIFD, CMP1) | 🟡 per-body bugs keep surfacing: A450–A700 SR2 black level and M/S ARW pedestal (#535, fixed); 5D Mark IV CR2 renders dark (#455, open); white-level pile-up in phone DNGs (#548, fixed) |
| Crop masks and aspect | ✅ | DefaultCrop, Sony recorded size, Canon AspectInfo, Panasonic in-camera crops, Nikon CropArea, measured Nikon bodies without it (D4, D90, D5000, D600, D610) | 🟡 7RM3 / 7RM4 crop origin 8–32 px off (cosmetic) |
| Lens metadata | ✅ read for profile selection | EXIF lens fields read; used only for the in-file corrections below | 🟡 |
| Highlight reconstruction | ✅ | neutral clip, chromaticity diffusion (`highlight.rs`) | 🟡 tuned by eye |
| AI denoise | ✅ Bayer and X-Trans, model bundled | Bayer only, user-installed weights, pure-Rust CPU / GPU | 🟡 no X-Trans, no shippable weights |
| Super Resolution | ✅ | — | ⬜ |
| HDR / panorama merge to raw DNG | ✅ | linear DNG (float16 HDR) with the source's colour tags | ✅ |
| Convert to DNG | ✅ | uncompressed or LJ92 tiles (`dngwrite.rs`) | ✅ (no lossy DNG) |

### X-Trans diagnostic correction (2026-10-21)

Both `xtrans_quality` and `noise_sweep` now use **metrics version 2**, implemented once in
`crates/raw/examples/support/xtrans_metrics.rs` and regression-tested in
`crates/raw/tests/xtrans_metrics.rs`. These are camera-RGB diagnostics, not fidelity scores.

- **FlatNoise:** green Laplacian standard deviation, p10 over mid-tone 256x256 windows
  (mean luma 0.08..0.6). This retains `xtrans_quality`'s definition. The old `noise_sweep`
  used different windows, sampling and a factor-of-two normalization: its flat values are
  not interchangeable with this series.
- **GreenHF / ChromaHF:** adjacent-neighbour residual, `Laplacian/4`, relative to green RMS.
  Unlike the old +/-2-pixel filter, it detects a period-two checkerboard. It is broadband,
  **not** a selective Nyquist/maze measurement. R-G and B-G squared energies are averaged,
  never added before squaring, so opposite chroma errors cannot cancel.
- **ChromaNoise:** p10 of the joint R-G/B-G Laplacian RMS over 128x128 windows, every pixel.
  Old chroma numbers used the Laplacian of their sum and sampled only one pixel parity.
  They cannot establish per-channel noise or be compared numerically to version 2.
- Edge/texture remain the sampled central-gradient percentiles: useful proxies, but blind
  to some period-two signals and not a measure of recovered detail.
- Missing qualifying windows, or zero green energy for a relative HF metric, print `NA`.
  Malformed/non-finite RGB frames fail explicitly. Both harnesses time normalization and
  demosaicing separately; the old `xtrans_quality` timer included normalization.

**Historical evidence below:** old columns named Nyquist, 2-px maze and chroma noise retain
their original numbers for traceability, not their former physical interpretation. The
old +/-2 filter nulls the very period-two pattern it claimed to measure. In particular,
the per-body "Nyquist >= -12%" threshold menu is withdrawn as a decision criterion.
Flat-noise/edge observations from `xtrans_quality` remain valid proxies, not proof of
ISO independence on every scene or fidelity across an entire sensor family.
Production stays at 2 sigma; the correction does not retune denoising.

`noise::estimate` now checks dimensions, buffer length and bounded CFA shape before indexing
or allocating, logs invalid inputs and returns no fit. The removed "below one 16-bit code"
chroma bypass was unreachable: the estimator's read-variance floor alone gives sigma >=
sqrt(1e-9), above 1/65535. Removing it does not change production pixels.

The X-T1 JSON is retained as an **opt-in research candidate**, not a built-in default.
Until its training manifest and an independent hold-out are available, X-T1 uses the
existing guarded per-file fitting/fallback path. For an explicit local experiment, copy
`assets/camera-profiles/X-T1.json` to a dedicated directory and launch with
`LIGHTCRAFT_CAMERA_PROFILES=/absolute/path/to/that/directory`. The loader reads profiles once
per process: restart after changing them. Do not describe this candidate as chart-calibrated.

**Version-2 spot check (2026-10-21):** both release harnesses produce the same six metrics
on each of these local Imaging Resource RAFs. These are current-output checks, not a new
before/after converter comparison.

| File | FlatNoise | GreenHF | Edge | Texture | ChromaNoise | ChromaHF |
|---|---:|---:|---:|---:|---:|---:|
| `XT1hSLI00200NR3D.RAF` | 1.4295e-3 | 2.7882e-2 | 1.8607e-1 | 8.0964e-3 | 8.8922e-4 | 1.2505e-2 |
| `XT1hSLI06400NR3D.RAF` | 3.9603e-3 | 2.9918e-2 | 6.7165e-2 | 3.1546e-3 | 1.1791e-3 | 1.3125e-2 |
| `XT5hSLI06400NR5D.RAF` | 1.0517e-2 | 1.9997e-2 | 1.6489e-1 | 1.3348e-2 | 4.9056e-3 | 1.2881e-2 |

An old-build dump compared with the corrected build on `XT1hSLI06400NR3D.RAF` has
**0 differing samples out of 47,941,632**, and 0 changed sRGB-proxy codes. This verifies
the noise guards/dead-branch removal do not change this frame's demosaic; it says nothing
about the intentional source-colour change from making the profile opt-in.
The rebuilt headless app imported and displayed this X-T1 with the normal fallback,
settled without a failed/pending render and left no background activity.

Validation passed formatting, workspace clippy/tests, parity, layers and assets. The CI's
WASM step initially failed because Homebrew Rust lacked the target; rerunning
`cargo xtask wasm` with the existing rustup stable toolchain passed all 29 crates.
The workspace run reported 165 skipped checks (137 corpus, 20 fonts, 8 model), not coverage.

### X-Trans follow-up plan: qualification before more tuning

**Status (2026-10-21): implementation available, qualification incomplete.** The current
X-Trans reconstruction is a production baseline, not a proven optimum or converter-parity
result. Keep the 2-sigma setting and existing reconstruction until a candidate has independent
evidence. G1/G2 remain rejected research variants; do not promote them on training residuals.
Fujifilm also makes Bayer cameras: this plan concerns X-Trans and does not qualify all RAFs.
Colour-profile qualification and demosaic qualification are separate workstreams.

These are future tasks, not implemented capabilities or newly closed gaps. Start with F1;
F2 and F3 can then proceed independently. F4 needs F3; F5 needs the corresponding validated
baseline and test data.

| Task | Work and prerequisites | Deliverable / completion evidence |
|---|---|---|
| F1: reproducible evidence | Inventory available RAWs and references; identify the old X-T1 training files if possible. Record SHA-256, source/author, licence or permission, camera/firmware, settings, scene/session and unknown metadata. Group duplicates, bursts and related scenes before assigning train/development/final-test sets. | Versioned manifests with no scene/session leakage; frozen test membership; exact code revision, commands, parameters, metric versions and reference hashes. Missing historical provenance is labelled unknown, never reconstructed by guesswork. |
| F2: traceable colour candidate | Extend the existing CLI `calibrate` workflow with explicit manifest inputs and machine-readable accepted/rejected-file reports. Record actual selected inputs, sampling, execution order/seeds where relevant, output hashes and training-set identity. Make parallel aggregation reproducible or document and test numerical tolerances. | A fresh X-T1 candidate can be regenerated from its recorded inputs. Evaluate on separate scenes against the current per-file fallback; report per-image errors, worst regressions, fit rejections and neutral/skin/saturated-colour behaviour. Keep it opt-in unless independent results justify promotion. |
| F3: independent reconstruction benchmark | Extend known-signal tests with held-out patterns/seeds, frequencies, orientations and CFA phases, low-contrast texture, diagonal edges, periodic fabrics, clipping and calibrated noise levels. Preserve an untouched final test set when choosing parameters. | Measure reconstruction error against known linear RGB, false colour, zipper/maze artifacts and detail/noise tradeoffs. Report per-case and worst-case regressions, including borders and small frames. Synthetic scenes do not by themselves establish natural-scene or sensor-family fidelity. |
| F4: real-sensor qualification | Acquire controlled flat fields at several exposure levels and ISOs, repeated frames and dark frames; document exposure, temperature when available and any in-camera processing. Add independent real scenes across the six already studied bodies and further bodies as available. | Compare estimated noise with repeated-frame measurements; document deviations from the assumed model. Freeze settings and publish version-2 per-scene diagnostics plus inspected crops. An embedded JPEG is a rendering reference, not clean demosaic ground truth. |
| F5: measured candidate and release decision | Change one reconstruction/regularization/noise parameter at a time only after a reproducible failure is identified. Use development data for tuning, not the final test set. Record quality budgets before comparing candidates. | Promote only with an improvement on independent data, no unacceptable worst-case regressions and measured preview/export cost. Otherwise retain the current baseline. Record process-version/cache implications for source-stage changes and cover affected CPU/GPU and preview/export paths. |

**Data acquisition has three different targets:**

- **Camera-JPEG look:** original RAW plus its corresponding embedded or paired JPEG, with
  film simulation, WB, dynamic-range mode, sharpening and NR settings recorded where available.
  Public samples with verified compatible licences and consented original contributions are
  useful, but unknown settings limit what can be concluded.
- **Measured colour:** a colour chart with legitimate reference values, neutral reference,
  stable documented illumination (ideally measured), unclipped manual exposures and independent
  acquisition sessions. Start at base ISO under daylight and a warm stable source; add other
  illuminants for validation. Nominal colour temperature alone does not establish spectral
  equivalence to D65 or another standard illuminant. This needs a chart-based measurement/fit
  workflow; the current JPEG-based `calibrate` command is not that workflow.
- **Noise/detail:** repeated uniform-field exposures and dark frames, plus stationary fine-detail
  scenes. Measure temporal noise separately from scene texture and fixed-pattern variation;
  do not infer a per-camera noise calibration from synthetic gain labels.

Keep private RAWs and third-party reference renders local unless their rights permit sharing;
a shareable manifest does not grant redistribution rights to its inputs. Never add these media
to the repository. Public reproducibility requires legally accessible inputs as well as hashes.
If the original 30 X-T1 training files cannot be identified, refit a new documented candidate:
the retained JSON cannot be retroactively certified by testing an overlapping or unknown set.

**Converter comparisons and performance:** regenerate reference exports with recorded settings,
versions, dimensions and ICC profiles; control exposure, tone, NR, sharpening and Texture, and
verify registration before scoring. Separate source-linear reconstruction tests from final-look
comparisons: a brighter or more saturated export does not prove a better demosaicer. Matched
controls do not imply identical internal algorithms. Measure draft/loupe and 24 MP export
separately on recorded hardware; keep normalization/demosaic timings distinct and run before/after
benchmarks for performance work. Existing 16 MP timings do not satisfy the 24 MP budget.

**Stop condition:** this phase can be called qualified for the explicitly tested cameras and
conditions once the manifests, independent quality results, real-noise checks and performance
budgets above are retained. It still does not prove optimality, support for every Fujifilm body,
or parity with Lightroom/Silkypix. Further algorithm work should be driven by measured failures,
not by the mere existence of another demosaicing method.

### X-T1 detail experiment (2026-10-20)

> Historical version-1 diagnostics; read the correction above before interpreting chroma,
> Nyquist/maze columns or timings. The current harness does not reproduce those metric definitions.

The portrait reference is `20260817_1594.RAF` (ISO 3200), **not** `20260817_1954.RAF`
(ISO 6400, a different group photo used by earlier branch measurements). Both are private,
16 MP files; neither images nor proprietary converter assets are included in the repository.

The existing sensor-derived DCT threshold was multiplied by `max(fx, fy) / 7`, leaving the lowest
AC bands at only one seventh of the intended threshold. The first correction applied a 1.5 σ soft
threshold to every AC coefficient; the later detail-preservation experiment below supersedes that
shrinkage rule. The block-energy fallback is unchanged. These are independent denoising choices,
not recovered SILKYPIX algorithms.

The chroma fit now uses `cov(G, ratio) / (var(G) + lambda)`, not the variance of the dependent
ratio; `lambda = sigma(G_mean)^2 + 0.01 * G_mean^2` retains an anti-aliasing prior. Double-precision
moments avoid cancellation in flat windows. Clipped top/bottom windows no longer contribute
zero coefficients, and inversion restores the guard: `C = ratio * (G + guard) - guard`.
Using sensor variance alone for the ridge was rejected: the existing synthetic maze test regressed.

| Metric | Before | After |
|---|---:|---:|
| ISO 3200 flat green Laplacian p10 | 0.005602 | 0.003422 |
| ISO 3200 top-1% green gradient | 0.0628 | 0.0597 |
| ISO 6400 flat green Laplacian p10 | 0.004965 | 0.003030 |
| ISO 6400 raw chroma Laplacian p10 | 0.000999 | 0.000722 |
| ISO 6400 chroma 2-px residue | 0.04390 | 0.03435 |
| ISO 3200 developed R−G / B−G Laplacian p10 | 0.01416 / 0.01689 | 0.01171 / 0.01367 |

Raw metrics use `cargo run --release -p lightcraft-raw --example xtrans_quality -- FILE.RAF`; add
`--dump FILE` / `--compare FILE` to diff two builds of the demosaic sample by sample.
Green metrics are independent of the chroma fit. Developed metrics compare full-resolution CLI
PNGs with `--set detail.nrLuminance=61 --set detail.nrDetail=87`, all other settings identical:
R−G/B−G Laplacian RMS in 128×128 windows, stride 128, starting at (64,64), then the tenth percentile.
Matching portrait crops were inspected at 200%. Demosaic on this machine was 238–250 ms after
the change, versus a 257 ms initial ISO 6400 run; these single-run times are not a speedup claim.

Less high-frequency energy is **not** proof of preserved detail. The noisy-frame median-to-90%
gradient falls about 40–46%; it mixes texture and noise. Synthetic known-signal tests additionally
require squared error below 65% of the noisy input, more than 85% sinusoidal texture contrast and
a sharp step; constant colour including borders/strip seams and a scalar ridge oracle are covered.
Fine low-contrast detail and other bodies/ISO values still need broader testing.

#### Local native-profile ablation

A separate, local-only harness linked the current LightCraft engine to an isolated evaluator
using the installed converter's X-T1 PROVIA map. No proprietary table, profile or implementation
was copied into this repository, and the application has no new native dependency or profile loader.
The evaluator read the original resource in place; the experiment does not make it redistributable.

Both branches used the same full-resolution LightCraft decode and NR 61 / Detail 87, without
sharpening. The baseline retained LightCraft's camera tone. The experimental branch converted the
denoised, linear working pixels to ISLRGB, called the native trilinear map, converted back, and
rendered without applying the camera tone again. It did **not** apply a LUT to the finished PNG.
This holds spatial processing fixed but substitutes colour and tone together; it does not separate
their individual contributions.

The native reader reported success and gamma 3.0; the colour-space matrix preserved neutral and
passed its inverse check. Repeated/permuted samples produced identical output. Input clamping
affected 0.165% of pixels; 0.672% of mapped pixels lay outside sRGB before final rendering.
In the same 500×500 portrait crop, mean absolute output RGB difference was 12.18 / 255. This is
an effect-size measurement, **not** a colour-accuracy score. The crop visibly changes saturation,
tone and apparent texture contrast, without a new spatial reconstruction stage.

The isolated map is **not a validated replacement**: the full-frame comparison has conspicuous
artifacts in saturated yellow objects, and a 257-step neutral ramp has channel reversals
(largest backward step 1095 / 65535 in red). LightCraft's starting camera calibration is retained;
SILKYPIX's actual input calibration, exposure normalization, interpolation configuration and
surrounding stages have not been reproduced. These artifacts must not be attributed to normal
SILKYPIX exports. The test establishes that the map affects the look, not that transplanting it
improves fidelity, recovers detail, or explains the reference converter's advantage. Matched
full-pipeline exports and broader camera/ISO coverage remain outstanding.

#### Capture One follow-up: preserving texture (2026-10-20)

The user's three-way JPEG screenshot shows much less noise in LightCraft's first correction,
but a softer hat weave than Capture One. The comparison also differs in tone and framing; the
original JPEGs, output sharpening and standard/AI NR settings were not available. It is a visual
lead, not a registered fidelity measurement.

Read-only inspection of installed Capture One **16.8.6.25** found separate shipped ImageCore
type/operation names for X-Trans green, `fixGreen_8x8`, `fixGreen_16x16`, chroma and capture
sharpening. Its Metal library names separate luma, high-ISO colour and spike-noise kernels.
These names alone are **stage-inventory observations**, not proof that both block sizes run
together or a trace of this export. Fresh deep MCP analysis was unavailable (provider configuration
and universal Mach-O slice support), but a subsequent check recovered the existing local Ghidra
analysis. Its arm64 input matches the installed ImageCore slice exactly: 43,853,328 bytes,
SHA-256 `b3d2f1c899b38a9c4b07aeafa1f0d64ff49aa8fece09f2e278014fb06286268a`.
No Capture One implementation, profile or resource was copied into LightCraft.

The matching static analysis establishes more than the names: `DemosaicXTrans` at `0x11e5a40`
constructs the operator sequence copy → green → fixGreen16 → fixGreen8 → fixGreen16 → fixGreen8
→ chroma. In `fixGreen8::Run` at `0x109f388`, the filtered overlap-add plane is normalized, then
passed to `ReinterpolateGreen` alongside a copy of the pre-filter green plane. The scalar path
also has neighbourhood bounds and a phase-dependent output choice before writing the green channel.
This is **static structure**, not an observed execution of the user's export or proof of why it
looks better. Using denoised green as a guide rather than simply as final green is a plausible
independent research direction; no replacement reconstruction formula is adopted here.

REA ImageCore identity evidence: `ev_708dedb47a0a67c755f7ecece5c6c3cce29fd1b0061766c6e3086d54980c1bbc`,
SHA-256 `30dc8336dddf993e730fc1a22816bc3433b24a853654b719ef729e385276de73`.
The stage names were inspected separately with local `nm`/`strings`; the identity evidence does
not itself establish their execution or implementation.

The actionable defect in **our own** DCT stage is soft-threshold bias: even a strong, coherent
coefficient loses the full threshold. Modelled modes now use nonnegative-garrote shrinkage,
`x * (1 - (t/x)^2)` for `abs(x) > t`, otherwise zero, with `t = 2 sigma`.
The higher cutoff controls noise while strong coefficients lose less contrast. DC, unmodelled
fallback, Bayer paths, NR slider meanings and sharpening settings are unchanged.

| Metric | First correction (soft 1.5 σ) | Detail follow-up (garrote 2 σ) |
|---|---:|---:|
| ISO 3200 flat green Laplacian p10 | 0.0034219 | 0.0032167 |
| ISO 3200 top-1% green gradient | 0.0597 | 0.0623 |
| ISO 3200 frame green 2-px energy | 0.0214 | 0.0224 |
| ISO 3200 raw chroma Laplacian p10 | 0.0011983 | 0.0011730 |
| ISO 3200 chroma 2-px residue | 0.02156 | 0.02291 |
| ISO 6400 flat green Laplacian p10 | 0.003030 | 0.0027915 |
| ISO 6400 chroma 2-px residue | 0.03435 | 0.03722 |

Flat noise improves a further 6% at ISO 3200 and 8% at ISO 6400; the strongest-gradient metric
recovers about 4% at ISO 3200. **Two-pixel energy increases**: real-image metrics cannot separate
recovered texture from aliases. Existing maze/range tests still pass. A new known-signal test
requires more than 94% contrast and squared error below 65% of the noisy input for nine combinations
of 4/8/16-pixel periods and horizontal/vertical/diagonal waves; it failed before the change.
This does not prove preservation of weak textures below the new cutoff.

Matching full-resolution JPEGs at NR 61 / Detail 87 and their 200% crops were inspected, as was
the full-resolution loupe in a headless app session (no pending/failed render). A separate NR 30
export illustrates the downstream detail/noise tradeoff; it is not a new default. Demosaic timings
of 234 ms before and 253 ms after on the ISO 3200 frame are individual runs, not a benchmark claim.
445 raw tests pass, including 20 X-Trans tests; 3 raw tests remain ignored.

Next targets remain residual-noise-aware spatial NR, orientation/scale-sensitive texture protection,
and sharpening that distinguishes residual noise from detail and controls halos. The existing
post-demosaic NR is a single-scale log-luminance guided filter, and its sharpening mask responds
to detail magnitude rather than a measured noise floor. These are research targets, not claims
that Capture One uses any particular replacement algorithm. Colour/tone calibration must be
compared separately, at matched exposure and scale.

#### Controlled downstream ablation (2026-10-20)

The previous real-photo comparisons held the other settings fixed, but those settings were
**not neutral**. Importing the ISO 3200 portrait `20260817_1594.RAF` gives Portrait at 100%,
Texture −5 and sharpening 32 / radius 0.9 / detail 20 / masking 60. Portrait itself contributes
another −6 Texture (`crates/pipeline/src/profiles.rs`), so the effective Texture was **−11**.
For this profile at this amount only, setting the displayed Texture to **+6** neutralizes that
spatial effect while retaining the profile's colour/tone adjustments. Setting it to zero does not.

Five full-resolution 3264×4896 renders freeze the imported process-2 colour/tone settings
(exposure −0.14, contrast 39, highlights −66, shadows 52, whites 8, blacks −4, vibrance 18,
as-shot WB). D reproduces the preceding experiment's NR 61 / Detail 87 override, not the
imported NR 24 / Detail 50. All variants retain the same current raw reconstruction; A means
**no additional** spatial NR or sharpening, not an untouched sensor plane.

| Variant | Extra luma NR | Extra colour NR | Sharpening | Effective Texture | Hat gradient RMS / A | Hair gradient RMS / A | Wall gradient RMS / A |
|---|---:|---:|---:|---:|---:|---:|---:|
| A: spatial baseline | 0 | 0 | 0 | 0 | 1.000 | 1.000 | 1.000 |
| B: NR only | 61 | 25 | 0 | 0 | 0.675 | 0.655 | 0.413 |
| C: NR + sharpening | 61 | 25 | 32 | 0 | 0.831 | 0.812 | 0.413 |
| D: preceding look | 61 | 25 | 32 | −11 | 0.746 | 0.730 | 0.396 |
| E: lower NR trial | 30 | 25 | 32 | 0 | 1.103 | 1.102 | 0.573 |

Measurements use lossless 8-bit PNGs, converted from sRGB to linear luminance, on identical
half-open pixel regions: hat `(1820,2030)-(1940,2150)`, hair `(2060,2400)-(2120,2500)`,
skin `(1940,2300)-(1980,2340)` and wall `(96,128)-(352,384)`. The regions were checked visually;
matched JPEG quality-92 exports and 200% PNG sheets were also inspected. Gradient energy includes
both signal and noise: a value above A is **not** recovered ground-truth detail.

Neutralizing Texture alone (D → C) raises hat/hair gradient RMS about **11%**, with wall
gradient RMS up **4.4%**. Wall luminance-Laplacian RMS changes from 0.0055531 to 0.0057475
(+3.5%). Lowering NR too (E) reveals more weave, but the wall gradient rises about **45%**
relative to D. C is the conservative settings trial; E is a more granular alternative, not
a noise-free algorithmic improvement or a proposed global default.

A portable partial settings file was verified to reproduce C pixel-for-pixel; a real headless
200% loupe rendered the same settings from full-resolution pixels with no pending/failed work.
The source image, sidecar and catalogue were not edited. No further production algorithm or profile
semantics were changed in this ablation: it identifies downstream NR/Texture as contributors
without attributing all residual softness to demosaic. Earlier source-stage before/after
measurements remain valid with their fixed settings, but do not establish neutral rendering or
Capture One parity. A new noise-aware spatial filter would need its own tests and process-version
boundary; green-guide reconstruction likewise needs known-signal evidence before adoption.

#### V3: direction-aware spatial NR (2026-10-20)

The downstream follow-up is now an original, versioned implementation, not another slider-only
comparison. `local::guided_nr` retains the self-guided log-luminance filter as its base. A Gaussian
average of Scharr gradient products identifies coherent directions; where the smaller eigenvalue
is less than one quarter of the larger, a tangential average contributes according to NR Detail
and that confidence. It smooths along a strand instead of across it. Isotropic patches keep the
old filter. Sampling favours nearby lattice points on the tangent with continuous, positive
weights: a nearest-point prototype blurred intermediate angles, while ordinary bilinear sampling
also attenuates fine diagonals. Protection follows `d * (2 - d)` for normalized NR Detail `d`.
Reduced previews blend in cell-centred gradients, which still detect alternating pixel-wide
stripes where the centred Scharr derivative vanishes. Fine-texture preview tests require less
than 0.01-stop RMS difference from the full render resized (measured at 1/2, 1/4 and 1/8).

This is **directional protection**, not a calibrated sensor-noise estimator, an AI denoiser, or
Capture One's recovered implementation. Crossed/isotropic texture and coherent noise are still
hard to distinguish. The change applies to spatial NR on all images using **process V3**, not
only X-Trans. V1/V2 retain their exact rendering; existing photos change only when explicitly
updated to the current process. Colour NR, sharpening, profiles, source reconstruction and the
V2 tone model are unchanged. NR Detail zero or luminance NR off bypasses the new protection.

Known-signal qualification at NR 61 / Detail 87 uses 0.3-stop sinusoids plus 0.03-stop Gaussian
noise: periods 4/8/16, horizontal, vertical, diagonal, and intermediate 15/30/60/75-degree
directions. All 21 cases retain more than 95% contrast (measured 95.7–98.8%) with squared error
below 50% of the noisy input (20.0–47.7%). The frozen filter fails the first case at 64.4%
contrast and 6.73 times the input error. White-noise residual variance remains 0.06345 of input;
Gaussian-correlated noise at sigma 1/2 gives 0.11726/0.31368 versus 0.11692/0.30111 before.
Tests cap the increase relative to the old flat-noise variance at 5%. Additional tests cover weak
and crossed textures, constant/tiny/empty planes, disabled NR, frozen legacy filtering, and cache
invalidation when switching processes, including drafts. V3 also recomputes wide draft planes
after NR changes: reusing them as V1/V2 do caused a 10-LSB stale-clarity error in an existing test.
The frozen V2 draft-reuse test remains, alongside exact V3 cached/uncached checks.

Matched full-resolution exports use the same frozen settings as C in the preceding ablation;
**only process 2 versus 3 differs**, not Texture, NR amount, sharpening or colour/tone.

| Frame / region | V3 / V2 luminance-gradient RMS | V3 / V2 luminance-Laplacian RMS |
|---|---:|---:|
| ISO 3200 hat | 1.0423 | 1.0290 |
| ISO 3200 hair | 1.1357 | 1.1079 |
| ISO 3200 skin | 1.0141 | 1.0132 |
| ISO 3200 wall | 1.0523 | 1.0310 |
| ISO 6400 hair | 1.0382 | 1.0134 |
| ISO 6400 wall | 1.0217 | 1.0112 |

ISO 3200 regions are those above. The secondary `20260817_1954.RAF` freezes its own imported
settings with NR 61 / Detail 87 and Portrait's Texture neutralized; hair is
`(1110,1900)-(1230,2140)`, wall `(96,128)-(352,384)`. These are lossless PNG measurements;
200% crops and the primary JPEG-92 pair were inspected. Energy includes noise and aliases:
the modest real-photo gain, mostly in hair, is **not a fidelity score** or a solution to the
remaining woven-texture gap.

The post-change V2 export is pixel-identical to the pre-change release. The V3 full-frame
CPU/GPU comparison differs by at most 1 LSB (mean 0.00030 LSB). All 18 GPU equivalence tests,
198 pipeline tests (2 ignored), 30 develop tests and 7 engine process tests pass; clippy and
the wasm pipeline build pass. A headless 200% full-source loupe has no pending/failed work.
The filter adds a three-channel tensor blur and tangent sampling; it is not a speed improvement.
Three alternating GPU-enabled CLI JPEG trials on this shared machine had median total times
0.665 s (V2) and 0.666 s (V3), with WB/NR submission timings 74.6/90.0 ms. Timing variation and
submission overlap preclude a throughput claim; no 24 MP performance-budget claim is made.

`detail.nrContrast` was subsequently wired into the renderer for Process V3 (commit `ff61fa8`),
allowing dynamic preservation of step edges while preserving high-ISO noise reduction.
Sharpening radius and detail controls were similarly integrated in V3 (commit `ae2f448`).
Converter parity, noise calibration, broader camera/ISO coverage and stronger woven-texture preservation remain open.

#### Review corrections and evidence limits (2026-10-20)

The initial V3 sharpening Radius implementation changed the shared Texture/local Noise/Defringe
plane, so adding a local sharpening mask could change pixels outside its mask. Separate,
radius-keyed sharpening planes now fix that on CPU/GPU. Two regression tests failed before the
fix; one checks exact outside-mask pixels at native-photo scale, the other checks that other
detail radii never depend on sharpening. GPU tests also cover cache changes across all processes.
This correction changes V3 combinations of sharpening and Texture; the old matched-photo
measurements above describe the earlier NR-only V3, not a benchmark of every subsequent edit.

The retained T1 CFA test qualifies pre-DCT/DCT baseline signals, not a guided re-interpolation
candidate. The claimed +16% reinjection noise cost and CFA-phase contrast values have no retained
reproducer and are withdrawn as evidence; guided reconstruction remains an open experiment.
The retained T2 heuristic uses `0.005 * nr.eps`, not measured sensor noise. Its independently
rerun correlated-noise residual rises from 0.22300 to 0.24840 (+11.39%), with texture/error gains.
Tests now pin both sides of that tradeoff. Rejecting that prototype does not prove the current
factor 4 optimal or close noise calibration. The earlier +22.4% and preview-failure claims are
not reproduced by this retained variant.

`noise::tests::estimate_accuracy_across_synthetic_gain_levels` now validates against known
`sqrt(a*mu+b)` noise at five levels, with independent samples rather than the fitted sigma.
Observed sigma errors are -10.88% to +4.13%, and transformed variance 0.9303 to 1.2836:
the former unit-variance-within-20% claim does not hold. The regression enforces the existing
15% sigma-accuracy budget and the corresponding variance interval with sampling tolerance.
Synthetic gain labels are not a per-camera ISO calibration; `is_usable()` only checks numerical
round-trip suitability.

The previous Capture One comparison mixed Adobe RGB (1998) values with sRGB in an untagged
collage. A corrected, source-hashed comparison converts embedded ICC profiles to sRGB before
measuring linear Rec.709 luminance. Integer translation search (+/-8 px) finds (0,0), gradient
correlation 0.84186, but does not establish subpixel/lens registration. The retained LightCraft
export changes profile, Texture, NR and sharpening together; Capture One settings remain unknown.
Mean luminance ratios LC/reference are 1.809 (hat), 2.140 (hair), 2.037 (skin), 1.223 (wall).
Consequently neither the earlier +79.4% FFT texture claim nor a noise percentage establishes
recovered detail or converter superiority. The corrected collage is a visual comparison of
different looks, not a matched-development fidelity test.

The opt-in X-T1 colour candidate (`8a74e490`, 30 files / 110976 pairs, 72x5x5 HSV table) remains
unqualified on held-out data. Its SHA-256 is
`f2abda4f6b4f315800bf3310c7cfbf6d2d22ebae28198be64d3037c571945eb9`.
The training manifest, exact fit invocation and camera-JPEG settings were not retained in this
handoff; no disjoint validation set can be certified. Recover them or refit with an explicit
split before claiming fidelity. Asset validation is not colour validation.

Nine measured X-T1 scenes do not cover other X-Trans bodies or V3 on Bayer/non-RAW images.
An export measured at 0.70 s for 3264x4896 does not verify the <=1 s / 24 MP budget (even a
linear extrapolation is 1.051 s, not a measurement). Broader coverage and repeated, correctly
sized benchmarks remain open. The local handoff in `plan/xtrans-reference-converter-analysis.md`
section 16 records the comparison script, source paths, hashes and artifacts; no user photos
or proprietary converter assets are added to the repository.

#### Guided-green reconstruction experiment G1 (2026-10-21)

The reproducible, test-only candidate in
[`xtrans_guided_tests.rs`](../crates/raw/src/demosaic/xtrans_guided_tests.rs) fits the residual
between measured CFA greens and the DCT guide against that guide, using only actual green sites
in a clipped 5x5 window. With guide/residual means G/R, guide variance V, covariance C and known
sensor variance n, its slope is `clamp(C/(V+n+1e-12), -0.5, 1)` and gate
`max(V-n,0)/(V+n+1e-12)`. It adds `strength * gate * (R + slope*(guide-G))` at every site.
Strengths 0.5 and 1 are compared with the unmodified guide. This is an original hypothesis,
not the lost earlier prototype or a recovered converter algorithm. Its gate is heuristic:
the guide and observation share noise, despite the sensor variance being known.

The campaign covers **513 cases** (96x96, 16-pixel measurement margin), three seeds/phases,
stripe/crosshatch periods 4/6/10, four angles, two amplitudes, flat fields and neutral/colour
edges, with sigma at level 0.35 of 0/0.003/0.012. Known heteroscedastic Gaussian noise
(`a=sigma²/0.36`, `b=sigma²/36`) drives the CFA samples, Anscombe/DCT and common RGB reconstruction.
The clean case uses a zero DCT threshold without chroma denoise. This is an oracle-noise
preflight, not qualification of the production estimator, Poisson sampling, or camera ISO.

Metrics measure green RMSE/centred contrast separately at green and non-green CFA sites,
and error in R-G/B-G against the true scene after the shared RGB reconstruction, not FFT energy.
The per-case gate allows at most 5% RMSE/chroma cost (+1e-6 tolerance); edge extrema may
worsen by no more than 0.001. At least half the 288 noisy texture cases must improve both
site groups' RMSE by over 5% with contrast closer to one.

| Strength | Cases outside budgets / 513 | Joint texture gains / 288 | Adopted |
|---|---:|---:|---|
| 0.5 | 20 | 74 | No |
| 1.0 | 55 | 101 | No |

For a period-10 weave at 30 degrees, amplitude 0.02, sigma 0.003, seed 11, strength 1
reduces green/non-green RMSE from 0.002552/0.002839 to 0.002055/0.002281 and reduces colour
error. But a period-4 weave at 0 degrees, amplitude 0.06, sigma 0.012, seed 91 improves
green-site RMSE while worsening non-green RMSE from 0.033204 to 0.034451 and colour error
by **8.41%**. Non-green contrast is already inverted (-0.02089), and G1 amplifies it (-0.03293).
A noisy 45-degree edge also gains up to 0.009761 in its upper extreme; that does not prove
deterministic halos on clean edges. Flat-field RMSE stays unchanged in this campaign.

Two normal regression tests preserve the successful case **and** the counterexamples, constant/
zero-strength invariance and exclusion of red/blue samples from the green fit. The full campaign
is explicit, emits all per-case measurements, checks its coverage and asserts the rejection:

```sh
cargo test -p lightcraft-raw --lib guided_green
cargo test -p lightcraft-raw --lib guided_green_candidate_campaign -- --ignored --nocapture
```

**Production remains unchanged:** only its existing RGB reconstruction stage was extracted for
reuse by the experiment. A CPU render of the primary X-T1 RAF with frozen full V3 settings
before/after that extraction has zero differing RGB components (3264x4896). RAW validation:
449 tests passed, four ignored (including the campaign, separately run), clippy all-targets,
WASM check and release CLI build passed.

G1 is not shipped; green reconstruction remains open. The next hypothesis must address
missing-site CFA alias rather than merely amplify a biased guide, then pass the synthetic
gate before any matched-settings real-RAF adoption trial. No converter-fidelity gap closes.

#### CFA-constrained reconstruction experiment G2 (2026-10-21)

G1's missing-site phase error motivated a different, test-only hypothesis: reconstruct from
the actual CFA greens rather than amplify the existing guide. G2 in the same
[`xtrans_guided_tests.rs`](../crates/raw/src/demosaic/xtrans_guided_tests.rs) runs 32 unit-step
proximal-gradient iterations per overlapping 8x8 block (stride 6), targeting
`0.5 * ||M * (IDCT(c) - observations)||² + lambda * sum(abs(c_AC))`.
M selects actual green sites. Each step replaces observed sites, transforms, soft-thresholds
AC, and inverts; DC is unpenalized. Overlaps are averaged; uncovered margins keep the guide.
Lambda is 0.75 or 1.5 stabilized sensor-noise units, not a blending strength.
The final output is regularized, not a hard reinsertion of noisy measured pixels.

The first implementation accumulated about 4e-6 drift on a noise-free flat field after
repeated `f32` transforms. Subtracting the observed-green mean before iteration and restoring
it afterwards fixes the numerical problem without changing lambda or the mathematical prior.
Constant preservation is tested at all 36 CFA anchors. A separate test verifies non-increasing
objective at every iteration on a synthetic block at each anchor; this is not a convergence
or image-quality guarantee for arbitrary scenes.

The same 513 cases, oracle noise and acceptance budgets as G1 give:

| Lambda | Cases outside budgets / 513 | Joint texture gains / 288 | Adopted |
|---|---:|---:|---|
| 0.75 | 247 | 78 | No |
| 1.5 | 256 | 41 | No |

On the period-4 weave that G1 made worse (angle 0, amplitude 0.06, sigma 0.012, seed 91),
G2 at lambda 0.75 reduces missing-site green RMSE from 0.033204 to 0.009107 and changes
contrast from -0.020886 to **+0.810760**; colour error falls from 0.014614 to 0.004990.
This is genuine signal recovery in that synthetic case, not merely more high-frequency energy.
However, a period-6 stripe at 30 degrees (amplitude 0.02, sigma 0.003, seed 11) worsens
missing-site RMSE from 0.003953 to 0.006460/0.008664 and colour error to 1.57/2.12 times
baseline. The lower lambda also more than doubles flat-field green-site RMSE in one case;
the higher lambda protects the flat field but loses more diagonal detail. Edge overshoot
also violates the budget. All these counterexamples are permanent regression tests.

```sh
cargo test -p lightcraft-raw --lib sparse_cfa_green
cargo test -p lightcraft-raw --lib sparse_cfa_green_candidate_campaign -- --ignored --nocapture
```

The 513-case quality campaign uses the standard CFA anchor; the 36-anchor mechanical tests
are not full quality coverage across all anchors. Shared-harness verification reproduces all
1539 original G1 measurements unchanged. Current RAW suite: 452 passed, five ignored, with
both research campaigns run explicitly; clippy all-targets and WASM check passed.
G2 changes tests only, not production rendering. No new real-RAF improvement or speedup is claimed.

The next proposed experiment is local model selection with genuinely withheld green observations,
uncertainty fallback and block-boundary checks. The initialization must not leak those withheld
samples; lower training residual is not validation. Test new angles/frequencies/seeds separately
before matched real-RAF trials. This selector is **not implemented**; noise calibration,
colour-profile hold-out provenance, broader camera coverage and 24 MP budgets also remain open.
The local handoff section 16.0 records the chain from RAW fixes and Texture ablation through V3,
review corrections and G1/G2, with current status and next actions in section 16.11.

#### X-Trans demosaic throughput optimization (2026-10-20)

Following the quality improvements (7-tap guide, orthogonal directional green, 2σ Garrote shrinkage,
and 7×7 ratio-guided joint chroma regression), demosaicing latency on 16 MP RAF files measured ~200–265 ms,
putting full-frame development outside comfortable interactive preview budgets. Profiling revealed four
primary bottlenecks:

1. **Zero-Clone `remove_isolated`:** The impulse noise suppression previously allocated a full-frame
   clone (`src = rgb.data.clone()`, 192 MB per frame) followed by 24 boundary and min/max lookups per pixel,
   consuming ~54 ms. This was replaced by a safe strip-boundary architecture: only 51 boundary rows (~2.9 MB)
   are buffered across Rayon strip boundaries, allowing each worker to operate on disjoint mutable row chunks
   with a 3-row sliding local buffer. Stage latency dropped from **53.8 ms to 15.2 ms** (3.5× speedup).
2. **Joint Chroma Regression (`chroma_guided_joint`):** The 7×7 ratio-guided filter was previously evaluated
   in two independent serial passes for Red (`ch = 0`) and Blue (`ch = 2`), traversing the image twice and
   computing redundant CFA lookups, division operations, and horizontal prefixes. Merging both channels into
   a single joint pass accumulates moments for Red and Blue simultaneously in a single image traversal.
   Stage latency dropped from **99.9 ms to 60.2 ms** (40% speedup).
3. **Precomputed R/B Spatial Offsets:** Hoisting channel lookups outside the innermost neighbour loop and
   precomputing neighbour Euclidean distances (`d2`) inside `RbNeighbour` eliminated per-pixel distance calculations
   and divisions in the R/B colour-difference stage, reducing stage time from **51.7 ms to 34.1 ms** (33% speedup).
4. **Fast Interior Slices for 7-Tap Guide (`guide_7tap_h` & `guide_7tap_v`):** The 7-tap green guidance filter
   evaluated `reflect(x, w)` boundary branches on every pixel. Splitting the horizontal and vertical sweeps into
   a fast unbranched interior slice (`3..w-3` and `3..h-3`) and small boundary heads/tails enabled LLVM
   SIMD auto-vectorization across the interior.

Equivalence with the pre-optimization implementation was checked on synthetic signals and on the
real frame `20260817_1594.RAF` with the repo's own harness
(`crates/raw/examples/xtrans_quality.rs`, `--dump`/`--compare`). The speed-ups reassociate
floating-point sums — the joint chroma pass accumulates Red and Blue moments in one traversal, and
each strip restarts the rolling f64 window — so the pixels are **not** bit-identical end to end:
73.49 % of the 47.9 M f32 samples differ, median |Δ| 6.0e-8, max 8.45e-5 (relative 8.5e-5), and
5,758 samples (0.012 %) change an 8-bit code in the sRGB proxy. The comparison is reproducible:
dump the frame from each build and compare (`--dump a.f32`, then the other build with
`--compare a.f32`). Every headline metric agrees to the digits shown:
- Flat green noise: `3.2167e-3`
- Nyquist energy: `0.0224`
- Top-1% edge gradient: `0.0623`
- Texture energy: `0.0032`
- Chroma noise Laplacian p10: `1.17e-3`
- Chroma 2-px maze residue: `0.02291`

Two regression tests now pin the transformations that must not change the maths:
`chroma_joint_strips_match_a_single_full_height_strip` (the first strip is bit-identical to a single
full-height call; later strips stay within 1e-6 relative — ≤ 2 ulp measured — because the rolling
window sum restarts at each boundary, which is the whole difference) and
`remove_isolated_strips_match_the_serial_pass` (bit-identical to the clone-based pass, including rows
on the 63/64 and 127/128 boundaries). `remove_isolated_clamps_spikes_and_keeps_one_pixel_lines` pins
the feature contract.

Overall full-frame demosaic execution times on 16 MP RAFs, measured once when the speed-ups landed
(before → after; the review re-ran only the final build, so the relative gains are not re-verified):
- `20260817_1594.RAF` (ISO 3200): **264 ms → 177 ms** (-33%)
- `20260817_1643.RAF` (ISO 6400): **245 ms → 213 ms** (-13%)
- `20260817_1954.RAF` (ISO 6400): **199 ms → 162 ms** (-19%)

Repeated review runs of the final build (the old `xtrans_quality` timer included normalization) gave
**165–252 ms** for `20260817_1594.RAF` and **181–212 ms** for `20260817_1954.RAF` on a loaded
machine (≈ 63–97 MP/s at 16 MP): the spread makes absolute milliseconds unsuitable as a budget —
use `cargo xtask bench`. An earlier claim of 24 MP in 174 ms (137 MP/s) is **withdrawn**: it is not
reproducible with this harness, which measures only these 16 MP frames.

#### ISO dependence and remaining headroom (2026-10-21)

> Historical version-1 campaign. Chroma percentages and the so-called Nyquist/maze budgets
> below are not per-channel noise or Nyquist measurements; their physical interpretation is
> withdrawn. Use version 2 for new A/B runs, never mix old/new metric definitions.

The gain is not an ISO 3200–6400 effect. On the Imaging Resource X-T1 still-life series — the same
scene and lighting at every ISO, so rows are comparable — `xtrans_quality` on baseline `77285628` →
current gives:

| ISO | flat green Laplacian | chroma noise (R−G, B−G) | strong-edge gradient | 2-px maze residue |
|---:|---:|---:|---:|---:|
| 200 | 2.61e-3 → 1.43e-3 (−45 %) | −36 % | +0.6 % | +0.1 % |
| 400 | 3.47e-3 → 1.83e-3 (−47 %) | −34 % | +0.7 % | −0.1 % |
| 800 | 4.61e-3 → 2.56e-3 (−45 %) | −33 % | +0.7 % | −1.0 % |
| 1600 | 6.34e-3 → 3.54e-3 (−44 %) | −31 % | +0.7 % | −2.8 % |
| 3200 | 5.45e-3 → 2.92e-3 (−46 %) | −24 % | +0.5 % | −6.3 % |
| 6400 | 6.23e-3 → 3.96e-3 (−36 %) | −22 % | +0.0 % | −9.1 % |

The demosaic removes 36–47 % of the flat green noise and 22–36 % of the chroma noise at every native
ISO, while the strong-edge gradient stays flat to slightly higher. The relative gain is close to
ISO-independent because the threshold scales with the sensor noise; the absolute residual still
grows with ISO. The earlier 43 % figure came from two field frames at ISO 3200/6400 and is
representative of that range. On 29 field frames (ISO 200–6400, different scenes) the sign is the
same but the size scatters — 0–9 % at ISO 200–400 — because the 10th-percentile statistic depends
on how much flat area and texture a frame contains, not because the algorithm fades at low ISO.
`texture energy` falls 6–35 % on the still life (small at ISO 200, large at 6400); an A/B of the
busiest 512² crop at ISO 6400 shows no perceptible loss of the pepper/salt-grinder detail, and the
quiet crop only looks slightly cleaner at 100 %, but neither observation is a fidelity measurement.

The threshold is not saturated at any ISO. Patching `THRESHOLD_SIGMAS` locally and rebuilding (same
still-life frames, 2.0 σ as control):

| threshold | ISO 200 flat / edge / texture | ISO 6400 flat / edge / texture |
|---|---|---|
| 2.0 σ (current) | 1.43e-3 / 0.186 / 8.1e-3 | 3.96e-3 / 0.067 / 3.2e-3 |
| 2.5 σ | 0.91e-3 (−37 %) / 0.186 (−0.2 %) / 7.8e-3 (−4 %) | 2.77e-3 (−30 %) / 0.066 (−1.3 %) / 2.6e-3 (−19 %) |
| 3.0 σ | 0.68e-3 (−52 %) / 0.185 (−0.4 %) / 7.6e-3 (−6 %) | 2.16e-3 (−46 %) / 0.065 (−2.7 %) / 2.3e-3 (−28 %) |

So another 30–37 % of flat-noise reduction is available at 2.5 σ for at most 1.3 % of strong-edge
energy, and the controlled scene pays only 4 % of its texture statistic at ISO 200. This metric pair
still cannot choose the operating point: there is no ground truth, so production stays at 2 σ. The
next step is a known-texture/known-noise synthetic campaign (or a matched-converter comparison), not
a constant change.

**Cross-body check: six X-Trans generations.** Imaging Resource publishes the same still life for the
other Fujifilm generations, so the same-frame A/B (baseline `77285628` → current, 2 σ) runs at ISO
200–12800 on every X-Trans family, plus X-Trans V in both its stacked 26 MP (X-H2S) and 40 MP
(X-T5) forms:

| body | sensor | MP | flat green Laplacian (range over ISO) | chroma noise | edge at 6400 | Nyquist at 6400 | edge at 12800 | Nyquist at 12800 |
|---|---|---:|---|---:|---:|---:|---:|---:|
| X-Pro1 | X-Trans I | 16 | −48…−51 % | −25…−41 % | −0.3 % | −5.8 % | — | — |
| X-T1 | X-Trans II | 16 | −36…−47 % | −22…−36 % | +0.0 % | −6.2 % | — | — |
| X-T2 | X-Trans III | 24 | −46…−47 % | −23…−37 % | −0.6 % | −12 % | −2.0 % | −21 % |
| X-T3 | X-Trans IV | 26 | −40…−43 % | −18…−29 % | −1.0 % | −15 % | −2.7 % | −25 % |
| X-H2S | X-Trans V HS | 26 | −39…−42 % | −20…−31 % | −0.8 % | −14 % | −2.3 % | −23 % |
| X-T5 | X-Trans V HR | 40 | −37…−42 % | −20…−22 % | −2.0 % | −24 % | −4.0 % | −32 % |

The flat-noise gain is ~constant with ISO on every body (the range is across ISOs), so the
noise-scaled threshold generalises across the whole family; its absolute size falls from ≈1 stop on
X-Trans I to ≈0.7–0.8 stop on X-Trans IV/V. The cost, however, grows monotonically with pixel
density: the strong-edge gradient at ISO 6400 is unchanged on the 16 MP bodies, −0.6 % at 24 MP,
−0.8/−1.0 % at 26 MP and −2.0 % at 40 MP, and the frame Nyquist energy (which counts both detail and
the noise being removed) falls −6 % → −24 % over the same range. The 2-px maze residue falls in step,
i.e. part of that energy is structured noise disappearing, not texture, but the proxies cannot say
how much. The absolute residuals are not comparable across bodies (the 40 MP pixels are the
smallest); only the paired percentages are.

The threshold curve at ISO 6400 (percentages against the baseline, not against 2 σ) shows why a
per-generation constant is tempting. Interpolating the measured k = 1.6/1.75/1.9/2.0/2.2/2.5:
if the single criterion were "keep the strong-edge gradient within −1 %", the old bodies could take
more smoothing and the 40 MP body less:

| body | k for edge ≥ −1 % at 6400 | flat gain there | Nyquist there |
|---|---:|---:|---:|
| X-Pro1 | ≈2.25 | −63 % | −7.6 % |
| X-T1 | ≈2.40 | −49 % | −8.9 % |
| X-T2 | ≈2.10 | −50 % | −13 % |
| X-T3 | 2.00 | −43 % | −15 % |
| X-H2S | ≈2.05 | −44 % | −15 % |
| X-T5 | ≈1.80 | −26 % | −19 % |

But a Nyquist-only budget of −12 % gives the opposite answer for the newest body (X-T5 ≈1.55 σ, only
≈10 % flat gain): the two proxies disagree because they weight noise and detail differently, and
neither is a fidelity measurement. Production therefore stays at **2 σ for every body**, including
the X-T5: lowering it to 1.8 σ would give back a third of the flat-noise gain to recover 1.2 % of an
edge statistic that may not be visible, and raising it on the old bodies would increase smoothing
without evidence that the extra smoothing is free. The table above is the measured menu for the
ground-truth campaign — a known-texture/known-noise synthetic set at 16, 26 and 40 MP, or a matched
converter comparison — that can finally choose per-generation values instead of inferring them from
proxies.

Remaining headroom, in expected order of payoff: the noise-model bias (−10.9 % under-estimate of
sigma, see the revision history) is equivalent to lowering the threshold by about 10 % — correcting
it moves along the same curve (≈ 2.2 σ); the chroma 2-px residue is still 0.047 (relative) at ISO
6400 with chroma noise −22 %; the noise profile is one family, not per-body calibrated (the six-body
series shows the flat-noise gain transfers but the edge/Nyquist cost grows with pixel density, and
the threshold that would hold a −1 % edge budget falls from ≈2.4 σ on X-Trans I/II to ≈1.8 σ on
X-Trans V HR); nothing above ISO 12800 was sampled (the X-T2/X-T3/X-H2S/X-T5 native maximum; all
bodies extend further with extended ISOs), and no Bayer body was checked; and there is still no
ground-truth fidelity measurement (the gap in [gaps.md](gaps.md)).

## Colour

| Item | Adobe | LightCraft |
|---|---|---|
| Default profile per model | Adobe Standard for every model (1,500 profile names in the bundle) | the DNG's own matrices; else a pooled profile (**8 bundled**: ILCA-99M2, ILCE-7CR, ILCE-7M4, NIKON D5300, NIKON Z 6_2, NIKON Z 8, X-H2S, X-T4); else a guarded per-file fit to the camera's embedded JPEG; else spectral matrices (**52 models**, rawtoaces-data, and the bodies sharing their sensors: 6 X-Trans CMOS 4 Fujifilm bodies take the X-T3's); else a neutral matrix. X-T1 is an opt-in, unqualified candidate, not a default |
| Camera Matching looks | 481 models (plus phone-maker looks) | none (the per-file JPEG fit imitates the camera's own look) |
| Creative profiles | Adobe Color, Portrait, Landscape, Vivid, Monochrome… | our own looks (Color, Neutral, Vivid, Landscape, Portrait, Monochrome) |
| DCP / dual-illuminant model | ✅ | dual-illuminant DNG colour (ColorMatrix / ForwardMatrix 1-2, CameraCalibration, AnalogBalance), HueSatMap, LookTable, ProfileToneCurve; external `.dcp` never loaded (clean-room) |
| White balance in Kelvin | ✅ for every model | absolute Kelvin where the camera has colour matrices (its own, or one of the 52 spectral rows), also for files whose look is a per-file fit (#730), or where the camera's white-balance presets in the file give the scale (6 Sony bodies and the Canon PowerShot G1 X Mark III, `kelvin_presets`); other raws keep the relative scale (#296) |

**How far the default rendering is from Adobe's** (CIE ΔE; measured where stated):

- Against the camera JPEG (what the fit targets): the Sony bulk check (#535) accepted fits on 81 of 87
  bodies, median ΔE 5.1, ΔL\* +0.2; rejected fits fall back to a neutral matrix at median ΔE 18.5,
  ΔL\* −11, chroma ×0.39. RW2 140 of 174 files accepted, median 5.0. NEF 13 of 13, 2.9–7.8. RAF 23 of
  24, pooled X-H2S 1.62 / X-T4 2.34. SRW 2.2–7.6.
- Against Lightroom itself (few): Sony DRO median ΔE76 7.5 → 3.3; Olympus E-1 vs Lightroom Classic
  ΔE00 11.25 → 8.30; ProRAW with its gain map ΔE00 19 (why the map is off). No measurement against
  Adobe Standard across models exists.
- User evidence: 33 open issues about raw colour or rendering (wrong colours, dark, flat, green,
  desaturated), the largest group of open core-path issues.

**Colour score ~35%** (estimated): most decoded files start close to the camera's own look (ΔE ~5),
about 10% start far off (ΔE ~18), nothing is calibrated, Kelvin WB is missing for fitted cameras without colour matrices, and
the distance to Adobe Standard is unmeasured. In the readiness numbers this becomes a colour factor
of ×0.85 for a working photographer (they correct colour by hand, at a cost) and ×0.75 for the
essentials user (the default look is what they keep).

## Lens profiles

| Item | Adobe | LightCraft |
|---|---|---|
| Profile database | 3,635 profile files, 56 makers, auto-selected | **0** (Adobe LCPs never used; no database of our own) |
| In-file corrections | ✅ | DNG opcodes (WarpRectilinear, FixVignetteRadial, GainMap…) applied; Panasonic / Leica RW2 distortion (79 photos, 13 bodies, 11 lenses); Sony tag 0x7037 distortion for the ILCE-7RM4A (2 lenses), full-frame Bayer 3:2 ILCE-7CR files with correction Auto or forced Auto (3 lenses; see sony-lens-corrections.md for residuals and limits) and the ILCE-7RM6 (3 lenses, 54 photos; `sony-ilce7rm6-lens-corrections.md`); Nikon NikonNEFInfo distortion (tag `0xc7d5`, about the sensor-shifted optical centre) for the Z 6II and Z f: F-mount lenses on the FTZ within 0.2 px of the camera JPEG, Z lenses within 0.5–1.5 px except the 24-70's wide end (7 px median; `docs/nikon-lens-corrections.md`) |
| Vignetting / lateral CA from maker notes | ✅ | ⬜ |
| Manual distortion, vignetting, CA, defringe | ✅ | ✅ |

**Lens score ~10%** (estimated): automatic correction reaches roughly Panasonic / Leica, phone and
drone DNG users (~9% of photographers by the weights above); Adobe reaches nearly everyone.

## Scores

| Part | Weight | Score | Basis |
|---|---:|---:|---|
| Decoding | 60% | **83%** | use-weighted 92% × 0.9 (82.8%) for per-body decode bugs still surfacing (levels, crops, WB: #535, #455, #461, #702) |
| Colour | 25% | **35%** | see [Colour](#colour) |
| Lens profiles | 15% | **10%** | see [Lens profiles](#lens-profiles) |
| **RAW readiness** | 100% | **~60%** (range 54–64%) | 49.7 + 8.75 + 1.5; demosaic and highlight quality unmeasured |

The previous estimate (earlier on 2026-10-10) was ~50%, with decoding at ~78% weighted by maker
from the format list alone. Measured model by model, decoding comes out about the same; the change is
the written decomposition, not new decoding.

## Remaining work, ranked by photographers unlocked per hour

Hours are Opus 5.5 agent hours, calibrated on this repo's raw PRs: RW2 every format 6.5 h (#215),
compressed RAF 8.6 h (#282), CR3 CRX 10 h (#279), NEF lossy after split 4.7 h (#633), ORF 12.8 bpp
1.7 h (#651), HEIC 7 h (#609). "Gain" is the change in the use-weighted share whose camera fully
works, from [raw/usage-weights.tsv](raw/usage-weights.tsv).

| # | Camera / variant | Gain (points of photographers) | Hours | Gain per hour | Needs |
|---:|---|---:|---:|---:|---|
| 1 | Compressed ORF (every Olympus / OM body since ~2008) | +2.6 | 8–15 | ~0.20 | CC0 samples (have 2) |
| 2 | CR3: remaining CRX variants (no public file uses them) | +0.3 | 10–25 | ~0.02 | files that open as a preview, from users |
| 3 | ~~Verify popular Nikon, Fujifilm, Canon CR2 and Pentax bodies (one CC0 file each, fix what breaks)~~ done (#1045, #1081) | (+2.0, taken) | — | — | public samples |
| 4 | Nikon packed 14-bit under a compressed tag (Z 6II uncompressed, #769) | +0.2 | 1–2 | ~0.13 | PR #671 exists |
| 5 | Sony DSLR-A200…A390 / A850 12-bit compressed ARW | +0.4 | 3–6 | ~0.08 | samples on raw.pixls.us |
| 6 | Pre-X FinePix RAF | +0.6 | 4–8 | ~0.08 | samples |
| 7 | Sony A100 regression (#702) and DSC-F828 sensor decoding (preview-only) | +0.1 | 1–2 | ~0.07 | samples on raw.pixls.us |
| 8 | Canon CRW (2000–2004 bodies) | +1.2 by the uniform tail weight, far less by real use | 6–12 | ≤ 0.1 | samples |
| 9 | Nikon HE / HE★ NEF (Z 8, Z 9, Z 6III, Z f, Z50II, Z5II set to HE) | +1.1 | 20–40 | ~0.04 | a pure-Rust JPEG XS decoder |
| 10 | Compressed SRW, Hasselblad 3FR, IIQ, X3F, KDC, DCR, GPR (ERF, MEF, MRW done) | +0.6 together | 45–85 | ~0.01 | samples; low use |

Beyond decoding:

| Gap | Hours | Needs |
|---|---:|---|
| Measured colour calibration for the ~60 popular bodies, Kelvin WB for cameras without matrices, render-fidelity suite against Lightroom | 80–150 | chart shots per camera (a human with cameras) or licensable measured data |
| Lens profile database of our own (fitting tools, data format, top 100 lenses), maker-note vignetting / CA | 40–80 | calibration shots (a human with lenses) |
| Demosaic and highlight quality measured against Lightroom; X-Trans AI denoise | 15–30 | |
| **Total RAW** | **255–480 h** (decoding list above 120–220 h; ~60% parallelizes) | |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-21 | patch | Share version-2 harness metrics: adjacent-neighbour HF, separate R-G/B-G energy, identical flat windows, explicit NA/errors and normalization/demosaic timing. Withdraw historical Nyquist/maze interpretation and per-channel chroma claims, not synthetic production regressions. Guard noise inputs and remove unreachable bypass without retuning pixels. Retain X-T1 JSON as opt-in pending provenance/hold-out; recount 8 automatic profiles. No fidelity gap closed. |
| 2026-10-21 | patch | Complete the cross-body check over six X-Trans generations (X-Pro1 I 16 MP, X-T1 II 16, X-T2 III 24, X-T3 IV 26, X-H2S V HS 26, X-T5 V HR 40; Imaging Resource still lifes, ISO 200–12800): flat green noise −37…−51 % at 2 σ on every body at every ISO, chroma −18…−41 %, so the noise-scaled threshold generalises across the entire family. The edge/Nyquist cost grows with pixel density (edge at ISO 6400 0.0/−0.3 % on 16 MP, −0.6/−1.0/−0.8/−2.0 % on III/IV/V-HS/V-HR; Nyquist −6…−24 %). The measured threshold curve is not saturated anywhere; a −1 % edge budget would allow ≈2.4 σ on X-Trans I/II and ask ≈1.8 σ on X-Trans V HR, while a −12 % Nyquist budget asks ≈1.55 σ on the X-T5, so the two proxies disagree and no ground truth adjudicates. Decision: production stays at 2 σ for every body; the per-generation menu is documented for the ground-truth campaign, not adopted. No Bayer body checked. |
| 2026-10-21 | patch | Cross-body check on the X-T2 (X-Trans III, 24 MP; Imaging Resource still life, ISO 200–12800): flat green noise −45.8…−47.0 % at every ISO (≈0.9 stop), chroma −22…−37 %, so the noise-scaled threshold generalises across sensor generation and pixel count. The cost appears at the top of the range: strong-edge gradient −0.6 % at ISO 6400 and −2.0 % at ISO 12800, frame Nyquist −12 %/−21 % (2-px maze residue −13 %/−21 %). The threshold sweep on the X-T2 is still not saturated (2.5 σ −36…−39 % more flat noise for −0.2…−2.6 % edge). Visual A/B at ISO 12800 shows no perceptible detail loss. Production stays at 2 σ; no gap closed. |
| 2026-10-21 | patch | Correct the ISO dependence with a controlled series: the Imaging Resource X-T1 still life (same scene and light at ISO 200/400/800/1600/3200/6400, 6 frames) shows the noise fix removes 36–47 % of flat green noise, 22–36 % of chroma noise and +0.0…+0.7 % of strong-edge gradient at **every** ISO, so the relative gain is ~ISO-independent and the field sweep's 0–9 % at ISO 200–400 was scene dependence of the 10th-percentile statistic, not an algorithm that fades. The threshold sweep repeated on the same frames still shows headroom at every ISO (2.5 σ −37 % / −30 % more flat noise at ISO 200 / 6400 for −0.2 % / −1.3 % edge; texture −4 % / −19 %). A/B of the busiest 512² crop at ISO 6400 shows no perceptible detail loss. Corpus stays gitignored (Imaging Resource copyright, not redistributed). Production pixels unchanged; no gap closed. |
| 2026-10-21 | patch | Evaluate the X-Trans noise fix across ISO: 29 X-T1 frames (ISO 200–6400) show flat green Laplacian −43/−42/−39/−45 % at ISO 3200/4000/5000/6400, −13…−29 % at ISO 800–2500, −9…0 % at ISO 200–400, strong-edge gradient within ±1.3 % (one low-contrast ISO 6400 frame −12 %). Local `THRESHOLD_SIGMAS` sweep shows the threshold is not saturated: 2.5 σ cuts residual flat noise another 42 % at ISO 6400 for −1.7 % edge, 3.0 σ −63 % for −3.2 %; no ground truth chooses the point, so production stays at 2 σ. Qualify the 43 % claim and record the remaining headroom (noise-model bias, chroma residue, per-body calibration, ISO > 6400, fidelity measurement). Production pixels unchanged. |
| 2026-10-21 | patch | Review: pin the X-Trans speed-ups with strip↔serial tests — `remove_isolated` is bit-identical to the clone-based pass across strip boundaries, the joint chroma pass is bit-identical in the first strip and ≤ 2 ulp later (the rolling f64 window sum restarts per strip), and a new test pins the impulse / one-pixel-line contract. Withdraw the "100 % bit-identical" claim: the speed-ups reassociate floating-point sums, changing 73.49 % of f32 samples (median 6.0e-8, max 8.45e-5, 0.012 % of 8-bit codes) while every headline metric agrees. Correct the ISO 3200 flat-noise figure (1.38e-3 → 3.2167e-3) and the 24 MP throughput claim (withdrawn; 16 MP re-measures 165–252 ms across runs). Tighten the synthetic noise-fit bound from 25 % to 15 % (observed sigma bias −10.9 %…+4.1 %). The A/B is reproducible with the repo harness (`--dump`/`--compare`). Production pixels unchanged. |
| 2026-10-21 | patch | G2 masked spectral reconstruction recovers a known CFA alias case but fails diagonal/noise/edge budgets; not adopted. Correct numerical DC drift, test all CFA anchors mechanically and retain 513-case campaign. Production unchanged; chronological handoff updated. |
| 2026-10-21 | patch | Retain original test-only guided-green candidate G1 and a 513-case oracle-noise campaign: partial texture gains fail CFA/colour/edge budgets, so no production adoption. Two permanent tradeoff/invariance tests, common RGB stage reused, primary RAF pixels unchanged. |
| 2026-10-20 | patch | Review: fix sharpening/Texture mask leakage on CPU/GPU; validate noise against independent known samples. Withdraw unsupported T1/T2 rejection, converter superiority, colour fidelity and 24 MP budget claims; retain open qualification tasks. No fidelity gap closed. |
| 2026-10-20 | minor | Process V3 adds original direction-aware luminance NR on CPU/GPU, with frozen legacy pixels, process-aware caches and known-signal regressions. Matched X-T1 exports show modest hair/texture gains with a small flat-noise cost; no converter-parity claim. NR Contrast found unwired; corresponding checklist row downgraded to partial. |
| 2026-10-20 | patch | Controlled five-way X-T1 ablation separates added NR, sharpening and Portrait's effective Texture −11. Neutralizing Texture recovers about 11% hat/hair gradient energy, with a small wall-noise cost; lower NR has a larger noise cost. Reused identity-matched Capture One static analysis verifies alternating block sizes and post-filter green reinterpolation, not runtime fidelity. Counts unchanged. |
| 2026-10-20 | patch | Capture One follow-up: stage inventory only, no recovered algorithm/parity claim. Original 2 σ modelled garrote DCT shrinkage reduces strong-signal bias; nine oriented/multiscale texture cases and 445 raw tests pass. Additional ISO 3200 flat-noise −6%, edge-gradient +4%, but 2-px energy rises; matched JPEG and headless 200% checks. Counts unchanged. |
| 2026-10-20 | patch | X-T1 ISO 3200 portrait identified; sensor-derived DCT threshold covers low AC bands, chroma ridge/guard inverse/border windows corrected. First-step flat noise −39%, developed chroma HF −17–19%, strong-edge gradient −5%; known-signal regression tests added. No converter parity or profile-equivalence claim; counts unchanged. |
| 2026-10-20 | patch | X-Trans demosaic: **directional orthogonal green interpolation**, **SILKYPIX 7-tap directional guide**, **RemoveIsolated impulse suppression**, and **adaptive linear refinement**. (1) Every non-green site in X-Trans has orthogonal horizontal and vertical green pairs: replaced isotropic robust mean with directional blending `(wh·gh + wv·gv)/(wh + wv)` with linear weights `1/(ε+Δ)`. (2) Red and Blue interpolation now evaluates green differences along a 7-tap directional smoothing guide (`[1, 2, 2, 2, 2, 2, 1]/12`), matching SILKYPIX clean-room analysis, avoiding noise-corrupted gradient weights. (3) Added post-demosaic `RemoveIsolated` clamping 8-neighbour isolated impulse spikes to `[min, max]`. (4) When a fitted noise profile cannot use Anscombe (e.g. read-noise dominated), DCT thresholding adapts to `σ(μ_block)` in linear domain instead of falling back to block AC. (5) Fast base-ISO bypass: guided chroma regression is skipped when sensor noise `σ(0.5) < 1/65535`. (6) Fixed border reflection phase matching so constant planes reproduce with zero error everywhere. On 16 MP ISO 6400 reference `20260817_1954.RAF`: flat noise drops from 5.41e-3 to 4.96e-3 (-8.2%), 2-px chroma maze residue falls from 0.0504 to 0.0439 (-12.9%), chroma noise drops to 9.99e-4 (-12.3%, below 1e-3); developed image high-frequency chroma noise drops 61% to 65% (R-G p10 0.0150, B-G p10 0.0184); demosaic throughput re-measured 2026-10-21 at 165–181 ms for 16 MP (the 24 MP / 137 MP/s claim is withdrawn) |
| 2026-10-20 | patch | X-Trans green refinement: the Anscombe domain is now **skipped when the frame's noise profile has no shot-noise term**. Such a profile is a legitimate fit (`a = 0`, read-noise only), but `b/a²` then drives the transform to ~1.3e9, where one `f32` ulp is worth tens of 16-bit codes, and the inverse's cancellation error becomes the plane: on a 1-px luminance checkerboard the green came back ranging 0…0.8246 for a scene topping at 0.3594 (2.3×), identical with the colour step disabled. `NoiseModel::is_usable()` requires the cancellation error `(b/a)·ε` to stay below one 16-bit code; a profile that fails it takes the block-own-threshold path instead. The 16 MP ISO 6400 X-T1 reference frame is unaffected bit for bit (5.4078e-3 flat green, 0.0534 Nyquist, 0.0479 edges, 0.0043 texture, 1.1388e-3 chroma, 0.05041 residue, 187 ms). Regression tests: a degenerate profile is rejected and its transform cannot resolve one code; the Nyquist checkerboard must stay within its scene's range |
| 2026-10-19 | patch | X-Trans chroma re-estimated in **level-ratio** space instead of colour-difference space — the first pass of the SILKYPIX/Fujifilm X-Trans demosaic (clean-room analysis in `plan/silkypix-reference-analysis.md`): the fit is now `ρ ≈ a·G + b` with `ρ = (C + 16/65535)/(G + 16/65535)` (the reference's guard of 16 on a 16-bit scale), the prediction `C = ρ̂·G`, and `λ = 0.01·(σ_ρ² + ρ̄²)` with the ratio's noise propagated from the sensor profile, `σ_ρ = σ·√(1 + ρ̄²)/(G + g₀)`. The ridge prior that pushed the slope up is dropped: in ratio space the prior slope is zero, so carrying it over injected a spurious ≈0.5 slope. A level-weighted variant of the moments was measured and **rejected**: its green-sum gate switched the fit off in the darkest windows (raw chroma Laplacian 1.83e-3, worse than the difference fit's 1.09e-3). On the 16 MP ISO 6400 X-T1 file the 2-px colour-difference residue falls 19 % (0.0622 → 0.0504) and the quiet-window chroma Laplacian rises 5 % (1.09e-3 → 1.14e-3, random rather than structured); with the default colour noise reduction the developed image's flat-window chroma high frequencies fall a further 9–26 % (R−G p10 −9 %, p50 −17 %; B−G p10 −22 %, p50 −26 %) against the difference fit, because the structured residue survives colour NR while the random part does not; demosaic 188 → 200 ms. Unit tests: the residue must fall 20 %, and a dark coloured band inside a bright field must not blow the ratio up |
| 2026-10-19 | patch | X-Trans maze pass (`RemoveMazeArtifacts`) implemented from the decompiled formula over the two colour estimates our pipeline now has (per-site and guided) and **rejected on measurement**: it re-imports the noise the guided fit removes (raw green Laplacian +49 %, raw chroma Laplacian +109 %) and makes the 2-px colour residue *worse* (0.0622 → 0.0860), because the target reconciles two equally trustworthy estimates while ours differ by noise. The guided chroma already cuts the residue 15 %; the harness grew that metric |
| 2026-10-19 | patch | X-Trans chroma: after the edge-weighted colour-difference interpolation each colour channel is re-estimated by a local ridge regression of its colour difference on green, `C−G ≈ a·G + b` over a 7×7 window with `λ = 0.01·(σ² + C̄²)` and `g = 4σ + max(C̄,0)` from the frame's noise profile, the coefficients averaged over the same window (guided-filter form) and the correction softly saturated at ±3 σ of the interpolated value; the moments come from a separable box filter maintained as running sums, so no full-frame plane is allocated. On the 16 MP ISO 6400 X-T1 file the raw chroma Laplacian falls 43 % (1.91e-3 → 1.09e-3) at unchanged green metrics, and the developed image with the default colour noise reduction loses 49 % of its flat-window chroma high frequencies; demosaic 120 → 194 ms |
| 2026-10-19 | patch | X-Trans green plane estimated in the Anscombe domain: the noise profile of the frame (`crates/raw/src/noise.rs`, now public: `NoiseModel`, `estimate`) drives the generalised transform `2·sqrt(μ/a + 3/8 + b/a²)`, the green interpolation and the 8×8 DCT refinement run there with a constant 1.5 σ threshold, and the result is mapped back with the algebraic inverse. On the 16 MP ISO 6400 X-T1 file the frame's Nyquist energy falls 29 % → 34 % against no refinement, the flat-window Laplacian stays at −69 %, and the top-1 % edge gradient at −7 % (demosaic 104 → 118 ms); the transform also yields the noise model the rest of the pipeline will need |
| 2026-10-19 | patch | X-Trans green refinement: the 8×8 DCT soft threshold is now set by the sensor's noise at each block's level instead of by the block's own mean absolute AC, which counted texture as noise and over-smoothed it. The noise profile (`crates/raw/src/demosaic/noise.rs`) is fitted per frame: same-colour neighbour differences in 16×16 blocks, the quiet tenth of the blocks per level group, least squares on `σ² = a·μ + b`. On the 16 MP ISO 6400 X-T1 file the flat-window Laplacian falls 49 % → 69 %, frame Nyquist energy 24 % → 29 %, top-1 % edge gradient 7 % → 6 %, demosaic 83 → 104 ms; threshold 1.5 σ, still capped at 8 % of the block mean, which binds in the deepest shadows |
| 2026-10-19 | patch | X-Trans demosaic: after the edge-weighted green interpolation, overlapping 8×8 blocks of the green plane are DCT-transformed and their AC coefficients shrunk by a block-adaptive threshold (DC kept, so tone does not move), then averaged back where blocks overlap; gated to X-Trans layouts, Bayer and bilinear untouched. On a 16 MP X-T1 file at ISO 6400 the flat-window Laplacian falls 49 %, the frame's Nyquist energy 24 %, and the top-1 % edge gradient 7 % (demosaic 51 → 83 ms); in the developed image at 100 % a flat wall's luma high-frequency energy falls 45 % and its chroma 28 % at the same mean, denim texture 23 % |
| 2026-10-18 | patch | X-T30, X-T30 II, X-Pro3, X100V, X-S10 and X-E4 take the X-T3's spectral matrices (same 26.1 MP X-Trans CMOS 4 sensor): an X-T30 raw with a black-and-white (Acros) camera JPEG no longer opens in the muted neutral fallback |
| 2026-10-16 | minor | Kelvin white balance from Canon CR2 `ColorData` presets (PowerShot G1 X Mark III) |
| 2026-10-15 | minor | DNGs with a look table and no tone curve (Lightroom-written, Adobe Standard) fit their tone to the embedded preview, also Lightroom's 1024 px one: two lossy ILCE-7CM2 DNGs no longer render 0.8 EV dark and flat |
| 2026-10-15 | minor | Lossy JPEG XL DNG tiles that declare a non-linear SDR transfer curve (sRGB, BT.709, DCI, gamma) decode with that curve restored instead of falling back to the embedded preview: Lightroom Classic 15.1 LinearRaw DNGs (Rec. 2100 primaries, sRGB curve; 2 private ILCE-7CM2 files) |
| 2026-10-15 | minor | Hasselblad 3FR / FFF embedded previews: the uncompressed 8-bit RGB image, handed on as a baseline TIFF (checked on the CC0 H4D-40 FFF, 1217 × 913, and CFV 3FR, 218 × 218, upright through the file's orientation); counts unchanged |
| 2026-10-15 | minor | Hasselblad 3FR / FFF get their own reader (`crates/raw/src/vendor/hasselblad.rs`): an FFF takes its colour matrix from the original 3FR header in tag `0xC51B` instead of developing with the neutral matrix; RGGB checked against the camera preview on the CC0 H4D-40 FFF and CFV 3FR; counts unchanged |
| 2026-10-12 | minor | SR2/SRF folder discovery and shared CLI/MCP extensions; public DSC-R1 sensor import and DSC-F828 preview-only import agree with direct import. F828 reclassified as preview-only (219 preview, 37 unsupported in the current generated coverage); sensor-decoding scores unchanged |
| 2026-10-14 | minor | Camera-coverage sweep (#1327): 30 models verified on 40 CC0 files (`coverage_sweep_corpus.rs`, `coverage-sweep.tsv`; `docs/raw/coverage.py` 515 → 545 verified rows); the counts above are not re-measured here. Sensor verification does not establish colour parity (#1277) |
| 2026-10-13 | minor | Bundled camera profiles 4 → 5 (ILCA-99M2) |
| 2026-10-14 | minor | 21 Sony and Leica models verified (one CC0 raw.pixls.us file each, pinned; `sony_leica_corpus.rs`): 258 verified, 920 unverified; Leica on a verified body 21% → 72%, Sony 91% → 92%, use-weighted 85% and 43% |
| 2026-10-14 | minor | 17 uncompressed Olympus and Samsung models verified on one CC0 file each (`olympus_samsung_corpus.rs`): 254 verified, 932 unverified, 224 preview; OM / Olympus 3% → 6% (5% on verified bodies); use-weighted 85% and 42% unchanged; duplicated summary rows from a merge removed |
| 2026-10-14 | minor | Nikon D600 and D610 image area (6016 × 4016 at 8, 6) measured on 4 CC0 NEFs of each, and the D90's area and white level applied to the D5000 (same sensor and filler, 2 CC0 NEFs): the filler and noise columns on the right no longer develop as a band, and for these bodies the white level is measured inside the image area, issue #1627 |
| 2026-10-11 | minor | Leica Q3 verified by a recorded check of 38 private DNGs (no decode failure; mean ΔE to the camera JPEG 6.3–13.9): 236 verified, 941 unverified, Leica bucket on a verified body 5% → 21%, weighted shares unchanged |
| 2026-10-12 | patch | CR2 white level from the active area, not the saturated masked rows above it (#1272: EOS 50D, 60D, 450D (Rebel XSi), 500D (Rebel T1i), 550D (Rebel T2i), 1200D (Rebel T5) and 100D (Rebel SL1)); scores unchanged |
| 2026-10-12 | minor | Canon PowerShot G9: as-shot white balance from WBInfo (`0x0029`, the row of the preset in ShotInfo), issue #1273; render unchanged (the camera-look fit already absorbed it), As Shot now the camera's; counts unchanged |
| 2026-10-12 | minor | Olympus E-10: as-shot white balance (`0x1017`/`0x1018`) and black level (`0x1012` in `0x102c` ValidBits units) from the main maker note, issue #1273; ΔE76 against its thumbnail 18.5 → 11.5, camera-look fit now accepted; counts unchanged |
| 2026-10-13 | minor | Nikon D90 image area (4288 × 2848 at 10, 10) and white level (3830) measured on 10 of 550 NEFs from one body: its 42 filler columns no longer develop as a pink band |
| 2026-10-12 | minor | Hasselblad Lunar verified (a Sony NEX-7 ARW under Make HASSELBLAD, now decoded): 236 verified, 232 preview; an uncompressed Leaf MOS is recognised as a raw (preview only) instead of opening as a greyscale image of its mosaic (#1275) |
| 2026-10-12 | minor | 22 more CR3 bodies verified (one CC0 raw.pixls.us file each, pinned, checked against the camera JPEG; all 110 CC0 CR3s decode); EOS R6 Mark III ColorData 66 white balance: 259 verified, 919 unverified; use-weighted 92% (59% on verified bodies); Canon 73% → 94%; decoding 83%, RAW readiness ~56% → ~60%; CR3 gain +6.2 → +0.3 |
| 2026-10-12 | minor | 117 Nikon and Fujifilm bodies verified (one CC0 raw.pixls.us file each, pinned, checked against the camera JPEG; `body_corpus.rs`); COOLPIX 5700 / 8400 / 8800 decode wrongly (now unsupported); #702 fixed, A100 preview-only: 354 verified, 824 unverified, 230 preview, 38 unsupported; use-weighted 86% (58% on verified bodies); decoding 77% unchanged |
| 2026-10-11 | minor | Nikon: every body without `CropArea` surveyed (#1053); 36 model / size / depth groups cropped to their measured camera frame (#976, #914), 6 pending; NEF row no longer claims every coding but HE decodes (compressed NEFs without a linearization table listed) |
| 2026-10-10 | minor | Compressed ORF: pointer to the two Olympus patents that describe the method, as a clean-room starting point |
| 2026-10-12 | minor | PEF without area tags: masked border rows and columns trimmed against the BlackPoint (issue #1271; KP, KF, K-3 Mark III, K-70, K-1, K-1 Mark II, K-S1, K-S2, 645Z); counts unchanged |
| 2026-10-13 | minor | Lens profiles: Nikon in-file distortion (NikonNEFInfo `0xc7d5`) for the Z 6II, Z 8 and Z f, validated on 83 frames from three lenses and the CC0 Z f sample (`docs/nikon-lens-corrections.md`); lens score unchanged |
| 2026-10-11 | minor | EOS R5 Mark II verified (CRX QP-map first literal and limited-length escape, odd-height Bayer planes; 4 CC0 files pinned, QP maps of 33,665 private files): 235 verified, 942 unverified; use-weighted 85% (42% on verified bodies); decoding 77%, RAW readiness unchanged at ~56% |
| 2026-10-10 (later) | major | Model-by-model measurement against Adobe's published list (1,446 models: 234 verified, 943 unverified, 233 preview, 36 unsupported); `docs/raw/` data and re-measure script; use-weighted share 84% (40% on verified bodies); variant matrix; colour and lens sections with scores; ranked next cameras; RAW readiness ~56% from written parts |
| 2026-10-10 | major | Created: formats, makers, demosaic, colour and lens checklists; target counts from the installed Lightroom 9.6 bundle listing |
| 2026-10-10 (later) | minor | Lens profiles: Sony ILCE-7RM6 in-file distortion next to the ILCE-7RM4A's (its lateral CA parsed, off by default); lens score unchanged (~10%: one more body) |
| 2026-10-14 | minor | 97 Canon CR2, Pentax and Ricoh models verified (one CC0 raw.pixls.us file each, pinned, checked against the camera JPEG; `canon_pentax_corpus.rs`): 334 verified, 844 unverified, 232 preview; 50% of photographers on a verified body; use-weighted 85% unchanged |
| 2026-10-14 | minor | Integration batch 36: CC0 per-model verification for 117 Nikon/Fujifilm, 97 Canon CR2/Pentax/Ricoh, 21 Sony/Leica and 23 Olympus/Samsung bodies, 22 more CR3 bodies and 30 more models; Epson ERF and Mamiya MEF decode; measured Nikon image areas (#1053, #1770): 515 verified, 675 unverified, 218 preview, 38 unsupported (re-measured with `coverage.py`) |
| 2026-10-15 | minor | By-maker and by-use tables re-measured with `coverage.py` after batches 36–39 (545 verified, 651 unverified, 212 preview, 38 unsupported; ~94% of photographers' cameras fully work, ~85% on a verified body); rows the per-model PRs had updated are current again |
| 2026-10-18 | minor | Minolta MRW decoded (`vendor/mrw.rs`): DiMAGE 7, 7Hi, A1, A2, A200 and Dynax / Maxxum 7D verified; 551 verified, 656 unverified, 201 preview (re-measured with `coverage.py`) |
