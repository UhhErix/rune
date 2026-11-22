// Written by `cargo run -p rune --example regen_dxbc` on Windows, the one way to regenerate the uber bytecode:
// it also writes the pinned fragments and their hashes (dxbc_pinned_hash.rs), which nothing else updates.
pub(super) const UBER_DXBC_WGSL_BLAKE3: &str = "7ebce7cedfc63e4730cfef4352e961c3b5bf7d203049b7c195f701b40402b48f";
pub(super) const UBER_DXBC_ABI_FNV64: u64 = 0x6b9932e7795ec011;
#[cfg(test)]
pub(super) const UBER_DXBC_HLSL_BLAKE3: &str = "4a196244e5dcb005889e6854b690b84b528e886ad68f4ca267f66625ed13032c";
#[cfg(test)]
pub(super) const UBER_DXBC_DUAL_HLSL_BLAKE3: &str = "d988427884b31c95f250be48823ed974471d4b46b502f8bed2b9999dce9c08f2";
