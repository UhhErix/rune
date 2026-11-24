// Written by `cargo run -p rune --example regen_dxbc` on Windows with uber.fs-k0/k1/k2.dxbc; nothing else writes them.
/// BLAKE3 of the WGSL each pinned fragment was compiled from (render/variant.rs `pinned_wgsl`, `PINNED` order).
pub(super) const UBER_DXBC_PINNED_WGSL_BLAKE3: [&str; 3] = ["d4775e8b9183c9092f373d79006e39703f40852dd2d4a53d4121c00932342601", "411e1c00b3831b83c936f8328570f21db79d5408fe35accd817b8f2c73becc2b", "dbab143ced0833ebbde9aa4bc9cda7d0f6a50faf556091541c1262f2faebc22c"];
/// BLAKE3 of each pinned fragment's file, and of the `uber.vs.dxbc` it was compiled beside.
pub(super) const UBER_DXBC_PINNED_FS_BLAKE3: [&str; 3] = ["c36098265655eb235780db672c0dd61ddbe0cb24e8e980157a11c2d0253d8b32", "373bbf31ceaaab776daec4f86934850633e3c84dc76f650cb9afba72756cfcd6", "a048c4a706cd18a30456459b51755fc484e52119209f6e05fd740abb353ecdc9"];
pub(super) const UBER_DXBC_PINNED_VS_BLAKE3: &str = "e6a25a34357594d88ffdbe2f8f6e242ee9b42aa46005cc71d067b4891bbdd42c";
