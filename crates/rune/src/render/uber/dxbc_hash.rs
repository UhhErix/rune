// Written by `cargo run -p rune --example regen_dxbc` on Windows, the one way to regenerate the uber bytecode:
// it also writes the pinned fragments and their hashes (dxbc_pinned_hash.rs), which nothing else updates.
pub(super) const UBER_DXBC_WGSL_BLAKE3: &str = "1771a8e7825d1e023dd9b05b556f7300c1401e0d47d1f6021f72c74575ad32a1";
pub(super) const UBER_DXBC_ABI_FNV64: u64 = 0x6b9932e7795ec011;
#[cfg(test)]
pub(super) const UBER_DXBC_HLSL_BLAKE3: &str = "3ffb36beb613e553a4dbb14a32e2e7f166af4a693a676bc8ed0a6a45ea7e4228";
#[cfg(test)]
pub(super) const UBER_DXBC_DUAL_HLSL_BLAKE3: &str = "f7544b3b214ac19017aa721addf4a813e710b1c96e53dd7eab031650ca5d032a";
