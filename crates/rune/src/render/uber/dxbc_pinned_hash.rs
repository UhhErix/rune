// Written by `cargo run -p rune --example regen_dxbc` on Windows with uber.fs-k0/k1/k2.dxbc; nothing else writes them.
/// BLAKE3 of the WGSL each pinned fragment was compiled from (render/variant.rs `pinned_wgsl`, `PINNED` order).
pub(super) const UBER_DXBC_PINNED_WGSL_BLAKE3: [&str; 3] = ["3a4ac1de89899dcab3d8721856110390fcf506c455fb5b4768708037d2a05209", "1aebd2f34796f31a7fa214c686bc9dde2b2e202b4dc4f52f0c1b97eee587adc2", "581059959f09d6ad0ee13f96828ccf780354834d57ba3599b02e80fac84b34ba"];
/// BLAKE3 of each pinned fragment's file, and of the `uber.vs.dxbc` it was compiled beside.
pub(super) const UBER_DXBC_PINNED_FS_BLAKE3: [&str; 3] = ["27f1ae9dc85c8a789d573fe175966fb8533e847a80b37ad7f592adb5409abf2a", "daaf0d13c697b270b4d22942414d2624146069b95752962ff8b4b5a4a8219feb", "f41efaffba39ce5d62ca69c12fa9507bb96c2033365f24899f6d0073416e1f37"];
pub(super) const UBER_DXBC_PINNED_VS_BLAKE3: &str = "e6a25a34357594d88ffdbe2f8f6e242ee9b42aa46005cc71d067b4891bbdd42c";
