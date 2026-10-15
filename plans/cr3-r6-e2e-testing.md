# Canon R6 CR3 End-to-End Testing Plan

## Objective
Add R6 to the CR3 corpus, verify decode parity with Ansel and Darktable via black-box reference extraction, and document results.

## Context
- lcc's CR3 decoder matches darktable/ansel on: lossless RAW, C-RAW (lossless compression), adaptive QP maps, horizontal/vertical C-RAW tile layouts
- Missing features: sRAW/mRAW subsampling, Dual Pixel RAW, HEIF/HDR PQ, 12/14-bit C-RAW, vertical grid C-RAW
- Only 4 camera bodies verified: M50, R100, R8, R5 Mark II. R6 would be the 5th.
- Existing test methodology: FNV-1a hash of decoded u16 samples compared against black-box reference (DNGLab 0.8.0)

## Reference Tools
- **Ansel** (v8.x): `ansel-rawdump -o linear --format=bmp {file}.cr3` → u16 BMP → convert to PGM → FNV-1a hash
- **Darktable** (v5.x): `darktable-rawdump -o linear --format=bmp {file}.cr3` → same pipeline
- **DCamProf** (v1.0.6): Available but for camera profiling, not decode verification. Not needed for this plan.

## Implementation Steps

### Step 1: Install/Verify Reference Tools
```bash
# Verify Ansel is installed (use v8.x for consistency with existing hashes)
ansel-rawdump --version
# Expected: ansel-rawdump 8.2.2 or similar

# Verify Darktable is installed
darktable-rawdump --version
# Expected: darktable-rawdump 5.x or similar

# If not installed:
# apt install ansel-tools darktable-tools
# or: nix profile install ansel darktable
```

### Step 2: Process Provided R6 CR3 Files

For each CR3 file provided:

```bash
# 1. Extract reference with Ansel
ansel-rawdump -o linear --format=bmp {file}.cr3
# Produces: {file}_ansel.bmp (u16 BMP)

# 2. Extract reference with Darktable
darktable-rawdump -o linear --format=bmp {file}.cr3
# Produces: {file}_darktable.bmp (u16 BMP)

# 3. Convert BMP to PGM for hashing (u16 PGM)
convert {file}_ansel.bmp pgm:{file}_ansel.pgm
convert {file}_darktable.bmp pgm:{file}_darktable.pgm

# 4. Compute FNV-1a hash of u16 samples
fnv1a_u16 {file}_ansel.pgm > {file}_ansel.hash
fnv1a_u16 {file}_darktable.pgm > {file}_darktable.hash
```

### Step 3: Compute lcc Hashes

```bash
# Build lcc CLI
cargo build --release -p lcc-cli

# Decode each CR3 with lcc and compute hash
./target/release/lcc-cli decode {file}.cr3 --format=pgm --output={file}_lcc.pgm
fnv1a_u16 {file}_lcc.pgm > {file}_lcc.hash
```

### Step 4: Compare and Document

For each file, create a comparison table:

| File | Ansel Hash | Darktable Hash | lcc Hash | Parity? |
|------|------------|----------------|----------|---------|
| R6_001.CR3 | <hash> | <hash> | <hash> | ✅/❌ |

Document any discrepancies:
- If lcc ≠ Ansel or lcc ≠ Darktable: investigate decode pipeline
- If Ansel ≠ Darktable: document the discrepancy (may be tool-specific differences)

### Step 5: Add to Test Suite

If hashes match, add R6 files to `crates/raw/tests/cr3_corpus.rs`:

```rust
// New test case
fn test_cr3_r6_001() {
    let data = include_bytes!("../../tests/fixtures/cr3/R6_001.CR3");
    let decoded = decode_cr3(data).expect("decode failed");
    assert_eq!(decoded.width, 5472);
    assert_eq!(decoded.height, 3648);
    assert_eq!(decoded.bpp, 14);
    // FNV-1a hash comparison
    let hash = fnv1a_u16(&decoded.samples);
    assert_eq!(hash, EXPECTED_R6_001_HASH);
}
```

### Step 6: Update Documentation

Update `docs/raw-parity.md`:
- Mark R6 as verified camera body
- Record hash values and tool versions
- Note any known discrepancies

## Expected Outcome
- R6 added to verified camera bodies (5 total)
- Decode parity confirmed via FNV-1a hash comparison
- E2E regression test added to prevent future regressions
- Documentation updated with R6 verification results

## Rollback Plan
If hashes don't match, investigate lcc decoder before committing test. If discrepancy is tool-specific (Ansel vs Darktable differ), document and use Ansel as primary reference (consistent with existing tests).