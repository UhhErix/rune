// Written by `cargo run -p rune --example regen_dxbc` on Windows, the one way to regenerate the uber bytecode:
// it also writes the pinned fragments and their hashes (dxbc_pinned_hash.rs), which nothing else updates.
pub(super) const UBER_DXBC_WGSL_BLAKE3: &str = "895f171d5e0f0f5fa2063093907d4cb10a6c274322976f01c9095887394f8e30";
pub(super) const UBER_DXBC_ABI_FNV64: u64 = 0x6b9932e7795ec011;
#[cfg(test)]
pub(super) const UBER_DXBC_HLSL_BLAKE3: &str = "18059a355ddeb3005df624224f1dfb8e6e17bec25b8dd88c00e83eaef65503dc";
#[cfg(test)]
pub(super) const UBER_DXBC_DUAL_HLSL_BLAKE3: &str = "7223cb2e027ce05a000d2102a2c4be3fd1f60b7b243af3311bf1083bf56ea41e";
