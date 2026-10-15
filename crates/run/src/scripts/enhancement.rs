//! Enhancement Shaman.
//!
//! [`enhancement`] ports SimC's 12.1 Stormbringer and Totemic priority lists
//! (`profiles/MID2/MID2_Shaman_Enhancement.simc`).

use portunus_core::{AuraId, HeroTreeId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, MoveGoal, Wait};
use portunus_env::Decision;
use portunus_gamedata::spec::MELEE_RANGE;

use super::GCD;
use crate::observe::{SeatObs, FOREVER};

const STORMSTRIKE: SpellId = SpellId(17364);
const LAVA_LASH: SpellId = SpellId(60103);
const LIGHTNING_BOLT: SpellId = SpellId(188196);
const CHAIN_LIGHTNING: SpellId = SpellId(188443);
const FLAME_SHOCK: SpellId = SpellId(188389);
const VOLTAIC_BLAZE: SpellId = SpellId(470057);
const CRASH_LIGHTNING: SpellId = SpellId(187874);
const SUNDERING: SpellId = SpellId(197214);
const DOOM_WINDS: SpellId = SpellId(384352);
const ASCENDANCE: SpellId = SpellId(114051);
const SURGING_TOTEM: SpellId = SpellId(444995);
const LIGHTNING_SHIELD: SpellId = SpellId(192106);
const WINDFURY_WEAPON: SpellId = SpellId(33757);
const FLAMETONGUE_WEAPON: SpellId = SpellId(318038);
const BLOODLUST: SpellId = SpellId(2825);

const MAELSTROM_WEAPON: AuraId = AuraId(344179);
const FLAME_SHOCK_DOT: AuraId = AuraId(188389);
const LASHING_FLAMES_DEBUFF: AuraId = AuraId(334168);
const LASHING_FLAMES_TALENT: AuraId = AuraId(334046);
const CRASH_LIGHTNING_BUFF: AuraId = AuraId(187878);
const STORM_UNLEASHED_BUFF: AuraId = AuraId(1262830);
const DOOM_WINDS_BUFF: AuraId = AuraId(466772);
const ASCENDANCE_BUFF: AuraId = AuraId(114051);
const TEMPEST_BUFF: AuraId = AuraId(454015);
const PRIMORDIAL_STORM_BUFF: AuraId = AuraId(1218125);
const HOT_HAND_BUFF: AuraId = AuraId(215785);
const CONVERGING_STORMS_BUFF: AuraId = AuraId(198300);
const THORIMS_INVOCATION: AuraId = AuraId(384444);
const ELEMENTAL_TEMPO: AuraId = AuraId(1250364);
const SURGING_ELEMENTS: AuraId = AuraId(382042);
const FERAL_SPIRIT: AuraId = AuraId(469314);
const FIRE_NOVA: AuraId = AuraId(1260666);
const SPLITSTREAM: AuraId = AuraId(445035);
const WHIRLING_AIR: AuraId = AuraId(453409);
const WHIRLING_FIRE: AuraId = AuraId(453405);
const WHIRLING_EARTH: AuraId = AuraId(453406);
const LIVELY_TOTEMS_BUFF: AuraId = AuraId(461242);
const LIGHTNING_SHIELD_BUFF: AuraId = AuraId(192106);
const WINDFURY_WEAPON_BUFF: AuraId = AuraId(319773);
const FLAMETONGUE_WEAPON_BUFF: AuraId = AuraId(319778);
const SATED: AuraId = AuraId(57724);

struct Enh<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    mw: u8,
    fs_rem: SimDuration,
    totemic: bool,
    ascendance_up: bool,
    doom_winds_up: bool,
    ti: bool,
}

impl<'a> Enh<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        Self {
            obs,
            legal,
            enemies: obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len()),
            mw: obs.buff_stacks(MAELSTROM_WEAPON),
            fs_rem: obs.target_aura_remaining(FLAME_SHOCK_DOT),
            totemic: obs.hero_tree == Some(HeroTreeId(56)),
            ascendance_up: obs.has_buff(ASCENDANCE_BUFF),
            doom_winds_up: obs.has_buff(DOOM_WINDS_BUFF),
            ti: obs.has_buff(THORIMS_INVOCATION),
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
        self.pick(WINDFURY_WEAPON, !self.obs.has_buff(WINDFURY_WEAPON_BUFF))
            .or_else(|| {
                self.pick(
                    FLAMETONGUE_WEAPON,
                    !self.obs.has_buff(FLAMETONGUE_WEAPON_BUFF),
                )
            })
            .or_else(|| self.pick(LIGHTNING_SHIELD, !self.obs.has_buff(LIGHTNING_SHIELD_BUFF)))
            .or_else(|| {
                self.pick(
                    BLOODLUST,
                    self.obs.combat_time.is_some() && !self.obs.has_buff(SATED),
                )
            })
    }
}

/// SimC's `single_sb` list (12.1).
fn single_sb(e: &Enh<'_>) -> Option<SpellId> {
    let early = e.obs.combat_time.is_some_and(|t| t < SimDuration(5_000));
    let very_early = e.obs.combat_time.is_some_and(|t| t < SimDuration(1_000));
    let ps_up = e.obs.has_buff(PRIMORDIAL_STORM_BUFF);
    let ps_rem = e.obs.buff_remaining(PRIMORDIAL_STORM_BUFF);
    let tempest_up = e.obs.has_buff(TEMPEST_BUFF);
    let fire_nova = u8::from(e.obs.has_buff(FIRE_NOVA));
    [
        e.pick(
            SUNDERING,
            ps_up && (e.mw >= 9 || ps_rem <= SimDuration(4_000) && e.mw >= 5),
        ),
        e.pick(VOLTAIC_BLAZE, e.fs_rem == SimDuration::ZERO && early),
        e.pick(FLAME_SHOCK, e.fs_rem == SimDuration::ZERO),
        e.pick(
            LAVA_LASH,
            e.obs.target_aura(LASHING_FLAMES_DEBUFF).is_none() && early,
        ),
        e.pick(STORMSTRIKE, very_early),
        e.pick(
            SUNDERING,
            !ps_up && (e.obs.has_buff(SURGING_ELEMENTS) || e.obs.has_buff(FERAL_SPIRIT)),
        ),
        e.pick(DOOM_WINDS, true),
        e.pick(
            CRASH_LIGHTNING,
            !e.obs.has_buff(CRASH_LIGHTNING_BUFF) || e.obs.has_buff(STORM_UNLEASHED_BUFF),
        ),
        e.pick(
            VOLTAIC_BLAZE,
            e.doom_winds_up && e.mw >= 10 - (1 + 2 * fire_nova) && e.mw != 10 && e.ti,
        ),
        e.pick(STORMSTRIKE, e.ascendance_up && e.mw > 0 && e.ti),
        e.pick(ASCENDANCE, true),
        e.pick(STORMSTRIKE, e.doom_winds_up && e.ti),
        e.pick(CRASH_LIGHTNING, e.doom_winds_up && e.ti),
        e.pick(LIGHTNING_BOLT, tempest_up && e.mw >= 10),
        e.pick(LIGHTNING_BOLT, e.mw >= 10),
        e.pick(STORMSTRIKE, e.obs.charges(STORMSTRIKE) >= 2),
        e.pick(LAVA_LASH, e.obs.has_buff(LASHING_FLAMES_TALENT)),
        e.pick(STORMSTRIKE, true),
        e.pick(VOLTAIC_BLAZE, true),
        e.pick(SUNDERING, !ps_up),
        e.pick(LAVA_LASH, true),
        e.pick(LIGHTNING_BOLT, e.mw >= 8),
        e.pick(CRASH_LIGHTNING, true),
        e.pick(LIGHTNING_BOLT, e.mw >= 5),
        e.pick(FLAME_SHOCK, true),
    ]
    .into_iter()
    .flatten()
    .next()
}

/// SimC's `single_totemic` list (12.1).
fn single_totemic(e: &Enh<'_>) -> Option<SpellId> {
    let ps_up = e.obs.has_buff(PRIMORDIAL_STORM_BUFF);
    let ps_rem = e.obs.buff_remaining(PRIMORDIAL_STORM_BUFF);
    let ll_cd = e.cd(LAVA_LASH);
    let mw_window = SimDuration(u32::from(e.mw) * 300);
    [
        e.pick(VOLTAIC_BLAZE, e.fs_rem == SimDuration::ZERO),
        e.pick(FLAME_SHOCK, e.fs_rem == SimDuration::ZERO),
        e.pick(SURGING_TOTEM, true),
        e.pick(
            SUNDERING,
            !ps_up
                && (e.obs.has_buff(SURGING_ELEMENTS)
                    || e.obs.has_buff(WHIRLING_EARTH)
                    || e.obs.has_buff(FERAL_SPIRIT)),
        ),
        e.pick(
            LAVA_LASH,
            e.obs.has_buff(WHIRLING_FIRE) || e.obs.has_buff(HOT_HAND_BUFF),
        ),
        e.pick(DOOM_WINDS, true),
        e.pick(
            CRASH_LIGHTNING,
            !e.obs.has_buff(CRASH_LIGHTNING_BUFF) || e.obs.has_buff(STORM_UNLEASHED_BUFF),
        ),
        e.pick(
            SUNDERING,
            ps_up && (e.mw >= 10 || ps_rem < SimDuration(3_500) && e.mw >= 5),
        ),
        e.pick(STORMSTRIKE, e.ti && e.ascendance_up),
        e.pick(ASCENDANCE, true),
        e.pick(CRASH_LIGHTNING, e.ti && e.doom_winds_up || e.ascendance_up),
        e.pick(STORMSTRIKE, e.ti && e.doom_winds_up),
        e.pick(
            LIGHTNING_BOLT,
            e.obs.has_buff(ELEMENTAL_TEMPO)
                && (e.mw >= 5 && ll_cd > GCD && ll_cd <= mw_window || e.mw >= 10),
        ),
        e.pick(CRASH_LIGHTNING, !e.obs.has_buff(CRASH_LIGHTNING_BUFF)),
        e.pick(LAVA_LASH, true),
        e.pick(
            SUNDERING,
            !ps_up && e.cd(SURGING_TOTEM) > SimDuration(25_000),
        ),
        e.pick(STORMSTRIKE, true),
        e.pick(VOLTAIC_BLAZE, true),
        e.pick(CRASH_LIGHTNING, true),
        e.pick(LIGHTNING_BOLT, e.mw >= 5),
        e.pick(FLAME_SHOCK, true),
    ]
    .into_iter()
    .flatten()
    .next()
}

/// SimC's `aoe` list (12.1).
fn aoe(e: &Enh<'_>) -> Option<SpellId> {
    let ps_up = e.obs.has_buff(PRIMORDIAL_STORM_BUFF);
    let tempest_up = e.obs.has_buff(TEMPEST_BUFF);
    let split_hh = e.obs.has_buff(SPLITSTREAM) && e.obs.has_buff(HOT_HAND_BUFF);
    let cl_cap = 9 + u8::from(e.totemic);
    [
        e.pick(VOLTAIC_BLAZE, e.totemic && e.fs_rem == SimDuration::ZERO),
        e.pick(FLAME_SHOCK, e.fs_rem == SimDuration::ZERO),
        e.pick(SURGING_TOTEM, true),
        e.pick(ASCENDANCE, true),
        e.pick(
            SUNDERING,
            !ps_up && (e.obs.has_buff(SURGING_ELEMENTS) || e.obs.has_buff(WHIRLING_EARTH)),
        ),
        e.pick(LAVA_LASH, e.obs.has_buff(WHIRLING_FIRE)),
        e.pick(DOOM_WINDS, true),
        e.pick(
            CRASH_LIGHTNING,
            e.ti && e.obs.has_buff(WHIRLING_AIR) && (e.doom_winds_up || e.ascendance_up),
        ),
        e.pick(
            STORMSTRIKE,
            e.ti && e.obs.has_buff(WHIRLING_AIR) && (e.ascendance_up || e.doom_winds_up),
        ),
        e.pick(LAVA_LASH, split_hh),
        e.pick(
            LIGHTNING_BOLT,
            tempest_up && e.mw >= 10 && (!e.ascendance_up || !e.doom_winds_up),
        ),
        e.pick(SUNDERING, ps_up && e.mw >= 10),
        e.pick(
            CRASH_LIGHTNING,
            e.ti && (e.doom_winds_up || e.ascendance_up) && split_hh,
        ),
        e.pick(
            STORMSTRIKE,
            e.ti && split_hh && (e.ascendance_up || e.doom_winds_up),
        ),
        e.pick(CHAIN_LIGHTNING, e.mw >= cl_cap && split_hh),
        e.pick(VOLTAIC_BLAZE, e.obs.has_buff(FIRE_NOVA)),
        e.pick(CRASH_LIGHTNING, true),
        e.pick(STORMSTRIKE, e.ti && (e.ascendance_up || e.doom_winds_up)),
        e.pick(CHAIN_LIGHTNING, e.mw >= cl_cap),
        e.pick(SUNDERING, !ps_up && e.obs.has_buff(FERAL_SPIRIT)),
        e.pick(VOLTAIC_BLAZE, true),
        e.pick(LAVA_LASH, e.obs.has_buff(LIVELY_TOTEMS_BUFF)),
        e.pick(
            STORMSTRIKE,
            e.ascendance_up
                || e.obs.charges(STORMSTRIKE) >= 2
                || e.obs.buff_stacks(CONVERGING_STORMS_BUFF) >= 6,
        ),
        e.pick(
            SUNDERING,
            !ps_up && e.cd(SURGING_TOTEM) > SimDuration(25_000),
        ),
        e.pick(STORMSTRIKE, !e.totemic),
        e.pick(LAVA_LASH, true),
        e.pick(STORMSTRIKE, true),
        e.pick(CHAIN_LIGHTNING, e.mw >= 5),
        e.pick(FLAME_SHOCK, true),
    ]
    .into_iter()
    .flatten()
    .next()
}

/// SimulationCraft's 12.1 Enhancement Shaman priority list (Stormbringer and Totemic).
pub fn enhancement(d: &Decision<SeatObs>) -> Choice {
    let e = Enh::new(d);
    if let Some(s) = e.precombat() {
        return Choice::cast(s);
    }
    if let Some(m) = e.approach() {
        return m;
    }
    let list = if e.enemies > 1 {
        aoe(&e)
    } else if e.totemic {
        single_totemic(&e)
    } else {
        single_sb(&e)
    };
    list.map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
