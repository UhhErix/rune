// Written by `cargo run -p rune --example regen_dxbc` on Windows with uber.fs-k0/k1/k2.dxbc; nothing else writes them.
/// BLAKE3 of the WGSL each pinned fragment was compiled from (render/variant.rs `pinned_wgsl`, `PINNED` order).
pub(super) const UBER_DXBC_PINNED_WGSL_BLAKE3: [&str; 3] = ["573dda8d4d842a01f014a3d88648e6595f07b649c78d4f3c2630882a5f4f463f", "a959d129f0e4e8750a16258081318e549bf35150801584019f7fd49dd02f3fe9", "6ceeccc5aea3556b723a35fbba0ee1326fb59be845c5f78f6275c2fe9a1a9514"];
/// BLAKE3 of each pinned fragment's file, and of the `uber.vs.dxbc` it was compiled beside.
pub(super) const UBER_DXBC_PINNED_FS_BLAKE3: [&str; 3] = ["27f1ae9dc85c8a789d573fe175966fb8533e847a80b37ad7f592adb5409abf2a", "daaf0d13c697b270b4d22942414d2624146069b95752962ff8b4b5a4a8219feb", "c5bfbb288a39e79a59bc037d6e3e76f8e29dfbf5513460b6356ebf01e96fc923"];
pub(super) const UBER_DXBC_PINNED_VS_BLAKE3: &str = "e6a25a34357594d88ffdbe2f8f6e242ee9b42aa46005cc71d067b4891bbdd42c";
