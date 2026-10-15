//! Fury Warrior.
//!
//! [`fury`] ports SimC's 12.1 Slayer and Mountain Thane priority lists
//! (`profiles/MID2/MID2_Warrior_Fury.simc`).

use portunus_core::{AuraId, HeroTreeId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, MoveGoal, Wait};
use portunus_env::Decision;
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::stats::ResourceKind;

use crate::observe::{SeatObs, FOREVER};

const BLOODTHIRST: SpellId = SpellId(23881);
const RAGING_BLOW: SpellId = SpellId(85288);
const RAMPAGE: SpellId = SpellId(184367);
const EXECUTE: SpellId = SpellId(5308);
const EXECUTE_MASSACRE: SpellId = SpellId(280735);
const WHIRLWIND: SpellId = SpellId(190411);
const SLAM: SpellId = SpellId(1464);
const RECKLESSNESS: SpellId = SpellId(1719);
const AVATAR: SpellId = SpellId(107574);
const BLADESTORM: SpellId = SpellId(227847);
const ODYNS_FURY: SpellId = SpellId(385059);
const THUNDER_CLAP: SpellId = SpellId(6343);
const WRECKING_THROW: SpellId = SpellId(384110);
const STORM_BOLT: SpellId = SpellId(107570);
const CHARGE: SpellId = SpellId(100);

const ENRAGE: AuraId = AuraId(184362);
const SUDDEN_DEATH: AuraId = AuraId(52437);
const WHIRLWIND_BUFF: AuraId = AuraId(85739);
const RECKLESSNESS_BUFF: AuraId = AuraId(1719);
const ASHEN_JUGGERNAUT: AuraId = AuraId(392537);
const BLOODCRAZE: AuraId = AuraId(383855);
const BLOODBATH_PROC: AuraId = AuraId(335082);
const CRUSHING_BLOW_PROC: AuraId = AuraId(335091);
const DEFT_EXPERIENCE: AuraId = AuraId(1265341);
const IMPROVED_WHIRLWIND: AuraId = AuraId(280772);
const MEAT_CLEAVER: AuraId = AuraId(280392);
const RECKLESS_ABANDON: AuraId = AuraId(392936);
const ANGER_MANAGEMENT: AuraId = AuraId(152278);
const BERSERKERS_TORMENT: AuraId = AuraId(1299953);
const EXECUTIONER: AuraId = AuraId(445584);
const BRUTAL_FINISH_RB: AuraId = AuraId(446088);
const OPPORTUNIST_BUFF: AuraId = AuraId(456120);
const THUNDER_BLAST_BUFF: AuraId = AuraId(435615);
const BURST_OF_POWER: AuraId = AuraId(435215);

struct Fury<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    execute: SpellId,
    enemies: usize,
    enraged: bool,
    enrage_rem: SimDuration,
    ww_stacks: u8,
    sudden_death: u8,
    rage: f64,
    execute_phase: bool,
}

impl<'a> Fury<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let execute = if legal.abilities.contains_key(&EXECUTE_MASSACRE) {
            EXECUTE_MASSACRE
        } else {
            EXECUTE
        };
        let hp = obs.target_health_pct().unwrap_or(100.0);
        let execute_phase =
            hp < 20.0 || legal.abilities.contains_key(&EXECUTE_MASSACRE) && hp < 35.0;
        let enrage_rem = obs.buff_remaining(ENRAGE);
        Self {
            obs,
            legal,
            execute,
            enemies: obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len()),
            enraged: enrage_rem > SimDuration::ZERO,
            enrage_rem,
            ww_stacks: obs.buff_stacks(WHIRLWIND_BUFF),
            sudden_death: obs.buff_stacks(SUDDEN_DEATH),
            rage: obs.resource(ResourceKind::Rage),
            execute_phase,
        }
    }

    fn known(&self, s: SpellId) -> bool {
        self.legal.abilities.contains_key(&s)
    }

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

    fn approach(&self) -> Option<Choice> {
        let t = self.obs.target.as_ref()?;
        (self.legal.can_move && self.obs.movement.is_none() && t.distance > MELEE_RANGE).then_some(
            Choice::Move(MoveGoal::Approach {
                target: t.actor,
                within: MELEE_RANGE,
            }),
        )
    }

    fn opener(&self) -> Option<SpellId> {
        self.pick(
            CHARGE,
            self.obs.combat_time.is_some_and(|t| t <= SimDuration(500)),
        )
    }

    fn cooldowns(&self) -> Option<SpellId> {
        let reck_ready = self.cd(RECKLESSNESS) <= SimDuration::ZERO;
        let avatar_cond = !self.obs.has_buff(BERSERKERS_TORMENT)
            || self.obs.has_buff(RECKLESSNESS_BUFF)
            || reck_ready
            || self.cd(RECKLESSNESS) > SimDuration(30_000);
        let reck_cond = !self.obs.has_buff(ANGER_MANAGEMENT)
            && (!self.obs.has_buff(BERSERKERS_TORMENT)
                || self.cd(AVATAR) < SimDuration(2_000)
                || self.cd(AVATAR) > SimDuration(35_000))
            || self.obs.has_buff(ANGER_MANAGEMENT);
        self.pick(RECKLESSNESS, reck_cond)
            .or_else(|| self.pick(AVATAR, avatar_cond))
    }
}

fn slayer_aoe(f: &Fury<'_>) -> Option<SpellId> {
    let ww_cleave = f.obs.has_buff(IMPROVED_WHIRLWIND) && f.ww_stacks == 0;
    let deft = f.obs.has_buff(DEFT_EXPERIENCE);
    [
        f.pick(WHIRLWIND, ww_cleave),
        f.pick(
            THUNDER_CLAP,
            f.obs.has_buff(MEAT_CLEAVER) && f.ww_stacks == 0,
        ),
        f.cooldowns(),
        f.pick(RAMPAGE, !f.enraged || f.rage >= 110.0),
        f.pick(BLADESTORM, f.enraged),
        f.pick(ODYNS_FURY, true),
        f.pick(
            f.execute,
            f.sudden_death > 0 && f.obs.buff_stacks(EXECUTIONER) >= 2,
        ),
        f.pick(BLOODTHIRST, deft && f.enrage_rem <= SimDuration(2_000)),
        f.pick(
            RAGING_BLOW,
            f.obs.has_buff(OPPORTUNIST_BUFF) || f.obs.has_buff(BRUTAL_FINISH_RB),
        ),
        f.pick(f.execute, f.execute_phase || f.sudden_death > 0),
        f.pick(RAGING_BLOW, f.obs.charges(RAGING_BLOW) >= 2),
        f.pick(RAMPAGE, true),
        f.pick(BLOODTHIRST, true),
        f.pick(RAGING_BLOW, true),
        f.pick(WHIRLWIND, true),
    ]
    .into_iter()
    .flatten()
    .next()
}

fn slayer_st(f: &Fury<'_>) -> Option<SpellId> {
    let ww_cleave = f.enemies > 1 && f.obs.has_buff(IMPROVED_WHIRLWIND) && f.ww_stacks == 0;
    let deft = f.obs.has_buff(DEFT_EXPERIENCE);
    let reckless_abandon = f.obs.has_buff(RECKLESS_ABANDON);
    let ashen_expiring = f.obs.has_buff(ASHEN_JUGGERNAUT)
        && f.obs.buff_remaining(ASHEN_JUGGERNAUT) <= SimDuration(2_000);
    let sd_expiring =
        f.sudden_death > 0 && f.obs.buff_remaining(SUDDEN_DEATH) <= SimDuration(2_000);
    [
        f.pick(WHIRLWIND, ww_cleave),
        f.cooldowns(),
        f.pick(RAMPAGE, !f.enraged),
        f.pick(
            f.execute,
            ashen_expiring || sd_expiring || f.sudden_death >= 2,
        ),
        f.pick(
            BLADESTORM,
            f.enraged && f.cd(RECKLESSNESS) > SimDuration(10_000),
        ),
        f.pick(
            ODYNS_FURY,
            (!f.enraged
                || f.enrage_rem <= SimDuration(2_000)
                || f.cd(AVATAR) > SimDuration(10_000))
                && f.rage < 100.0,
        ),
        f.pick(BLOODTHIRST, deft && f.enrage_rem <= SimDuration(2_000)),
        f.pick(
            RAMPAGE,
            f.rage >= 100.0 && reckless_abandon && !f.obs.has_buff(CRUSHING_BLOW_PROC),
        ),
        f.pick(
            RAGING_BLOW,
            f.obs.has_buff(CRUSHING_BLOW_PROC)
                && (f.obs.has_buff(OPPORTUNIST_BUFF)
                    || f.obs.has_buff(BRUTAL_FINISH_RB)
                    || f.obs.charges(RAGING_BLOW) == 2),
        ),
        f.pick(
            f.execute,
            f.enraged && f.sudden_death > 0 && f.obs.buff_stacks(EXECUTIONER) >= 2,
        ),
        f.pick(BLOODTHIRST, f.obs.has_buff(BLOODBATH_PROC)),
        f.pick(RAMPAGE, f.rage >= 115.0),
        f.pick(RAGING_BLOW, f.obs.has_buff(CRUSHING_BLOW_PROC)),
        f.pick(
            f.execute,
            f.enraged && f.execute_phase && !f.obs.has_buff(RECKLESSNESS_BUFF),
        ),
        f.pick(
            RAGING_BLOW,
            f.obs.has_buff(OPPORTUNIST_BUFF)
                || f.obs.has_buff(BRUTAL_FINISH_RB)
                || f.obs.charges(RAGING_BLOW) == 2,
        ),
        f.pick(
            BLOODTHIRST,
            !f.enraged || !reckless_abandon || f.obs.buff_stacks(BLOODCRAZE) >= 2,
        ),
        f.pick(RAMPAGE, true),
        f.pick(f.execute, f.sudden_death > 0),
        f.pick(BLOODTHIRST, true),
        f.pick(RAGING_BLOW, true),
        f.pick(SLAM, true),
        f.pick(WRECKING_THROW, true),
        f.pick(STORM_BOLT, true),
    ]
    .into_iter()
    .flatten()
    .next()
}

fn thane_aoe(f: &Fury<'_>) -> Option<SpellId> {
    let meat_cleaver = f.obs.has_buff(MEAT_CLEAVER);
    let tb = f.obs.has_buff(THUNDER_BLAST_BUFF);
    let deft = f.obs.has_buff(DEFT_EXPERIENCE);
    [
        f.pick(THUNDER_CLAP, meat_cleaver && f.ww_stacks == 0 && tb),
        f.pick(THUNDER_CLAP, meat_cleaver && f.ww_stacks == 0),
        f.pick(RAMPAGE, !f.enraged),
        f.pick(THUNDER_CLAP, f.enraged && tb),
        f.cooldowns(),
        f.pick(ODYNS_FURY, true),
        f.pick(BLOODTHIRST, deft && f.enrage_rem <= SimDuration(2_000)),
        f.pick(BLADESTORM, f.enraged),
        f.pick(RAMPAGE, f.rage >= 110.0),
        f.pick(f.execute, f.execute_phase || f.sudden_death > 0),
        f.pick(
            BLOODTHIRST,
            f.obs.has_buff(BLOODBATH_PROC) || f.obs.buff_stacks(BURST_OF_POWER) == 2,
        ),
        f.pick(RAGING_BLOW, f.obs.has_buff(CRUSHING_BLOW_PROC)),
        f.pick(RAMPAGE, true),
        f.pick(BLOODTHIRST, true),
        f.pick(RAGING_BLOW, true),
        f.pick(THUNDER_CLAP, true),
        f.pick(WHIRLWIND, true),
    ]
    .into_iter()
    .flatten()
    .next()
}

fn thane_st(f: &Fury<'_>) -> Option<SpellId> {
    let meat_cleaver = f.obs.has_buff(MEAT_CLEAVER);
    let tb = f.obs.has_buff(THUNDER_BLAST_BUFF);
    let tb_stacks = f.obs.buff_stacks(THUNDER_BLAST_BUFF);
    let deft = f.obs.has_buff(DEFT_EXPERIENCE);
    let reckless_abandon = f.obs.has_buff(RECKLESS_ABANDON);
    let ashen_expiring = f.obs.has_buff(ASHEN_JUGGERNAUT)
        && f.obs.buff_remaining(ASHEN_JUGGERNAUT) <= SimDuration(2_000);
    let sd_expiring =
        f.sudden_death > 0 && f.obs.buff_remaining(SUDDEN_DEATH) <= SimDuration(2_000);
    [
        f.pick(
            THUNDER_CLAP,
            f.enemies > 1 && meat_cleaver && f.ww_stacks == 0,
        ),
        f.cooldowns(),
        f.pick(THUNDER_CLAP, tb_stacks == 2 && f.enraged),
        f.pick(RAMPAGE, !f.enraged),
        f.pick(
            ODYNS_FURY,
            !f.enraged || f.enrage_rem <= SimDuration(2_000) || f.cd(AVATAR) > SimDuration(10_000),
        ),
        f.pick(BLOODTHIRST, deft && f.enrage_rem <= SimDuration(2_000)),
        f.pick(f.execute, ashen_expiring || sd_expiring),
        f.pick(
            RAMPAGE,
            f.rage >= 115.0 && reckless_abandon && !f.obs.has_buff(BLOODBATH_PROC),
        ),
        f.pick(BLOODTHIRST, f.obs.has_buff(BLOODBATH_PROC)),
        f.pick(THUNDER_CLAP, tb && f.enraged),
        f.pick(BLADESTORM, f.enraged && f.enemies > 1),
        f.pick(
            RAGING_BLOW,
            f.obs.has_buff(CRUSHING_BLOW_PROC) && f.obs.charges(RAGING_BLOW) == 2,
        ),
        f.pick(RAMPAGE, f.rage >= 115.0),
        f.pick(
            BLOODTHIRST,
            f.obs.buff_stacks(BURST_OF_POWER) == 2 || f.obs.buff_stacks(BLOODCRAZE) >= 2,
        ),
        f.pick(RAGING_BLOW, f.obs.has_buff(CRUSHING_BLOW_PROC)),
        f.pick(f.execute, f.enraged),
        f.pick(
            RAGING_BLOW,
            f.obs.charges(RAGING_BLOW) >= 1 && reckless_abandon,
        ),
        f.pick(RAMPAGE, true),
        f.pick(BLOODTHIRST, !reckless_abandon),
        f.pick(RAGING_BLOW, true),
        f.pick(BLOODTHIRST, true),
        f.pick(THUNDER_CLAP, true),
        f.pick(SLAM, true),
        f.pick(STORM_BOLT, true),
    ]
    .into_iter()
    .flatten()
    .next()
}

/// SimulationCraft's 12.1 Fury priority list (Slayer and Mountain Thane).
pub fn fury(d: &Decision<SeatObs>) -> Choice {
    let f = Fury::new(d);
    if let Some(m) = f.approach() {
        return m;
    }
    let thane = d.obs.hero_tree == Some(HeroTreeId(61));
    let list = if thane {
        if f.enemies >= 2 {
            thane_aoe(&f)
        } else {
            thane_st(&f)
        }
    } else if f.enemies >= 2 {
        slayer_aoe(&f)
    } else {
        slayer_st(&f)
    };
    f.opener()
        .or(list)
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
