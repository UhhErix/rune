// Written by `cargo run -p rune --example regen_dxbc` on Windows with uber.fs-k0/k1/k2.dxbc; nothing else writes them.
/// BLAKE3 of the WGSL each pinned fragment was compiled from (render/variant.rs `pinned_wgsl`, `PINNED` order).
pub(super) const UBER_DXBC_PINNED_WGSL_BLAKE3: [&str; 3] = ["167482bc01a9085f143464c1bdd5784d615073bbad3614e51196b111eadeb036", "98cfcbdd65f507dc6c784c94d0aa223ce71385e5485526acdc46ebf0d02e2a0a", "63829f8afbbc3cff772abee79982b55a37855247a4fd41676c6809a3e2d217bf"];
/// BLAKE3 of each pinned fragment's file, and of the `uber.vs.dxbc` it was compiled beside.
pub(super) const UBER_DXBC_PINNED_FS_BLAKE3: [&str; 3] = ["eec4c3120cce80805d742a40942b685dada3fd3454b3a56f44c379059f9e8d77", "d65504b8e6fad69bfc30cd9b64a7d7c8025e747f74c4603257d971fa9b59a677", "2cfe82874793af07f7508128733b328603478bdf2a4e66c8f0451a9bdbfdea80"];
pub(super) const UBER_DXBC_PINNED_VS_BLAKE3: &str = "e6a25a34357594d88ffdbe2f8f6e242ee9b42aa46005cc71d067b4891bbdd42c";
