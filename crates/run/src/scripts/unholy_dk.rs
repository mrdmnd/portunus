//! Unholy Death Knight.
//!
//! [`unholy_dk`] ports SimC's 12.1 Rider of the Apocalypse and San'layn
//! priority lists (`profiles/MID2/MID2_Death_Knight_Unholy.simc`).

use portunus_core::{AuraId, PetId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, MoveGoal, Wait};
use portunus_env::Decision;
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::stats::ResourceKind;

use crate::observe::{SeatObs, FOREVER};

const OUTBREAK: SpellId = SpellId(77575);
const FESTERING_STRIKE: SpellId = SpellId(85948);
const SCOURGE_STRIKE: SpellId = SpellId(55090);
const DEATH_COIL: SpellId = SpellId(47541);
const EPIDEMIC: SpellId = SpellId(207317);
const PUTREFY: SpellId = SpellId(1247378);
const SOUL_REAPER: SpellId = SpellId(343294);
const DARK_TRANSFORMATION: SpellId = SpellId(1233448);
const ARMY_OF_THE_DEAD: SpellId = SpellId(42650);
const DEATH_AND_DECAY: SpellId = SpellId(43265);
const RAISE_DEAD: SpellId = SpellId(46584);

const ARMY_GHOUL_PET: PetId = PetId(24207);

const SUDDEN_DOOM_BUFF: AuraId = AuraId(81340);
const DARK_TRANSFORMATION_BUFF: AuraId = AuraId(1235391);
const FORBIDDEN_KNOWLEDGE_BUFF: AuraId = AuraId(1242223);
const LESSER_GHOUL_READY_BUFF: AuraId = AuraId(1254252);
const FESTERING_SCYTHE_BUFF: AuraId = AuraId(458123);
const FESTERING_SCYTHE_TT_BUFF: AuraId = AuraId(1241077);
const BLIGHTFALL_BUFF: AuraId = AuraId(1271975);
const VAMPIRIC_STRIKE_BUFF: AuraId = AuraId(433899);
const ESSENCE_OF_THE_BLOOD_QUEEN_BUFF: AuraId = AuraId(433925);

const DREAD_PLAGUE_DOT: AuraId = AuraId(1240996);

const FESTERING_SCYTHE_TALENT: AuraId = AuraId(455397);
const BLIGHTBURST_TALENT: AuraId = AuraId(1254552);
const CYCLE_OF_DEATH_TALENT: AuraId = AuraId(1290864);
const FRENZIED_BLOODTHIRST_TALENT: AuraId = AuraId(434075);

struct Unholy<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    runes: f64,
    rp_deficit: f64,
    dt_up: bool,
    fk_up: bool,
    sd_up: bool,
    vs_up: bool,
    ebq_stacks: u8,
    ebq_max: u8,
    ebq_rem: SimDuration,
    ghoul_ready: u8,
}

impl<'a> Unholy<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let enemies = obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len());
        let runes = obs.resource(ResourceKind::Runes);
        let rp = obs.resource(ResourceKind::RunicPower);
        let rp_max = obs
            .resource_max
            .get(&ResourceKind::RunicPower)
            .copied()
            .unwrap_or(100.0);
        let ebq_max = if obs.has_buff(FRENZIED_BLOODTHIRST_TALENT) {
            7
        } else {
            5
        };
        Self {
            obs,
            legal,
            enemies,
            runes,
            rp_deficit: (rp_max - rp).max(0.0),
            dt_up: obs.has_buff(DARK_TRANSFORMATION_BUFF),
            fk_up: obs.has_buff(FORBIDDEN_KNOWLEDGE_BUFF),
            sd_up: obs.has_buff(SUDDEN_DOOM_BUFF),
            vs_up: obs.has_buff(VAMPIRIC_STRIKE_BUFF),
            ebq_stacks: obs.buff_stacks(ESSENCE_OF_THE_BLOOD_QUEEN_BUFF),
            ebq_max,
            ebq_rem: obs.buff_remaining(ESSENCE_OF_THE_BLOOD_QUEEN_BUFF),
            ghoul_ready: obs.buff_stacks(LESSER_GHOUL_READY_BUFF),
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

    fn precombat(&self) -> Option<SpellId> {
        self.pick(RAISE_DEAD, self.obs.pet.is_none())
    }

    /// SimC's `high_priority` + `cooldowns` list (12.1).
    fn cooldowns(&self) -> Option<SpellId> {
        let fs_talented = self.obs.has_buff(FESTERING_SCYTHE_TALENT);
        let fs_tt_up = self.obs.has_buff(FESTERING_SCYTHE_TT_BUFF);
        let bb_talented = self.obs.has_buff(BLIGHTBURST_TALENT);
        let dp_missing = self.obs.target_aura(DREAD_PLAGUE_DOT).is_none();
        let early = self.obs.combat_time.is_some_and(|t| t < SimDuration(5_000));
        let army_active = self
            .obs
            .guardians
            .get(&ARMY_GHOUL_PET)
            .is_some_and(|v| !v.is_empty());
        let blightfall_up = self.obs.has_buff(BLIGHTFALL_BUFF);
        [
            self.pick(
                SCOURGE_STRIKE,
                self.vs_up && self.ebq_rem > SimDuration::ZERO && self.ebq_rem < SimDuration(4_500),
            ),
            self.pick(
                OUTBREAK,
                (!bb_talented || self.cd(PUTREFY) > SimDuration(3_000) || early) && dp_missing,
            ),
            self.pick(
                DEATH_AND_DECAY,
                self.enemies >= 3
                    && self.obs.has_buff(CYCLE_OF_DEATH_TALENT)
                    && (fs_tt_up || !fs_talented),
            ),
            self.pick(ARMY_OF_THE_DEAD, fs_tt_up || !fs_talented),
            self.pick(
                DARK_TRANSFORMATION,
                (!blightfall_up
                    && (army_active
                        || self.cd(ARMY_OF_THE_DEAD) > SimDuration(30_000)
                        || !self.known(ARMY_OF_THE_DEAD)))
                    || (blightfall_up
                        && (self.obs.buff_remaining(DARK_TRANSFORMATION_BUFF)
                            <= SimDuration(3_000)
                            || self.cd(DARK_TRANSFORMATION) < SimDuration(3_000))),
            ),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `single_target` list (12.1).
    fn single_target(&self) -> Option<SpellId> {
        let fs_talented = self.obs.has_buff(FESTERING_SCYTHE_TALENT);
        let fs_up = self.obs.has_buff(FESTERING_SCYTHE_BUFF);
        let fs_rem = self.obs.buff_remaining(FESTERING_SCYTHE_BUFF);
        let fs_tt_rem = self.obs.buff_remaining(FESTERING_SCYTHE_TT_BUFF);
        let target_hp = self.obs.target.as_ref().map_or(100.0, |t| t.health_pct);
        [
            self.pick(
                FESTERING_STRIKE,
                fs_talented
                    && ((fs_up
                        && (fs_rem <= SimDuration(3_000) || fs_tt_rem < SimDuration(3_000)))
                        || (!fs_up && fs_tt_rem < SimDuration(3_000))),
            ),
            self.pick(
                SOUL_REAPER,
                (self.dt_up && self.cd(DARK_TRANSFORMATION) < SimDuration(38_000))
                    || target_hp < 35.0,
            ),
            self.pick(PUTREFY, self.dt_up && self.rp_deficit > 10.0),
            self.pick(
                SCOURGE_STRIKE,
                self.rp_deficit < 90.0 && self.ebq_stacks > 0 && self.ebq_stacks < self.ebq_max,
            ),
            self.pick(
                SCOURGE_STRIKE,
                self.ghoul_ready >= 1 && !self.obs.has_buff(AuraId(1241569)),
            ),
            self.pick(DEATH_COIL, self.sd_up || self.rp_deficit <= 10.0),
            self.pick(PUTREFY, self.dt_up),
            self.pick(
                DEATH_COIL,
                self.dt_up
                    || self.fk_up
                    || (self.ebq_rem > SimDuration::ZERO
                        && self.ebq_rem < SimDuration(5_000)
                        && !self.vs_up),
            ),
            self.pick(
                DEATH_COIL,
                self.cd(ARMY_OF_THE_DEAD) > SimDuration(5_000) || self.rp_deficit < 50.0,
            ),
            self.pick(SCOURGE_STRIKE, self.ghoul_ready >= 1),
            self.pick(FESTERING_STRIKE, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `aoe` list (12.1).
    fn aoe(&self) -> Option<SpellId> {
        let fs_talented = self.obs.has_buff(FESTERING_SCYTHE_TALENT);
        let fs_up = self.obs.has_buff(FESTERING_SCYTHE_BUFF);
        let fs_rem = self.obs.buff_remaining(FESTERING_SCYTHE_BUFF);
        let fs_tt_rem = self.obs.buff_remaining(FESTERING_SCYTHE_TT_BUFF);
        let target_hp = self.obs.target.as_ref().map_or(100.0, |t| t.health_pct);
        let epi_prio = (self.enemies >= 3 && !self.fk_up) || (self.enemies >= 4 && self.fk_up);
        let rp_dump = self.sd_up
            || self.rp_deficit < 15.0
            || (self.ebq_stacks > 0 && self.ebq_stacks >= self.ebq_max - 1)
            || (self.ebq_rem > SimDuration::ZERO
                && self.ebq_rem < SimDuration(7_000)
                && !self.vs_up)
            || self.runes < 2.0;
        [
            self.pick(
                FESTERING_STRIKE,
                fs_talented
                    && ((fs_up
                        && (fs_rem <= SimDuration(4_500) || fs_tt_rem < SimDuration(4_500)))
                        || (!fs_up && fs_tt_rem < SimDuration(4_500))),
            ),
            self.pick(PUTREFY, self.dt_up || self.obs.charges(PUTREFY) >= 2),
            self.pick(
                SOUL_REAPER,
                (self.dt_up && self.cd(DARK_TRANSFORMATION) < SimDuration(38_000))
                    || target_hp < 35.0,
            ),
            self.pick(EPIDEMIC, epi_prio && rp_dump),
            self.pick(FESTERING_STRIKE, self.ghoul_ready < 3),
            self.pick(SCOURGE_STRIKE, self.ghoul_ready >= 1),
            self.pick(DEATH_COIL, !epi_prio && rp_dump),
            self.pick(EPIDEMIC, epi_prio),
            self.pick(DEATH_COIL, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Unholy Death Knight priority list (Rider & San'layn).
pub fn unholy_dk(d: &Decision<SeatObs>) -> Choice {
    let u = Unholy::new(d);
    if let Some(s) = u.precombat() {
        return Choice::cast(s);
    }
    if let Some(move_in) = u.approach() {
        return move_in;
    }
    let rot = if u.enemies >= 3 {
        u.aoe()
    } else {
        u.single_target()
    };
    u.cooldowns()
        .or(rot)
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
