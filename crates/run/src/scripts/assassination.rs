//! Assassination Rogue.
//!
//! [`assassination`] ports SimC's 12.1 Deathstalker and Fatebound priority
//! lists (`profiles/MID2/MID2_Rogue_Assassination.simc`).

use portunus_core::{AuraId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, MoveGoal, Wait};
use portunus_env::Decision;
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::stats::ResourceKind;

use crate::observe::{SeatObs, FOREVER};

const MUTILATE: SpellId = SpellId(1329);
const AMBUSH: SpellId = SpellId(8676);
const GARROTE: SpellId = SpellId(703);
const RUPTURE: SpellId = SpellId(1943);
const ENVENOM: SpellId = SpellId(32645);
const FAN_OF_KNIVES: SpellId = SpellId(51723);
const CRIMSON_TEMPEST: SpellId = SpellId(1247227);
const DEATHMARK: SpellId = SpellId(360194);
const KINGSBANE: SpellId = SpellId(385627);
const SHIV: SpellId = SpellId(5938);
const THISTLE_TEA: SpellId = SpellId(381623);
const VANISH: SpellId = SpellId(1856);
const STEALTH: SpellId = SpellId(1784);

const STEALTH_BUFF: AuraId = AuraId(1784);
const VANISH_BUFF: AuraId = AuraId(11327);
const IMPROVED_GARROTE_BUFF: AuraId = AuraId(392403);
const ENVENOM_BUFF: AuraId = AuraId(32645);
const DEATHMARK_CASTER_BUFF: AuraId = AuraId(360195);
const DARKEST_NIGHT_BUFF: AuraId = AuraId(457280);

const GARROTE_DOT: AuraId = AuraId(703);
const RUPTURE_DOT: AuraId = AuraId(1943);
const DEATHMARK_DOT: AuraId = AuraId(360194);

const IMPROVED_GARROTE_TALENT: AuraId = AuraId(381632);

struct Assa<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    energy: f64,
    cp: f64,
    cp_deficit: f64,
    darkest_night: bool,
    stealthed: bool,
}

impl<'a> Assa<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let enemies = obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len());
        let energy = obs.resource(ResourceKind::Energy);
        let cp = obs.resource(ResourceKind::ComboPoints);
        let cp_max = obs
            .resource_max
            .get(&ResourceKind::ComboPoints)
            .copied()
            .unwrap_or(7.0);
        let stealthed = obs.has_buff(STEALTH_BUFF) || obs.has_buff(VANISH_BUFF);
        Self {
            obs,
            legal,
            enemies,
            energy,
            cp,
            cp_deficit: (cp_max - cp).max(0.0),
            darkest_night: obs.has_buff(DARKEST_NIGHT_BUFF),
            stealthed,
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

    fn dot_rem(&self, dot: AuraId) -> SimDuration {
        self.obs
            .target_aura(dot)
            .and_then(|a| a.remaining)
            .unwrap_or(SimDuration::ZERO)
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

    /// SimC's `cds` list.
    fn cds(&self) -> Option<SpellId> {
        let garrote_up = self.dot_rem(GARROTE_DOT) > SimDuration::ZERO;
        let rupture_up = self.dot_rem(RUPTURE_DOT) > SimDuration::ZERO;
        let dm_up = self.dot_rem(DEATHMARK_DOT) > SimDuration::ZERO
            || self.obs.has_buff(DEATHMARK_CASTER_BUFF);
        [
            self.pick(STEALTH, true),
            self.pick(THISTLE_TEA, self.energy < 45.0),
            self.pick(
                DEATHMARK,
                garrote_up && rupture_up && self.cd(KINGSBANE) <= SimDuration(2_000),
            ),
            self.pick(
                KINGSBANE,
                garrote_up && rupture_up && (dm_up || self.cd(DEATHMARK) > SimDuration(52_000)),
            ),
            self.pick(
                SHIV,
                dm_up || self.dot_rem(AuraId(385627)) > SimDuration::ZERO,
            ),
            self.pick(
                VANISH,
                !self.stealthed
                    && self.obs.has_buff(IMPROVED_GARROTE_TALENT)
                    && !self.obs.has_buff(IMPROVED_GARROTE_BUFF)
                    && dm_up,
            ),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `core_dot` list.
    fn core_dot(&self) -> Option<SpellId> {
        let garrote_rem = self.dot_rem(GARROTE_DOT);
        let rupture_rem = self.dot_rem(RUPTURE_DOT);
        let imp_garrote = self.obs.has_buff(IMPROVED_GARROTE_BUFF) || self.stealthed;
        [
            self.pick(GARROTE, imp_garrote && garrote_rem <= SimDuration(18_000)),
            self.pick(
                GARROTE,
                self.cp_deficit >= 1.0 && garrote_rem <= SimDuration(5_400),
            ),
            self.pick(
                RUPTURE,
                self.cp >= 4.0
                    && rupture_rem <= SimDuration(7_200)
                    && (!self.darkest_night || rupture_rem == SimDuration::ZERO),
            ),
        ]
        .into_iter()
        .flatten()
        .next()
    }

    /// SimC's `spend` list.
    fn spend(&self) -> Option<SpellId> {
        let env_rem = self.obs.buff_remaining(ENVENOM_BUFF);
        let dm_up = self.obs.has_buff(DEATHMARK_CASTER_BUFF);
        self.pick(
            ENVENOM,
            self.darkest_night
                || env_rem <= SimDuration(1_500)
                || dm_up
                || self.energy >= 70.0
                || self.cp >= 5.0,
        )
    }

    /// SimC's `generate` list.
    fn generate(&self) -> Option<SpellId> {
        [
            self.pick(CRIMSON_TEMPEST, self.enemies > 1),
            self.pick(
                FAN_OF_KNIVES,
                (self.darkest_night && self.cp_deficit <= 1.0) || self.enemies > 1,
            ),
            self.pick(AMBUSH, true),
            self.pick(MUTILATE, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Assassination Rogue priority list (Deathstalker & Fatebound).
pub fn assassination(d: &Decision<SeatObs>) -> Choice {
    let a = Assa::new(d);
    if let Some(move_in) = a.approach() {
        return move_in;
    }
    let finish_ready =
        (!a.darkest_night && a.cp >= 5.0) || (a.darkest_night && a.cp_deficit == 0.0);
    let deathmark = a.dot_rem(DEATHMARK_DOT) > SimDuration::ZERO;
    let act = super::use_buff_trinket(d, deathmark)
        .or_else(|| a.cds())
        .or_else(|| a.core_dot())
        .or_else(|| {
            if finish_ready {
                a.spend()
            } else {
                a.generate()
            }
        });
    act.map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
