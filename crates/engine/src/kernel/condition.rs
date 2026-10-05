//! Solving `Wait::Condition` exactly.
//!
//! Between events every scalar a condition can name is monotone in time:
//! resources fill toward their cap, remaining times count down, charges
//! step up when one returns, and the rest are constant. Each comparison is
//! therefore true either from some moment on or up to some moment, so the
//! earliest time the whole condition holds is now or one of the moments a
//! comparison starts to hold. Those are the only candidates checked. Any
//! event resets the prediction (see `Kernel::rearm`).

use portunus_core::{Seat, SimTime};

use crate::choice::{CmpOp, Condition, Scalar};

use super::world::{millis_ceil, World};

/// A scalar's course from now until the next event.
#[derive(Debug, Clone, Copy)]
enum Track {
    /// `v0 + slope * (t - now)`, clamped to `[lo, hi]`; slope per ms.
    Linear {
        v0: f64,
        slope: f64,
        lo: f64,
        hi: f64,
    },
    Step {
        before: f64,
        after: f64,
        at: SimTime,
    },
}

impl Track {
    fn constant(v: f64) -> Self {
        Track::Linear {
            v0: v,
            slope: 0.0,
            lo: f64::NEG_INFINITY,
            hi: f64::INFINITY,
        }
    }

    fn countdown(until: SimTime, now: SimTime) -> Self {
        Track::Linear {
            v0: f64::from(until.saturating_since(now).millis()),
            slope: -1.0,
            lo: 0.0,
            hi: f64::INFINITY,
        }
    }

    fn value(&self, now: SimTime, t: SimTime) -> f64 {
        match *self {
            Track::Linear { v0, slope, lo, hi } => {
                let dt = f64::from(t.saturating_since(now).millis());
                (v0 + slope * dt).clamp(lo, hi)
            }
            Track::Step { before, after, at } => {
                if t >= at {
                    after
                } else {
                    before
                }
            }
        }
    }

    /// Moments after `now` at which `value op rhs` may start to hold.
    fn onsets(&self, now: SimTime, op: CmpOp, rhs: f64, out: &mut Vec<SimTime>) {
        match *self {
            Track::Linear { v0, slope, lo, hi } => {
                let rising = slope > 0.0 && matches!(op, CmpOp::Ge | CmpOp::Gt) && rhs <= hi;
                let falling = slope < 0.0 && matches!(op, CmpOp::Le | CmpOp::Lt) && rhs >= lo;
                if rising || falling {
                    let dt = (rhs - v0) / slope;
                    if dt > 0.0 {
                        let t = now + millis_ceil(dt);
                        out.push(t);
                        out.push(t + portunus_core::SimDuration(1));
                    }
                }
            }
            Track::Step { at, .. } => {
                if at > now {
                    out.push(at);
                }
            }
        }
    }
}

fn holds(op: CmpOp, lhs: f64, rhs: f64) -> bool {
    match op {
        CmpOp::Lt => lhs < rhs,
        CmpOp::Le => lhs <= rhs,
        CmpOp::Ge => lhs >= rhs,
        CmpOp::Gt => lhs > rhs,
    }
}

impl World {
    fn track(&self, seat: Seat, scalar: Scalar) -> Track {
        let now = self.now;
        let me = self.seat_actor(seat);
        let Some(a) = self.actor_ref(me) else {
            return Track::constant(0.0);
        };
        let remaining = |holder, from_me: bool, aura| {
            self.actor_ref(holder)
                .map(|h| {
                    h.auras
                        .iter()
                        .filter(|i| i.aura == aura && (!from_me || i.source == me))
                        .map(|i| i.expires)
                        .fold(None, |best: Option<Option<SimTime>>, e| match (best, e) {
                            (Some(None), _) | (_, None) => Some(None),
                            (Some(Some(b)), Some(e)) => Some(Some(b.max(e))),
                            (None, Some(e)) => Some(Some(e)),
                        })
                })
                .unwrap_or_default()
        };
        let as_track = |r: Option<Option<SimTime>>| match r {
            None => Track::constant(0.0),
            Some(None) => Track::constant(f64::INFINITY),
            Some(Some(e)) => Track::countdown(e, now),
        };
        match scalar {
            Scalar::Resource(kind) => match a.resources.iter().find(|r| r.def.kind == kind) {
                Some(r) => Track::Linear {
                    v0: r.value_at(now, a.haste),
                    slope: r.rate(a.haste) / 1000.0,
                    lo: 0.0,
                    hi: r.def.max,
                },
                None => Track::constant(0.0),
            },
            Scalar::CooldownRemaining(spell) => match self.empty_until(me, spell) {
                Some(t) => Track::countdown(t, now),
                None => Track::constant(0.0),
            },
            Scalar::Charges(spell) => {
                let view = crate::state::StateView::cooldown(self, me, spell);
                match view {
                    Some(v) => match v.next_charge_at {
                        Some(at) => Track::Step {
                            before: f64::from(v.charges),
                            after: f64::from(v.charges + 1),
                            at,
                        },
                        None => Track::constant(f64::from(v.charges)),
                    },
                    None => Track::constant(0.0),
                }
            }
            Scalar::AuraRemaining(aura) => as_track(remaining(me, false, aura)),
            Scalar::AuraStacks(aura) => Track::constant(
                a.auras
                    .iter()
                    .filter(|i| i.aura == aura)
                    .map(|i| f64::from(i.stacks))
                    .fold(0.0, f64::max),
            ),
            Scalar::AuraValue(aura) => Track::constant(
                a.auras
                    .iter()
                    .filter(|i| i.aura == aura)
                    .map(|i| i.value)
                    .sum(),
            ),
            Scalar::TargetAuraRemaining(aura) => match a.target {
                Some(t) => as_track(remaining(t, true, aura)),
                None => Track::constant(0.0),
            },
            Scalar::Time => Track::Linear {
                v0: f64::from(now.millis()),
                slope: 1.0,
                lo: 0.0,
                hi: f64::INFINITY,
            },
        }
    }

    fn eval(&self, seat: Seat, c: &Condition, t: SimTime) -> bool {
        match c {
            Condition::Cmp { lhs, op, rhs } => {
                holds(*op, self.track(seat, *lhs).value(self.now, t), *rhs)
            }
            Condition::All(cs) => cs.iter().all(|c| self.eval(seat, c, t)),
            Condition::Any(cs) => cs.iter().any(|c| self.eval(seat, c, t)),
        }
    }

    fn onsets(&self, seat: Seat, c: &Condition, out: &mut Vec<SimTime>) {
        match c {
            Condition::Cmp { lhs, op, rhs } => {
                self.track(seat, *lhs).onsets(self.now, *op, *rhs, out);
            }
            Condition::All(cs) | Condition::Any(cs) => {
                for c in cs {
                    self.onsets(seat, c, out);
                }
            }
        }
    }

    pub(crate) fn condition_now(&self, seat: Seat, c: &Condition) -> bool {
        self.eval(seat, c, self.now)
    }

    /// The earliest moment the condition holds if nothing else happens.
    pub(crate) fn solve(&self, seat: Seat, c: &Condition) -> Option<SimTime> {
        let mut candidates = vec![self.now];
        self.onsets(seat, c, &mut candidates);
        candidates.sort_unstable();
        candidates.dedup();
        candidates.into_iter().find(|&t| self.eval(seat, c, t))
    }
}
