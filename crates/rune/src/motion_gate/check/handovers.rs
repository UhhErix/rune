//! Pairing recorded `El::matched` handovers between a leaving source and an arriving target.

use std::collections::HashSet;

use super::{AXES, Finding, Recording, Snap, rule, seen, short, shown_edge};

/// Maximum opacity difference between the source's last frame and the target's first frame.
const OPACITY_TOL: f32 = 0.05;

/// One recorded `El::matched` handover on frame `frame`: `from` left the tree as `to` took over.
#[derive(Clone, Debug, PartialEq)]
pub struct Handover {
    pub frame: usize,
    pub from: String,
    pub to: String,
    pub from_rect: [f32; 4],
    pub to_rect: [f32; 4],
}

/// Deaths and births cleanly paired by a recorded handover on `(frame, track_index)`.
#[derive(Default)]
pub(super) struct Paired {
    pub deaths: HashSet<(usize, usize)>,
    pub births: HashSet<(usize, usize)>,
}

impl Recording {
    /// Records one `El::matched` handover on `frame`.
    pub fn handover(
        &mut self,
        frame: usize,
        from: impl Into<String>,
        to: impl Into<String>,
        from_rect: [f32; 4],
        to_rect: [f32; 4],
    ) {
        self.handovers.push(Handover { frame, from: from.into(), to: to.into(), from_rect, to_rect });
    }
}

pub(super) fn check(rec: &Recording, out: &mut Vec<Finding>) -> Paired {
    let mut paired = Paired::default();
    let rect_tol = rule("bad-handover");
    for h in &rec.handovers {
        let k = h.frame;
        if k <= rec.start || k == 0 {
            continue;
        }
        let from_i = rec.index.get(&h.from).copied();
        let to_i = rec.index.get(&h.to).copied();
        let from_name = short(&h.from);
        let to_name = short(&h.to);

        let Some(fi) = from_i else {
            out.push(Finding {
                kind: "bad-handover",
                element: from_name,
                frame: k,
                detail: format!("matched handover to {to_name}: source was not mounted on frame {}", k - 1),
            });
            continue;
        };
        let Some(src) = rec.tracks[fi].at(k - 1).copied() else {
            out.push(Finding {
                kind: "bad-handover",
                element: from_name,
                frame: k,
                detail: format!("matched handover to {to_name}: source was not mounted on frame {}", k - 1),
            });
            continue;
        };
        if rec.tracks[fi].at(k).is_some() {
            out.push(Finding {
                kind: "bad-handover",
                element: from_name,
                frame: k,
                detail: format!("matched handover to {to_name}: source did not unmount on frame {k}"),
            });
            continue;
        }

        let Some(ti) = to_i else {
            out.push(Finding {
                kind: "bad-handover",
                element: from_name,
                frame: k,
                detail: format!("matched handover to {to_name}: target never mounted"),
            });
            continue;
        };
        if rec.tracks[ti].at(k - 1).is_some() {
            out.push(Finding {
                kind: "bad-handover",
                element: from_name,
                frame: k,
                detail: format!("matched handover to {to_name}: target was already mounted before frame {k}"),
            });
            continue;
        }
        let Some(dst) = rec.tracks[ti].at(k).copied() else {
            let late = rec.tracks[ti].frames.iter().find(|f| f.0 > k).map(|f| f.0);
            let detail = match late {
                Some(born) => format!("matched handover to {to_name}: target arrived {} frame(s) late (frame {born} instead of {k})", born - k),
                None => format!("matched handover to {to_name}: target did not mount on frame {k}"),
            };
            out.push(Finding { kind: "bad-handover", element: from_name, frame: k, detail });
            continue;
        };

        let vel = k.checked_sub(2)
            .and_then(|k2| rec.tracks[fi].at(k2))
            .map(|prev| std::array::from_fn::<f32, 4, _>(|a| src.rect[a] - prev.rect[a]))
            .filter(|v| v.iter().any(|d| d.abs() > 0.5))
            .unwrap_or([0.0; 4]);
        let expected: [f32; 4] = std::array::from_fn(|a| src.rect[a] + vel[a]);
        let next = rec.tracks[ti].at(k + 1).copied();
        let mut worst = 0.0f32;
        for a in 0..4 {
            let err = dst.rect[a] - expected[a];
            if err.abs() <= rect_tol {
                continue;
            }
            // A spring steps dt on frame k before painting; from rest, frame k's step is under
            // half of frame k + 1's step in the same direction (about a third), however far it
            // flies. How hard the step across changes is `jerk`'s to judge, below.
            let smooth_start = next.is_some_and(|n| {
                let next_step = (n.rect[a] - dst.rect[a]) - vel[a];
                err * next_step > 0.0 && err.abs() <= 0.5 * next_step.abs() + 1.0
            });
            if !smooth_start {
                worst = worst.max(err.abs());
            }
        }
        if worst > 0.0 {
            out.push(Finding {
                kind: "bad-handover",
                element: from_name,
                frame: k,
                detail: format!(
                    "matched handover to {to_name}: target jumped {worst:.1} pt from ({:.0},{:.0} {:.0}×{:.0}) to ({:.0},{:.0} {:.0}×{:.0})",
                    expected[0], expected[1], expected[2], expected[3],
                    dst.rect[0], dst.rect[1], dst.rect[2], dst.rect[3],
                ),
            });
            continue;
        }

        if (dst.op - src.op).abs() > OPACITY_TOL {
            out.push(Finding {
                kind: "bad-handover",
                element: from_name,
                frame: k,
                detail: format!("matched handover to {to_name}: opacity jumped {:.2} → {:.2}", src.op, dst.op),
            });
            continue;
        }

        paired.deaths.insert((k, fi));
        paired.births.insert((k, ti));

        let prev = k.checked_sub(2).and_then(|k2| rec.tracks[fi].at(k2)).copied();
        if let Some(jerk) = jerk_across(rec, prev, src, dst) {
            out.push(Finding { kind: "jerk", element: to_name, frame: k, detail: format!("{jerk} across the matched handover from {from_name}") });
        }
    }
    paired
}

/// The `jerk` of the step that crosses the handover: the source's last step against the
/// target's first, which neither track sees alone. From rest it is no jerk, as on one track, and
/// as there it is judged on what shows: an edge its clip hides can change step unseen.
fn jerk_across(rec: &Recording, prev: Option<Snap>, src: Snap, dst: Snap) -> Option<String> {
    let shows = |s: &Snap| seen(s, rec.size) && s.op >= rule("faint");
    let prev = prev.filter(shows)?;
    if !shows(&src) || !shows(&dst) {
        return None;
    }
    let jerk = rule("jerk");
    (0..4)
        .map(|axis| {
            let shown = |s: &Snap| shown_edge(s, axis, rec.size);
            let seen_change = (shown(&dst) - shown(&src)) - (shown(&src) - shown(&prev));
            (axis, src.rect[axis] - prev.rect[axis], dst.rect[axis] - src.rect[axis], seen_change)
        })
        .filter(|&(_, before, step, seen_change)| before.abs() >= 0.5 && step.abs() >= 0.5 && (step - before).abs() > jerk && seen_change.abs() > jerk)
        .max_by(|a, b| (a.2 - a.1).abs().total_cmp(&(b.2 - b.1).abs()))
        .map(|(axis, before, step, _)| {
            format!("{} step {before:+.1} → {step:+.1} pt/frame ({:.0} → {:.0} pt/s)", AXES[axis], before as f64 * rec.hz, step as f64 * rec.hz)
        })
}
