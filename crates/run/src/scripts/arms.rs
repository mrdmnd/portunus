//! Arms Warrior.
//!
//! [`arms`] is the port of SimC's Slayer lists. The other policies here
//! are candidates, each the best so far plus one change, and
//! `docs/ARMS_TUNING.md` records how each did.

use portunus_core::{AuraId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, MoveGoal, Wait};
use portunus_env::Decision;
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::stats::ResourceKind;

use super::GCD;
use crate::observe::{SeatObs, FOREVER};

const MORTAL_STRIKE: SpellId = SpellId(12294);
const OVERPOWER: SpellId = SpellId(7384);
const EXECUTE: SpellId = SpellId(163201);
const EXECUTE_MASSACRE: SpellId = SpellId(281000);
const SLAM: SpellId = SpellId(1464);
const COLOSSUS_SMASH: SpellId = SpellId(167105);
const AVATAR: SpellId = SpellId(107574);
const BLADESTORM: SpellId = SpellId(227847);
const REND: SpellId = SpellId(772);
const CLEAVE: SpellId = SpellId(845);
const WRECKING_THROW: SpellId = SpellId(384110);
const CHARGE: SpellId = SpellId(100);
const SWEEPING_STRIKES: SpellId = SpellId(260708);
const DEMOLISH: SpellId = SpellId(436358);

const SUDDEN_DEATH: AuraId = AuraId(52437);
const COLOSSUS_SMASH_DEBUFF: AuraId = AuraId(208086);
const REND_DOT: AuraId = AuraId(388539);
const AVATAR_BUFF: AuraId = AuraId(107574);
const MASTER_OF_WARFARE_PROC: AuraId = AuraId(1269391);
const EXECUTIONERS_PRECISION: AuraId = AuraId(386633);
const BLOODLETTING: AuraId = AuraId(383154);
const EXECUTIONER: AuraId = AuraId(445584);
const IMMINENT_DEMISE: AuraId = AuraId(445606);
const SWEEPING_STRIKES_BUFF: AuraId = AuraId(260708);
const COLLATERAL_DAMAGE_BUFF: AuraId = AuraId(334783);
const COLOSSAL_MIGHT: AuraId = AuraId(440989);
const BROAD_STROKES: AuraId = AuraId(1261049);

/// The changes a candidate makes to the SimC port, one per field, all off
/// for [`arms`].
#[derive(Debug, Clone, Copy)]
struct Tweaks {
    /// In the execute phase, Overpower with both charges ahead of Execute.
    execute_overpower_capped: bool,
    /// Avatar, Bladestorm, and Colossus Smash as soon as ready, ignoring
    /// SimC's alignment conditions.
    avatar_on_cd: bool,
    bladestorm_on_cd: bool,
    smash_on_cd: bool,
    /// Bladestorm only with at least this many Imminent Demise stacks.
    bladestorm_demise: Option<u8>,
    /// Spend Sudden Death after Mortal Strike with no Executioner up too.
    free_execute_ungated: bool,
    /// Outside the execute phase, no Execute ahead of Colossus Smash to
    /// make way for Bladestorm's Imminent Demise.
    no_demise_execute: bool,
    /// Cleave for Rend only when it's about to fall off.
    rend_last_gcd: bool,
    /// Overpower with both charges ahead of Mortal Strike.
    overpower_capped_first: bool,
    /// Outside the execute phase, spend Sudden Death ahead of Mortal
    /// Strike.
    free_execute_early: bool,
    /// In the execute phase, spend Sudden Death right after Colossus
    /// Smash.
    execute_free_first: bool,
    /// In the execute phase, Mortal Strike with at least this many
    /// Executioner's Precision stacks instead of SimC's 2.
    execute_ms_precision: Option<u8>,
    /// In the execute phase, Execute ahead of Overpower only above this
    /// much rage instead of SimC's 40.
    execute_rage: Option<f64>,
}

const PORT: Tweaks = Tweaks {
    execute_overpower_capped: false,
    avatar_on_cd: false,
    bladestorm_on_cd: false,
    smash_on_cd: false,
    bladestorm_demise: None,
    free_execute_ungated: false,
    no_demise_execute: false,
    rend_last_gcd: false,
    overpower_capped_first: false,
    free_execute_early: false,
    execute_free_first: false,
    execute_ms_precision: None,
    execute_rage: None,
};

/// What the lists read, worked out once per decision.
struct Arms<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    /// Massacre's Execute if the build has it.
    execute: SpellId,
    enemies: usize,
    rend: SimDuration,
    smashed: bool,
    cs_cd: SimDuration,
    sudden_death: u8,
    executioner: u8,
    cleave: bool,
    tweaks: Tweaks,
}

impl<'a> Arms<'a> {
    fn new(d: &'a Decision<SeatObs>, tweaks: Tweaks) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let execute = if legal.abilities.contains_key(&EXECUTE_MASSACRE) {
            EXECUTE_MASSACRE
        } else {
            EXECUTE
        };
        let mut a = Self {
            obs,
            legal,
            execute,
            enemies: obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len()),
            rend: obs.target_aura_remaining(REND_DOT),
            smashed: obs.target_aura(COLOSSUS_SMASH_DEBUFF).is_some(),
            cs_cd: SimDuration::ZERO,
            sudden_death: obs.buff_stacks(SUDDEN_DEATH),
            executioner: obs.buff_stacks(EXECUTIONER),
            cleave: legal.abilities.contains_key(&CLEAVE),
            tweaks,
        };
        a.cs_cd = a.cd(COLOSSUS_SMASH);
        a
    }

    fn known(&self, s: SpellId) -> bool {
        self.legal.abilities.contains_key(&s)
    }

    /// A cooldown the build lacks reads as never coming back.
    fn cd(&self, s: SpellId) -> SimDuration {
        if self.known(s) {
            self.obs.cooldown_remaining(s)
        } else {
            FOREVER
        }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
    }

    fn rage(&self) -> f64 {
        self.obs.resource(ResourceKind::Rage)
    }

    /// SimC's `variable.execute_phase`.
    fn execute_phase(&self) -> bool {
        let hp = self.obs.target_health_pct().unwrap_or(100.0);
        hp < 20.0 || self.known(EXECUTE_MASSACRE) && hp < 35.0
    }

    /// Bladestorm's own condition, then the tweaks' on top.
    fn bladestorm(&self, simc: bool) -> Option<SpellId> {
        let t = self.tweaks;
        let demise = t
            .bladestorm_demise
            .is_none_or(|n| self.obs.buff_stacks(IMMINENT_DEMISE) >= n);
        self.pick(BLADESTORM, (t.bladestorm_on_cd || simc) && demise)
    }

    /// Into melee range of the target, ahead of any list.
    fn approach(&self) -> Option<Choice> {
        let t = self.obs.target.as_ref()?;
        (self.legal.can_move && self.obs.movement.is_none() && t.distance > MELEE_RANGE).then_some(
            Choice::Move(MoveGoal::Approach {
                target: t.actor,
                within: MELEE_RANGE,
            }),
        )
    }

    /// SimC's `charge,if=time<=0.5`.
    fn opener(&self) -> Option<SpellId> {
        self.pick(
            CHARGE,
            self.obs.combat_time.is_some_and(|t| t <= SimDuration(500)),
        )
    }

    /// SimC's `sweeping_strikes,if=active_enemies>1&buff.sweeping_strikes.down`.
    fn sweeping_strikes(&self) -> Option<SpellId> {
        let broad_cs_ready = self.obs.has_buff(BROAD_STROKES) && self.cs_cd <= SimDuration::ZERO;
        self.pick(
            SWEEPING_STRIKES,
            self.enemies > 1 && !self.obs.has_buff(SWEEPING_STRIKES_BUFF) && !broad_cs_ready,
        )
    }
}

/// Moves in, then plays the first spell the lists pick.
fn play(d: &Decision<SeatObs>, tweaks: Tweaks) -> Choice {
    let a = Arms::new(d, tweaks);
    if let Some(m) = a.approach() {
        return m;
    }
    let colossus = a.known(DEMOLISH);
    let list = if colossus {
        if a.enemies > 2 {
            colossus_aoe(&a)
        } else if a.execute_phase() {
            colossus_execute(&a)
        } else {
            colossus_st(&a)
        }
    } else if a.enemies >= 3 {
        slayer_aoe(&a)
    } else if a.execute_phase() {
        slayer_execute(&a)
    } else {
        slayer_st(&a)
    };
    super::use_buff_trinket(d, a.obs.has_buff(AVATAR_BUFF))
        .or_else(|| a.opener())
        .or(list)
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}

/// SimC's `slayer_st` (12.1). Sweeping Strikes, Ravager, and the
/// bladestorming Storm Bolt aren't modelled, and the Opportunist and
/// Collateral Damage lines are for talents the build skips. Heroic Strike
/// is Slam while Master of Warfare's proc is up.
fn slayer_st(a: &Arms<'_>) -> Option<SpellId> {
    let soon = SimDuration(2000);
    let t = a.tweaks;
    let rend_soon = if t.rend_last_gcd {
        a.rend <= GCD
    } else {
        a.rend <= GCD || a.cs_cd < soon && a.rend <= SimDuration(10_000)
    };
    let rend_late = !t.rend_last_gcd && a.rend <= SimDuration(5000);
    let warfare = a.obs.has_buff(MASTER_OF_WARFARE_PROC);
    let free_execute = a.sudden_death > 0 && (a.executioner > 0 || t.free_execute_ungated);
    let demise_execute = !t.no_demise_execute
        && a.cd(BLADESTORM) == SimDuration::ZERO
        && a.obs.buff_stacks(IMMINENT_DEMISE) < 1;
    [
        a.sweeping_strikes(),
        a.pick(CLEAVE, rend_soon),
        a.pick(REND, !a.cleave && a.enemies < 2 && rend_soon),
        a.pick(
            AVATAR,
            t.avatar_on_cd || a.smashed || a.cs_cd < SimDuration(10_000) && a.cs_cd > GCD,
        ),
        a.pick(a.execute, a.sudden_death == 2 || demise_execute),
        a.pick(COLOSSUS_SMASH, t.smash_on_cd || a.sudden_death < 2),
        a.bladestorm(a.smashed || a.obs.has_buff(AVATAR_BUFF)),
        a.pick(SLAM, warfare),
        a.pick(
            OVERPOWER,
            t.overpower_capped_first && a.obs.charges(OVERPOWER) >= 2,
        ),
        a.pick(a.execute, free_execute && t.free_execute_early),
        a.pick(MORTAL_STRIKE, true),
        a.pick(a.execute, free_execute),
        a.pick(OVERPOWER, true),
        a.pick(CLEAVE, a.enemies == 2 || rend_late),
        a.pick(REND, !a.cleave && rend_late),
        a.pick(SLAM, true),
        a.pick(WRECKING_THROW, a.enemies == 1),
    ]
    .into_iter()
    .flatten()
    .next()
}

/// SimC's `slayer_execute` (12.1) without the Opportunist and Collateral
/// Damage lines and without `target.time_to_die`.
fn slayer_execute(a: &Arms<'_>) -> Option<SpellId> {
    let t = a.tweaks;
    let bloodletting = a.obs.has_buff(BLOODLETTING);
    let rend_low = a.rend < SimDuration(2000) && !bloodletting;
    let precision = a.obs.buff_stacks(EXECUTIONERS_PRECISION);
    let ms_precision = t.execute_ms_precision.unwrap_or(2);
    let execute_rage = t.execute_rage.unwrap_or(40.0);
    [
        a.sweeping_strikes(),
        a.pick(CLEAVE, rend_low),
        a.pick(REND, rend_low && !a.cleave),
        a.pick(AVATAR, t.avatar_on_cd || a.smashed || a.cs_cd > GCD),
        a.pick(COLOSSUS_SMASH, true),
        a.pick(a.execute, t.execute_free_first && a.sudden_death > 0),
        a.pick(SLAM, a.obs.has_buff(MASTER_OF_WARFARE_PROC)),
        a.bladestorm(
            a.smashed
                || a.cs_cd > SimDuration(25_000)
                || a.obs.has_buff(AVATAR_BUFF)
                || precision == 2,
        ),
        a.pick(MORTAL_STRIKE, precision >= ms_precision),
        a.pick(
            OVERPOWER,
            t.execute_overpower_capped && a.obs.charges(OVERPOWER) >= 2,
        ),
        a.pick(a.execute, a.sudden_death > 0 && a.executioner > 0),
        a.pick(a.execute, a.rage() > execute_rage),
        a.pick(OVERPOWER, true),
        a.pick(a.execute, true),
        a.pick(WRECKING_THROW, true),
    ]
    .into_iter()
    .flatten()
    .next()
}

/// SimC's `slayer_aoe` (12.1) at 3 or more targets.
fn slayer_aoe(a: &Arms<'_>) -> Option<SpellId> {
    let soon = SimDuration(2000);
    let rend_soon = a.rend <= GCD || a.cs_cd < soon && a.rend <= SimDuration(10_000);
    let warfare = a.obs.has_buff(MASTER_OF_WARFARE_PROC);
    [
        a.sweeping_strikes(),
        a.pick(CLEAVE, rend_soon),
        a.pick(REND, !a.cleave && rend_soon),
        a.pick(
            AVATAR,
            a.smashed || a.cs_cd < SimDuration(10_000) && a.cs_cd > GCD,
        ),
        a.pick(COLOSSUS_SMASH, true),
        a.bladestorm(a.smashed || a.cs_cd > SimDuration(25_000) || a.obs.has_buff(AVATAR_BUFF)),
        a.pick(CLEAVE, true),
        a.pick(SLAM, warfare),
        a.pick(a.execute, a.sudden_death > 0),
        a.pick(
            MORTAL_STRIKE,
            a.obs.has_buff(SWEEPING_STRIKES_BUFF) && a.obs.buff_stacks(EXECUTIONERS_PRECISION) == 2,
        ),
        a.pick(OVERPOWER, a.obs.has_buff(SWEEPING_STRIKES_BUFF)),
        a.pick(MORTAL_STRIKE, true),
        a.pick(OVERPOWER, true),
        a.pick(a.execute, true),
        a.pick(SLAM, true),
    ]
    .into_iter()
    .flatten()
    .next()
}

/// SimC's `colossus_st` (12.1).
fn colossus_st(a: &Arms<'_>) -> Option<SpellId> {
    let soon = SimDuration(2000);
    let rend_soon = a.rend <= GCD || a.cs_cd < soon && a.rend <= SimDuration(10_000);
    let rend_late = a.rend <= SimDuration(5000);
    let warfare = a.obs.has_buff(MASTER_OF_WARFARE_PROC);
    let cm = a.obs.buff_stacks(COLOSSAL_MIGHT);
    [
        a.sweeping_strikes(),
        a.pick(CLEAVE, rend_soon),
        a.pick(REND, !a.cleave && rend_soon),
        a.pick(
            AVATAR,
            a.smashed || a.cs_cd < SimDuration(10_000) && a.cs_cd > GCD,
        ),
        a.pick(COLOSSUS_SMASH, true),
        a.pick(DEMOLISH, a.smashed && cm >= 6 || cm == 10),
        a.pick(SLAM, warfare),
        a.pick(MORTAL_STRIKE, true),
        a.pick(a.execute, a.sudden_death > 0),
        a.pick(OVERPOWER, true),
        a.pick(CLEAVE, a.enemies == 2 || rend_late),
        a.pick(REND, !a.cleave && rend_late),
        a.pick(SLAM, true),
        a.pick(WRECKING_THROW, a.enemies == 1),
    ]
    .into_iter()
    .flatten()
    .next()
}

/// SimC's `colossus_execute` (12.1).
fn colossus_execute(a: &Arms<'_>) -> Option<SpellId> {
    let bloodletting = a.obs.has_buff(BLOODLETTING);
    let rend_low = a.rend < SimDuration(2000) && !bloodletting;
    let precision = a.obs.buff_stacks(EXECUTIONERS_PRECISION);
    let warfare = a.obs.has_buff(MASTER_OF_WARFARE_PROC);
    let cm = a.obs.buff_stacks(COLOSSAL_MIGHT);
    [
        a.sweeping_strikes(),
        a.pick(CLEAVE, rend_low),
        a.pick(REND, rend_low && !a.cleave),
        a.pick(AVATAR, a.smashed || a.cs_cd > GCD),
        a.pick(COLOSSUS_SMASH, true),
        a.pick(
            DEMOLISH,
            a.smashed || cm == 10 && a.cs_cd > SimDuration(15_000),
        ),
        a.pick(SLAM, warfare),
        a.pick(
            MORTAL_STRIKE,
            precision == 2 || !a.obs.has_buff(COLOSSAL_MIGHT),
        ),
        a.pick(a.execute, a.rage() >= 40.0 || a.sudden_death > 0),
        a.pick(OVERPOWER, true),
        a.pick(a.execute, true),
        a.pick(MORTAL_STRIKE, true),
        a.pick(CLEAVE, a.enemies == 2),
        a.pick(SLAM, true),
    ]
    .into_iter()
    .flatten()
    .next()
}

/// SimC's `colossus_aoe` (12.1).
fn colossus_aoe(a: &Arms<'_>) -> Option<SpellId> {
    let soon = SimDuration(2000);
    let rend_soon = a.rend <= GCD || a.cs_cd < soon && a.rend <= SimDuration(10_000);
    let warfare = a.obs.has_buff(MASTER_OF_WARFARE_PROC);
    let cm = a.obs.buff_stacks(COLOSSAL_MIGHT);
    [
        a.sweeping_strikes(),
        a.pick(CLEAVE, rend_soon || a.obs.has_buff(COLLATERAL_DAMAGE_BUFF)),
        a.pick(REND, !a.cleave && rend_soon),
        a.pick(
            AVATAR,
            a.smashed || a.cs_cd < SimDuration(10_000) && a.cs_cd > GCD,
        ),
        a.pick(COLOSSUS_SMASH, true),
        a.pick(
            DEMOLISH,
            a.smashed || cm >= 8 && a.cs_cd > SimDuration(15_000),
        ),
        a.pick(BLADESTORM, a.smashed || a.cs_cd > SimDuration(25_000)),
        a.pick(CLEAVE, true),
        a.pick(SLAM, warfare),
        a.pick(OVERPOWER, a.obs.has_buff(SWEEPING_STRIKES_BUFF)),
        a.pick(MORTAL_STRIKE, true),
        a.pick(a.execute, a.sudden_death > 0),
        a.pick(OVERPOWER, true),
        a.pick(a.execute, true),
        a.pick(SLAM, true),
    ]
    .into_iter()
    .flatten()
    .next()
}

/// SimulationCraft's Arms Slayer lists: `slayer_execute` in the execute
/// phase and `slayer_st` otherwise, as its default list runs them below
/// three targets.
pub fn arms(d: &Decision<SeatObs>) -> Choice {
    play(d, PORT)
}

/// The candidates, by name: each is the best so far plus one change.
pub const CANDIDATES: &[(&str, super::Script)] = &[
    ("arms_op_capped", |d| {
        play(
            d,
            Tweaks {
                execute_overpower_capped: true,
                ..PORT
            },
        )
    }),
    ("arms_avatar_cd", |d| {
        play(
            d,
            Tweaks {
                avatar_on_cd: true,
                ..PORT
            },
        )
    }),
    ("arms_bladestorm_cd", |d| {
        play(
            d,
            Tweaks {
                bladestorm_on_cd: true,
                ..PORT
            },
        )
    }),
    ("arms_smash_cd", |d| {
        play(
            d,
            Tweaks {
                smash_on_cd: true,
                ..PORT
            },
        )
    }),
    ("arms_demise3", |d| {
        play(
            d,
            Tweaks {
                bladestorm_demise: Some(3),
                ..PORT
            },
        )
    }),
    ("arms_ungated", |d| {
        play(
            d,
            Tweaks {
                free_execute_ungated: true,
                ..PORT
            },
        )
    }),
    ("arms_no_demise_execute", |d| {
        play(
            d,
            Tweaks {
                no_demise_execute: true,
                ..PORT
            },
        )
    }),
    ("arms_rend_last", |d| {
        play(
            d,
            Tweaks {
                rend_last_gcd: true,
                ..PORT
            },
        )
    }),
    ("arms_op_first", |d| {
        play(
            d,
            Tweaks {
                overpower_capped_first: true,
                ..PORT
            },
        )
    }),
    ("arms_execute_early", |d| {
        play(
            d,
            Tweaks {
                free_execute_early: true,
                ..PORT
            },
        )
    }),
    ("arms_execute_free_first", |d| {
        play(
            d,
            Tweaks {
                execute_free_first: true,
                ..PORT
            },
        )
    }),
    ("arms_ms_precision1", |d| {
        play(
            d,
            Tweaks {
                execute_ms_precision: Some(1),
                ..PORT
            },
        )
    }),
    ("arms_execute_rage20", |d| {
        play(
            d,
            Tweaks {
                execute_rage: Some(20.0),
                ..PORT
            },
        )
    }),
];
