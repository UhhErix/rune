// Written by `cargo run -p rune --example regen_dxbc` on Windows with uber.fs-k0/k1/k2.dxbc; nothing else writes them.
/// BLAKE3 of the WGSL each pinned fragment was compiled from (render/variant.rs `pinned_wgsl`, `PINNED` order).
pub(super) const UBER_DXBC_PINNED_WGSL_BLAKE3: [&str; 3] = ["445e5200faaebf03198fcd97cd43fd9620b9ce0f873815e42611a5e43f80a44d", "304eeccdf370f54f8cdd203d7d66366d295db6df8c4ec679e6b0992a7716e7fe", "60bc9e9c1cd042b13ed637b59ecbd1775f237936636555cd0a9892a9bd2cec76"];
/// BLAKE3 of each pinned fragment's file, and of the `uber.vs.dxbc` it was compiled beside.
pub(super) const UBER_DXBC_PINNED_FS_BLAKE3: [&str; 3] = ["27f1ae9dc85c8a789d573fe175966fb8533e847a80b37ad7f592adb5409abf2a", "daaf0d13c697b270b4d22942414d2624146069b95752962ff8b4b5a4a8219feb", "f41efaffba39ce5d62ca69c12fa9507bb96c2033365f24899f6d0073416e1f37"];
pub(super) const UBER_DXBC_PINNED_VS_BLAKE3: &str = "e6a25a34357594d88ffdbe2f8f6e242ee9b42aa46005cc71d067b4891bbdd42c";
