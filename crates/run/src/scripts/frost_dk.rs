//! Frost Death Knight.
//!
//! [`frost_dk`] ports SimC's 12.1 Deathbringer and Rider of the Apocalypse
//! priority lists (`profiles/MID2/MID2_Death_Knight_Frost.simc`).

use portunus_core::{AuraId, HeroTreeId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, MoveGoal, Wait};
use portunus_env::Decision;
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::stats::ResourceKind;

use super::GCD;
use crate::observe::{SeatObs, FOREVER};

const FROST_STRIKE: SpellId = SpellId(49143);
const OBLITERATE: SpellId = SpellId(49020);
const HOWLING_BLAST: SpellId = SpellId(49184);
const FROSTSCYTHE: SpellId = SpellId(207230);
const GLACIAL_ADVANCE: SpellId = SpellId(194913);
const REMORSELESS_WINTER: SpellId = SpellId(196770);
const PILLAR_OF_FROST: SpellId = SpellId(51271);
const EMPOWER_RUNE_WEAPON: SpellId = SpellId(47568);
const FROSTWYRMS_FURY: SpellId = SpellId(279302);
const BREATH_OF_SINDRAGOSA: SpellId = SpellId(1249658);
const REAPERS_MARK: SpellId = SpellId(439843);
const RAISE_DEAD_GUARDIAN: SpellId = SpellId(46585);

const KILLING_MACHINE_BUFF: AuraId = AuraId(51124);
const RIME_BUFF: AuraId = AuraId(59052);
const PILLAR_OF_FROST_BUFF: AuraId = AuraId(51271);
const BREATH_OF_SINDRAGOSA_BUFF: AuraId = AuraId(1249658);
const REMORSELESS_WINTER_BUFF: AuraId = AuraId(196770);
const GATHERING_STORM_BUFF: AuraId = AuraId(211805);
const BONEGRINDER_FROST_BUFF: AuraId = AuraId(377103);
const CHOSEN_OF_FROSTBROOD_FWF_BUFF: AuraId = AuraId(1265639);
const FROSTBANE_BUFF: AuraId = AuraId(1229310);
const ICY_ONSLAUGHT_BUFF: AuraId = AuraId(1230273);
const UNHOLY_STRENGTH_BUFF: AuraId = AuraId(53365);

const FROST_FEVER_DOT: AuraId = AuraId(55095);
const RAZORICE_DEBUFF: AuraId = AuraId(51714);
const REAPERS_MARK_DEBUFF: AuraId = AuraId(434765);

const OBLITERATION_TALENT: AuraId = AuraId(281238);
const GATHERING_STORM_TALENT: AuraId = AuraId(194912);
const FROSTBOUND_WILL_TALENT: AuraId = AuraId(1238680);
const SHATTERING_BLADE_TALENT: AuraId = AuraId(207057);
const KILLING_STREAK_TALENT: AuraId = AuraId(1230153);
const ICY_ONSLAUGHT_TALENT: AuraId = AuraId(1230272);
const APOCALYPSE_NOW_TALENT: AuraId = AuraId(444040);
const CHOSEN_OF_FROSTBROOD_TALENT: AuraId = AuraId(1265632);

struct Frost<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    runes: f64,
    rp: f64,
    rp_deficit: f64,
    km: u8,
    rime: bool,
    pof_up: bool,
    bos_up: bool,
    deathbringer: bool,
    rune_pooling: bool,
    rp_pooling: bool,
}

impl<'a> Frost<'a> {
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
        let deathbringer = obs.hero_tree == Some(HeroTreeId(33));
        let rm_cd = if legal.abilities.contains_key(&REAPERS_MARK) {
            obs.cooldown_remaining(REAPERS_MARK)
        } else {
            FOREVER
        };
        let bos_cd = if legal.abilities.contains_key(&BREATH_OF_SINDRAGOSA) {
            obs.cooldown_remaining(BREATH_OF_SINDRAGOSA)
        } else {
            FOREVER
        };
        let io_up = f64::from(u8::from(obs.has_buff(ICY_ONSLAUGHT_BUFF)));
        let rune_pooling = deathbringer && rm_cd < SimDuration(6_000) && runes < 3.0;
        let rp_pooling = legal.abilities.contains_key(&BREATH_OF_SINDRAGOSA)
            && bos_cd < SimDuration(6_000)
            && rp < 60.0 + 35.0 + 5.0 * io_up;
        Self {
            obs,
            legal,
            enemies,
            runes,
            rp,
            rp_deficit: (rp_max - rp).max(0.0),
            km: obs.buff_stacks(KILLING_MACHINE_BUFF),
            rime: obs.has_buff(RIME_BUFF),
            pof_up: obs.has_buff(PILLAR_OF_FROST_BUFF),
            bos_up: obs.has_buff(BREATH_OF_SINDRAGOSA_BUFF),
            deathbringer,
            rune_pooling,
            rp_pooling,
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

    /// SimC's `cooldowns` list (12.1).
    fn cooldowns(&self) -> Option<SpellId> {
        let bos_talented = self.known(BREATH_OF_SINDRAGOSA);
        let bos_cd = self.cd(BREATH_OF_SINDRAGOSA);
        let bos_check = !bos_talented
            || bos_cd > SimDuration(20_000)
            || (bos_cd <= GCD && self.rp >= if self.deathbringer { 40.0 } else { 60.0 });
        let pof_rem = self.obs.buff_remaining(PILLAR_OF_FROST_BUFF);
        let us_rem = self.obs.buff_remaining(UNHOLY_STRENGTH_BUFF);
        let bg_rem = self.obs.buff_remaining(BONEGRINDER_FROST_BUFF);
        let razorice_5 = self
            .obs
            .target_aura(RAZORICE_DEBUFF)
            .is_some_and(|a| a.stacks >= 5);
        let fwf_buffs = ((pof_rem > SimDuration::ZERO && pof_rem <= GCD)
            || (us_rem > SimDuration::ZERO && us_rem <= GCD)
            || (bg_rem > SimDuration::ZERO && bg_rem <= GCD))
            && (self.enemies > 1 || razorice_5 || self.obs.has_buff(SHATTERING_BLADE_TALENT));
        let apoc_or_chosen = self.obs.has_buff(APOCALYPSE_NOW_TALENT)
            || self.obs.has_buff(CHOSEN_OF_FROSTBROOD_TALENT);
        let fwf_recall_up = self.obs.has_buff(CHOSEN_OF_FROSTBROOD_FWF_BUFF);
        let io_stacks = f64::from(self.obs.buff_stacks(ICY_ONSLAUGHT_BUFF));
        let io_talented = f64::from(u8::from(self.obs.has_buff(ICY_ONSLAUGHT_TALENT)));
        let ks_talented = u8::from(self.obs.has_buff(KILLING_STREAK_TALENT));
        [
            self.pick(
                REMORSELESS_WINTER,
                self.enemies > 1
                    || self.obs.has_buff(GATHERING_STORM_TALENT)
                    || (self.obs.buff_stacks(GATHERING_STORM_BUFF) >= 10
                        && self.obs.buff_remaining(REMORSELESS_WINTER_BUFF) <= GCD),
            ),
            self.pick(
                PILLAR_OF_FROST,
                !bos_talented && (!self.deathbringer || self.runes >= 2.0),
            ),
            self.pick(
                PILLAR_OF_FROST,
                bos_talented && bos_check && (!self.deathbringer || self.runes >= 2.0),
            ),
            self.pick(BREATH_OF_SINDRAGOSA, !self.bos_up && self.pof_up),
            self.pick(
                REAPERS_MARK,
                self.obs.target_aura(REAPERS_MARK_DEBUFF).is_none()
                    && (self.pof_up || self.cd(PILLAR_OF_FROST) > SimDuration(5_000)),
            ),
            self.pick(
                FROSTWYRMS_FURY,
                apoc_or_chosen
                    && !fwf_recall_up
                    && self.pof_up
                    && self.obs.has_buff(BONEGRINDER_FROST_BUFF),
            ),
            self.pick(FROSTWYRMS_FURY, fwf_recall_up && fwf_buffs),
            self.pick(FROSTWYRMS_FURY, !apoc_or_chosen && self.pof_up && fwf_buffs),
            self.pick(RAISE_DEAD_GUARDIAN, true),
            self.pick(
                EMPOWER_RUNE_WEAPON,
                (self.runes < 2.0 || self.km == 0)
                    && self.rp < 35.0 + io_talented * io_stacks * 5.0,
            ),
            self.pick(
                EMPOWER_RUNE_WEAPON,
                self.obs.charges(EMPOWER_RUNE_WEAPON) >= 2 && self.km < 2 - ks_talented,
            ),
            self.pick(
                EMPOWER_RUNE_WEAPON,
                self.obs.has_buff(OBLITERATION_TALENT)
                    && pof_rem > SimDuration(6_000)
                    && self.runes <= 2.0
                    && self.km == 1,
            ),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `single_target` list (12.1).
    fn single_target(&self) -> Option<SpellId> {
        let sb = self.obs.has_buff(SHATTERING_BLADE_TALENT);
        let razorice_5 = self
            .obs
            .target_aura(RAZORICE_DEBUFF)
            .is_some_and(|a| a.stacks >= 5);
        let oblit_window = self.obs.has_buff(OBLITERATION_TALENT) && self.pof_up;
        [
            self.pick(
                OBLITERATE,
                self.km >= 2 || (self.km >= 1 && self.runes >= 3.0),
            ),
            self.pick(
                HOWLING_BLAST,
                self.rime && self.obs.has_buff(FROSTBOUND_WILL_TALENT),
            ),
            self.pick(FROST_STRIKE, razorice_5 && sb && !self.rp_pooling),
            self.pick(HOWLING_BLAST, self.rime),
            self.pick(
                FROST_STRIKE,
                !sb && !self.rp_pooling && self.rp_deficit < 30.0,
            ),
            self.pick(OBLITERATE, self.km >= 1 && !self.rune_pooling),
            self.pick(FROST_STRIKE, !self.rp_pooling),
            self.pick(OBLITERATE, !self.rune_pooling && !oblit_window),
            self.pick(HOWLING_BLAST, self.km == 0 && oblit_window),
            self.pick(
                HOWLING_BLAST,
                self.obs.target_aura(FROST_FEVER_DOT).is_none(),
            ),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `aoe` list (12.1).
    fn aoe(&self) -> Option<SpellId> {
        let sb = self.obs.has_buff(SHATTERING_BLADE_TALENT);
        let razorice_5 = self
            .obs
            .target_aura(RAZORICE_DEBUFF)
            .is_some_and(|a| a.stacks >= 5);
        let oblit_window = self.obs.has_buff(OBLITERATION_TALENT) && self.pof_up;
        let fsc_prio = self.enemies >= 3;
        [
            self.pick(FROSTSCYTHE, self.km >= 2 && fsc_prio),
            self.pick(
                FROST_STRIKE,
                razorice_5 && self.obs.has_buff(FROSTBANE_BUFF),
            ),
            self.pick(FROSTSCYTHE, self.km >= 1 && self.runes >= 3.0 && fsc_prio),
            self.pick(
                OBLITERATE,
                self.km >= 2 || (self.km >= 1 && self.runes >= 3.0),
            ),
            self.pick(
                HOWLING_BLAST,
                (self.rime && self.obs.has_buff(FROSTBOUND_WILL_TALENT))
                    || self.obs.target_aura(FROST_FEVER_DOT).is_none(),
            ),
            self.pick(
                FROST_STRIKE,
                razorice_5 && sb && self.enemies < 5 && !self.rp_pooling,
            ),
            self.pick(FROSTSCYTHE, self.km >= 1 && !self.rune_pooling && fsc_prio),
            self.pick(OBLITERATE, self.km >= 1 && !self.rune_pooling),
            self.pick(HOWLING_BLAST, self.rime),
            self.pick(GLACIAL_ADVANCE, !self.rp_pooling && self.enemies >= 3),
            self.pick(FROST_STRIKE, !self.rp_pooling),
            self.pick(FROSTSCYTHE, !self.rune_pooling && !oblit_window && fsc_prio),
            self.pick(OBLITERATE, !self.rune_pooling && !oblit_window),
            self.pick(HOWLING_BLAST, self.km == 0 && oblit_window),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Frost Death Knight priority list (Deathbringer & Rider).
pub fn frost_dk(d: &Decision<SeatObs>) -> Choice {
    let f = Frost::new(d);
    if let Some(move_in) = f.approach() {
        return move_in;
    }
    let rot = if f.enemies >= 2 {
        f.aoe()
    } else {
        f.single_target()
    };
    f.cooldowns()
        .or(rot)
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
