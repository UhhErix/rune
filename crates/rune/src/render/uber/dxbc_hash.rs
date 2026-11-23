// Written by `cargo run -p rune --example regen_dxbc` on Windows, the one way to regenerate the uber bytecode:
// it also writes the pinned fragments and their hashes (dxbc_pinned_hash.rs), which nothing else updates.
pub(super) const UBER_DXBC_WGSL_BLAKE3: &str = "8460010c026acacc717ee4488420d6e8999a74832874f1fb64103cf8fb7849fe";
pub(super) const UBER_DXBC_ABI_FNV64: u64 = 0x6b9932e7795ec011;
#[cfg(test)]
pub(super) const UBER_DXBC_HLSL_BLAKE3: &str = "46131763c2da476d9887471cdb8b9fb7ca2ccdd843f92a3a8f61e5d4cc92f727";
#[cfg(test)]
pub(super) const UBER_DXBC_DUAL_HLSL_BLAKE3: &str = "0b63b3ac9f5f39039650ba8b6dbc725515547963d14c0cbcfe3a0cb654150d99";
