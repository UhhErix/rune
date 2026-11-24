//! Accessibility policy and shared-element morph tests for Liquid Glass.

use super::pyramid::pyramid_bytes;
use crate::render::KIND_LIQUID_GLASS;
use crate::{Color, Cx, Env, GlassPolicy, LiquidGlass, Runtime, el};

#[test]
fn accessibility_policies_and_1080p_pyramid_memory() {
    let mut rt = Runtime::new(
        (),
        |_: &(), _: &mut Cx<()>| {
            el().size(200.0, 100.0).child(
                el().size(100.0, 44.0)
                    .liquid_glass(LiquidGlass::regular().still(Color::BLACK)),
            )
        },
        (200.0, 100.0),
        Env::new(true, 1.0),
    );
    rt.set_glass_policy(GlassPolicy::FollowSystem);
    rt.set_system_reduced_transparency(true);
    rt.frame(0.0);
    let sh_rt = rt.liquid_glass_frame().shapes[0];
    assert!(
        sh_rt.tone[3] >= 10.0,
        "reduced transparency sigma must be >= 10 pt, got {}",
        sh_rt.tone[3]
    );
    assert_ne!(
        (sh_rt.light[1] as u32) & crate::render::liquid_glass::FLAG_REDUCED_TRANSPARENCY,
        0
    );
    assert!(!rt.glass_on());

    rt.set_system_reduced_transparency(false);
    rt.set_system_increased_contrast(true);
    rt.frame(0.016);
    let sh_ic = rt.liquid_glass_frame().shapes[0];
    assert_ne!(
        (sh_ic.light[1] as u32) & crate::render::liquid_glass::FLAG_INCREASED_CONTRAST,
        0
    );

    rt.set_system_increased_contrast(false);
    rt.set_system_reduced_transparency(true);
    rt.set_glass_policy(GlassPolicy::Glass);
    rt.frame(0.032);
    let sh_g = rt.liquid_glass_frame().shapes[0];
    assert_eq!(
        (sh_g.light[1] as u32) & crate::render::liquid_glass::FLAG_REDUCED_TRANSPARENCY,
        0
    );
    assert!(rt.glass_on());

    rt.set_glass_policy(GlassPolicy::Solid);
    rt.frame(0.048);
    assert!(rt.liquid_glass_frame().shapes.is_empty());
    assert_eq!(
        rt.instances
            .iter()
            .filter(|i| i.params[1] == KIND_LIQUID_GLASS)
            .count(),
        0
    );

    let bytes = pyramid_bytes([960, 576]);
    assert!(
        bytes > 5_000_000 && bytes < 6_500_000,
        "1080p pyramid bytes={bytes}"
    );
}

#[test]
fn glass_id_morph_flies_inside_container() {
    let mut rt = Runtime::new(
        false,
        |&moved: &bool, _: &mut Cx<bool>| {
            let mut cont = el().size(300.0, 100.0).glass_container(12.0);
            if !moved {
                cont = cont.child(
                    el().key("a")
                        .absolute(20.0, 20.0)
                        .size(60.0, 44.0)
                        .glass_id("chip")
                        .liquid_glass(LiquidGlass::regular()),
                );
            } else {
                cont = cont
                    .child(
                        el().key("anchor")
                            .absolute(100.0, 20.0)
                            .size(60.0, 44.0)
                            .liquid_glass(LiquidGlass::regular()),
                    )
                    .child(
                        el().key("b")
                            .absolute(200.0, 20.0)
                            .size(60.0, 44.0)
                            .glass_id("chip")
                            .liquid_glass(LiquidGlass::regular()),
                    );
            }
            cont
        },
        (300.0, 100.0),
        Env::new(true, 1.0),
    );
    rt.set_glass_policy(GlassPolicy::Glass);
    rt.frame(0.0);
    rt.update(|m, tx| {
        *m = true;
        tx.animated();
    });
    rt.frame(0.05);
    assert_eq!(
        rt.instances
            .iter()
            .filter(|i| i.params[1] == KIND_LIQUID_GLASS)
            .count(),
        1
    );
    assert_eq!(rt.liquid_glass_frame().shapes.len(), 2);
}
