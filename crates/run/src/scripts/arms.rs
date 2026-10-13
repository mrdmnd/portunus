//! Arms Warrior.
//!
//! [`arms`] is the port of SimC's list. The other policies here are
//! candidates, each the best so far plus one change; `arms_h4` is the
//! current best, and `docs/ARMS_TUNING.md` records how each did.

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

const SUDDEN_DEATH: AuraId = AuraId(52437);
const COLOSSUS_SMASH_DEBUFF: AuraId = AuraId(208086);
const REND_DOT: AuraId = AuraId(388539);
const AVATAR_BUFF: AuraId = AuraId(107574);
const MASTER_OF_WARFARE_PROC: AuraId = AuraId(1269391);
const EXECUTIONERS_PRECISION: AuraId = AuraId(386633);
const BLOODLETTING: AuraId = AuraId(383154);

/// The changes a candidate makes to the SimC port, one per field, all off
/// for [`arms`].
#[derive(Debug, Clone, Copy, Default)]
struct Tweaks {
    /// Run `slayer_execute` in the execute phase.
    execute_list: bool,
    /// Outside the execute phase, spend Sudden Death whenever it's up, at
    /// this point in the list.
    free_execute: Option<FreeExecute>,
    /// In the execute phase, a free Execute right after Colossus Smash.
    free_execute_first: bool,
    /// Slam (not Heroic Strike) only above this much rage.
    slam_rage: Option<f64>,
    /// No Slam once the target is below this health percent and not yet
    /// in the execute phase, to start it with rage.
    pool_below: Option<f64>,
    /// Overpower ahead of Mortal Strike: only with both charges (`true`),
    /// or always.
    overpower_first: Option<bool>,
    /// In the execute phase, Overpower with both charges ahead of Execute.
    execute_overpower_capped: bool,
    /// Avatar, Bladestorm, and Colossus Smash as soon as ready, ignoring
    /// SimC's alignment conditions.
    avatar_on_cd: bool,
    bladestorm_on_cd: bool,
    smash_on_cd: bool,
    /// Cleave for Rend only when it's about to fall off, or never.
    rend_upkeep: Option<RendUpkeep>,
    /// In the execute phase, Mortal Strike with at least this many
    /// Executioner's Precision stacks instead of SimC's 2.
    execute_ms_precision: Option<u8>,
    /// In the execute phase, Execute ahead of Overpower only above this
    /// much rage instead of SimC's 40.
    execute_rage: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RendUpkeep {
    LastGcd,
    Never,
}

/// Where the free Execute goes: just ahead of this ability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FreeExecute {
    HeroicStrike,
    MortalStrike,
    Overpower,
}

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
}

/// Moves in, then plays the first spell the lists pick.
fn play(d: &Decision<SeatObs>, tweaks: Tweaks) -> Choice {
    let a = Arms::new(d, tweaks);
    if let Some(m) = a.approach() {
        return m;
    }
    let list = if tweaks.execute_list && a.execute_phase() {
        slayer_execute(&a)
    } else {
        slayer_st(&a)
    };
    a.opener()
        .or(list)
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}

/// SimC's `slayer_st` (12.1) without the Slayer talents: their buffs never
/// come up, so Imminent Demise's `stack<1` always holds and the
/// Executioner and Opportunist lines never fire. Sweeping Strikes,
/// Ravager, and the bladestorming Storm Bolt aren't modelled. Heroic
/// Strike is Slam while Master of Warfare's proc is up.
fn slayer_st(a: &Arms<'_>) -> Option<SpellId> {
    let soon = SimDuration(2000);
    let t = a.tweaks;
    let rend_soon = match t.rend_upkeep {
        None => a.rend <= GCD || a.cs_cd < soon && a.rend <= SimDuration(10_000),
        Some(RendUpkeep::LastGcd) => a.rend <= GCD,
        Some(RendUpkeep::Never) => false,
    };
    let rend_late = t.rend_upkeep.is_none() && a.rend <= SimDuration(5000);
    let op_capped = a.obs.charges(OVERPOWER) >= 2;
    let op_first = match t.overpower_first {
        None => false,
        Some(true) => op_capped,
        Some(false) => true,
    };
    let free = a.sudden_death > 0;
    let free_execute = a.tweaks.free_execute;
    let warfare = a.obs.has_buff(MASTER_OF_WARFARE_PROC);
    let hp = a.obs.target_health_pct().unwrap_or(100.0);
    let slam_ok = a.tweaks.slam_rage.is_none_or(|r| a.rage() > r)
        && a.tweaks.pool_below.is_none_or(|p| hp >= p);
    [
        a.pick(CLEAVE, rend_soon),
        a.pick(REND, !a.cleave && a.enemies < 2 && rend_soon),
        a.pick(
            AVATAR,
            t.avatar_on_cd || a.smashed || a.cs_cd < SimDuration(10_000) && a.cs_cd > GCD,
        ),
        a.pick(
            a.execute,
            a.sudden_death == 2 || a.cd(BLADESTORM) == SimDuration::ZERO,
        ),
        a.pick(COLOSSUS_SMASH, t.smash_on_cd || a.sudden_death < 2),
        a.pick(
            BLADESTORM,
            t.bladestorm_on_cd || a.smashed || a.obs.has_buff(AVATAR_BUFF),
        ),
        a.pick(
            a.execute,
            free && free_execute == Some(FreeExecute::HeroicStrike),
        ),
        a.pick(SLAM, warfare),
        a.pick(
            a.execute,
            free && free_execute == Some(FreeExecute::MortalStrike),
        ),
        a.pick(OVERPOWER, op_first),
        a.pick(MORTAL_STRIKE, true),
        a.pick(
            a.execute,
            free && free_execute == Some(FreeExecute::Overpower),
        ),
        a.pick(OVERPOWER, true),
        a.pick(CLEAVE, a.enemies == 2 || rend_late),
        a.pick(REND, !a.cleave && rend_late),
        a.pick(SLAM, warfare || slam_ok),
        a.pick(WRECKING_THROW, a.enemies == 1),
    ]
    .into_iter()
    .flatten()
    .next()
}

/// SimC's `slayer_execute` (12.1) without the Slayer lines (Executioner,
/// Opportunist, Collateral Damage) and without `target.time_to_die`.
fn slayer_execute(a: &Arms<'_>) -> Option<SpellId> {
    let bloodletting = a.obs.has_buff(BLOODLETTING);
    let rend_low = a.rend < SimDuration(2000) && !bloodletting;
    let precision = a.obs.buff_stacks(EXECUTIONERS_PRECISION);
    let ms_precision = a.tweaks.execute_ms_precision.unwrap_or(2);
    let execute_rage = a.tweaks.execute_rage.unwrap_or(40.0);
    [
        a.pick(CLEAVE, rend_low),
        a.pick(REND, rend_low && !a.cleave),
        a.pick(AVATAR, a.smashed || a.cs_cd > GCD),
        a.pick(COLOSSUS_SMASH, true),
        a.pick(a.execute, a.tweaks.free_execute_first && a.sudden_death > 0),
        a.pick(SLAM, a.obs.has_buff(MASTER_OF_WARFARE_PROC)),
        a.pick(
            BLADESTORM,
            a.smashed
                || a.cs_cd > SimDuration(25_000)
                || a.obs.has_buff(AVATAR_BUFF)
                || precision == 2,
        ),
        a.pick(MORTAL_STRIKE, precision >= ms_precision),
        a.pick(
            OVERPOWER,
            a.tweaks.execute_overpower_capped && a.obs.charges(OVERPOWER) >= 2,
        ),
        a.pick(a.execute, a.rage() > execute_rage),
        a.pick(OVERPOWER, true),
        a.pick(a.execute, true),
        a.pick(WRECKING_THROW, true),
    ]
    .into_iter()
    .flatten()
    .next()
}

/// SimulationCraft's Arms `slayer_st` list for the whole fight. Its
/// default list runs it below three targets when Slayer is talented; this
/// build has no hero tree, so SimC itself would run no list at all.
pub fn arms(d: &Decision<SeatObs>) -> Choice {
    play(d, Tweaks::default())
}

const H1: Tweaks = Tweaks {
    execute_list: true,
    free_execute: None,
    free_execute_first: false,
    slam_rage: None,
    pool_below: None,
    overpower_first: None,
    execute_overpower_capped: false,
    avatar_on_cd: false,
    bladestorm_on_cd: false,
    smash_on_cd: false,
    rend_upkeep: None,
    execute_ms_precision: None,
    execute_rage: None,
};

const H2: Tweaks = Tweaks {
    free_execute: Some(FreeExecute::Overpower),
    ..H1
};

const H3: Tweaks = Tweaks {
    execute_overpower_capped: true,
    ..H2
};

const H4: Tweaks = Tweaks {
    bladestorm_on_cd: true,
    ..H3
};

/// The candidates, by name: each is the best so far plus one change.
pub const CANDIDATES: &[(&str, super::Script)] = &[
    ("arms_h1", |d| play(d, H1)),
    ("arms_h2", |d| play(d, H2)),
    ("arms_h3", |d| play(d, H3)),
    ("arms_h4", |d| play(d, H4)),
    ("arms_h10a", |d| {
        play(
            d,
            Tweaks {
                overpower_first: Some(true),
                ..H2
            },
        )
    }),
    ("arms_h10b", |d| {
        play(
            d,
            Tweaks {
                overpower_first: Some(false),
                ..H2
            },
        )
    }),
    ("arms_h11", |d| {
        play(
            d,
            Tweaks {
                execute_overpower_capped: true,
                ..H2
            },
        )
    }),
    ("arms_h12", |d| {
        play(
            d,
            Tweaks {
                avatar_on_cd: true,
                ..H2
            },
        )
    }),
    ("arms_h13", |d| {
        play(
            d,
            Tweaks {
                bladestorm_on_cd: true,
                ..H2
            },
        )
    }),
    ("arms_h14", |d| {
        play(
            d,
            Tweaks {
                smash_on_cd: true,
                ..H2
            },
        )
    }),
    ("arms_h15a", |d| {
        play(
            d,
            Tweaks {
                rend_upkeep: Some(RendUpkeep::LastGcd),
                ..H2
            },
        )
    }),
    ("arms_h15b", |d| {
        play(
            d,
            Tweaks {
                rend_upkeep: Some(RendUpkeep::Never),
                ..H2
            },
        )
    }),
];
