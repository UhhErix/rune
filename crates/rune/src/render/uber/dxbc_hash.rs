// Written by `cargo run -p rune --example regen_dxbc` on Windows, the one way to regenerate the uber bytecode:
// it also writes the pinned fragments and their hashes (dxbc_pinned_hash.rs), which nothing else updates.
pub(super) const UBER_DXBC_WGSL_BLAKE3: &str = "8f98a7c8a7b987ffa6829ddb0088dd6e5cd159abb50174a377b78bb5b064d65d";
pub(super) const UBER_DXBC_ABI_FNV64: u64 = 0x6b9932e7795ec011;
#[cfg(test)]
pub(super) const UBER_DXBC_HLSL_BLAKE3: &str = "28cb34a92a5ea97a20e6e879bec01be46c5cee301773d7dcd186ccdbe213500e";
#[cfg(test)]
pub(super) const UBER_DXBC_DUAL_HLSL_BLAKE3: &str = "1c4cd4009c3b6d464fe32f88ac8e516e6cf2bb649baf5626297bfe2c105c7a9e";
