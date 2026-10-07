//! Hand-written policies over [`SeatObs`].
//!
//! A policy is a plain function from a decision to a choice. Run files
//! name one per seat; [`by_name`] resolves the name. To add one, write the
//! function and list it in [`SCRIPTS`].

use std::sync::Arc;

use portunus_core::{AuraId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, MoveGoal, Wait};
use portunus_env::Decision;
use portunus_gamedata::stats::ResourceKind;
use portunus_policy::Policy;

use crate::observe::{SeatObs, FOREVER};

type Script = fn(&Decision<SeatObs>) -> Choice;

/// Every named policy.
pub const SCRIPTS: &[(&str, Script)] = &[("elemental", elemental), ("idle", idle)];

pub fn by_name(name: &str) -> Option<Arc<dyn Policy<SeatObs>>> {
    SCRIPTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|&(_, f)| Arc::new(f) as Arc<dyn Policy<SeatObs>>)
}

/// Never acts.
pub fn idle(_: &Decision<SeatObs>) -> Choice {
    Choice::Wait(Wait::NextEvent)
}

const LIGHTNING_BOLT: SpellId = SpellId(188196);
const LAVA_BURST: SpellId = SpellId(51505);
const FLAME_SHOCK: SpellId = SpellId(188389);
const EARTH_SHOCK: SpellId = SpellId(8042);
const ELEMENTAL_BLAST: SpellId = SpellId(117014);
const FROST_SHOCK: SpellId = SpellId(196840);
const LIGHTNING_SHIELD: SpellId = SpellId(192106);
const THUNDERSTRIKE_WARD: SpellId = SpellId(462757);
const VOLTAIC_BLAZE: SpellId = SpellId(470057);
const ASCENDANCE: SpellId = SpellId(114050);
const STORMKEEPER: SpellId = SpellId(191634);
const ANCESTRAL_SWIFTNESS: SpellId = SpellId(443454);

const FLAME_SHOCK_DOT: AuraId = AuraId(188389);
const LIGHTNING_SHIELD_BUFF: AuraId = AuraId(192106);
const THUNDERSTRIKE_WARD_BUFF: AuraId = AuraId(462757);
const MASTER_OF_THE_ELEMENTS: AuraId = AuraId(260734);
const LAVA_SURGE: AuraId = AuraId(77762);
const STORMKEEPER_BUFF: AuraId = AuraId(191634);
const TEMPEST_BUFF: AuraId = AuraId(454015);
const ASCENDANCE_BUFF: AuraId = AuraId(1219480);
const FIRE_ELEMENTAL: AuraId = AuraId(188592);
const STORM_ELEMENTAL: AuraId = AuraId(157299);
const WIND_GUST: AuraId = AuraId(263806);
const CALL_OF_THE_ANCESTORS_BUFF: AuraId = AuraId(447244);

// Talents, read as the passive auras they grant.
const MASTER_OF_THE_ELEMENTS_TALENT: AuraId = AuraId(16166);
const EYE_OF_THE_STORM: AuraId = AuraId(381708);
const MOLTEN_WRATH: AuraId = AuraId(1258843);
const PURGING_FLAMES: AuraId = AuraId(1259471);
const CRACKLING_FURY: AuraId = AuraId(1269215);
const TEMPEST_TALENT: AuraId = AuraId(454009);
const CALL_OF_THE_ANCESTORS_TALENT: AuraId = AuraId(443450);

const SPIRITWALKERS_GRACE: SpellId = SpellId(79206);
const SPIRITWALKERS_GRACE_BUFF: AuraId = AuraId(79206);
const GUST_OF_WIND: SpellId = SpellId(192063);

/// Seconds kept between finishing a cast and having to start running.
const MOVE_MARGIN: f64 = 0.1;
/// The reach of the spec's damage spells, in yards.
const SPELL_RANGE: f64 = 40.0;

/// Flame Shock inside 30% of its 18 s duration.
const PANDEMIC: SimDuration = SimDuration(5400);
/// SimC's `gcd`, unhasted.
const GCD: SimDuration = SimDuration(1500);

fn secs(d: SimDuration) -> f64 {
    f64::from(d.millis()) / 1000.0
}

/// Dodging and keeping in range, ahead of the rotation: a cast in progress
/// finishes if there is still time to run afterwards, otherwise the seat
/// runs at once, with Gust of Wind when running alone can't make it.
/// Spiritwalker's Grace keeps casting through long forced movement, and an
/// idle seat walks back into range of its target.
fn movement(obs: &SeatObs, legal: &ActionMask) -> Option<Choice> {
    let graced = obs.has_buff(SPIRITWALKERS_GRACE_BUFF);
    if let Some(m) = obs.movement {
        if m.forced && m.remaining > SimDuration(3000) && !graced {
            return legal
                .is_ready(SPIRITWALKERS_GRACE)
                .then(|| Choice::cast(SPIRITWALKERS_GRACE));
        }
    }
    let slack = obs.move_slack();
    if slack.is_some_and(|s| s < 0.0) && legal.is_ready(GUST_OF_WIND) {
        return Some(Choice::cast(GUST_OF_WIND));
    }
    if !legal.can_move || obs.movement.is_some() {
        return None;
    }
    let casting = obs.cast_remaining.map_or(0.0, secs);
    if let Some(slack) = slack {
        if casting > 0.0 && !graced && casting + MOVE_MARGIN <= slack {
            return Some(Choice::Wait(Wait::NextEvent));
        }
        return Some(Choice::Move(MoveGoal::ClearDemands));
    }
    match &obs.target {
        Some(t) if casting == 0.0 && t.distance > SPELL_RANGE => {
            Some(Choice::Move(MoveGoal::Approach {
                target: t.actor,
                within: SPELL_RANGE,
            }))
        }
        _ => None,
    }
}

/// SimulationCraft's Elemental single-target list (12.0), without its
/// set-bonus, trinket, and two-target lines. A cooldown the build lacks
/// reads as never coming back, so an untalented Ascendance doesn't hold
/// Flame Shock or Stormkeeper.
pub fn elemental(d: &Decision<SeatObs>) -> Choice {
    let (obs, legal) = (&d.obs, &d.legal);
    if let Some(choice) = movement(obs, legal) {
        return choice;
    }
    let ready = |s: SpellId| legal.is_ready(s);
    let talent = |a: AuraId| obs.has_buff(a);
    let known = |s: SpellId| legal.abilities.contains_key(&s);
    let cd = |s: SpellId| {
        if known(s) {
            obs.cooldown_remaining(s)
        } else {
            FOREVER
        }
    };
    let pick = |s: SpellId, when: bool| (when && ready(s)).then_some(s);

    let mote = obs.has_buff(MASTER_OF_THE_ELEMENTS);
    let mote_talent = talent(MASTER_OF_THE_ELEMENTS_TALENT);
    let ascended = obs.has_buff(ASCENDANCE_BUFF);
    let refreshable = obs.target_aura_remaining(FLAME_SHOCK_DOT) < PANDEMIC;
    let asc_cd = cd(ASCENDANCE);
    let maelstrom = obs.resource(ResourceKind::Maelstrom);
    let deficit = obs.resource_deficit(ResourceKind::Maelstrom);
    let lvb_charges = obs.charges_fractional(LAVA_BURST);
    let elemental_blast = f64::from(u8::from(known(ELEMENTAL_BLAST)));
    let eye_of_the_storm = f64::from(u8::from(talent(EYE_OF_THE_STORM)));
    let gusts_stacked = !obs.has_buff(STORM_ELEMENTAL) || obs.buff_stacks(WIND_GUST) == 4;
    let empowered_lvb = talent(MOLTEN_WRATH) || talent(PURGING_FLAMES);

    let choice = [
        pick(LIGHTNING_SHIELD, !obs.has_buff(LIGHTNING_SHIELD_BUFF)),
        pick(THUNDERSTRIKE_WARD, !obs.has_buff(THUNDERSTRIKE_WARD_BUFF)),
        pick(STORMKEEPER, asc_cd > SimDuration(10_000) || asc_cd < GCD),
        pick(ANCESTRAL_SWIFTNESS, true),
        pick(
            FLAME_SHOCK,
            !mote && refreshable && asc_cd > SimDuration(5000),
        ),
        pick(
            FLAME_SHOCK,
            !mote
                && !ascended
                && obs.has_buff(FIRE_ELEMENTAL)
                && obs.buff_remaining(FIRE_ELEMENTAL) < SimDuration(2000),
        ),
        pick(
            VOLTAIC_BLAZE,
            !mote && refreshable && asc_cd > SimDuration(5000),
        ),
        pick(ASCENDANCE, cd(STORMKEEPER) > SimDuration(15_000)),
        pick(
            LAVA_BURST,
            mote_talent
                && !mote
                && deficit > 15.0
                && lvb_charges > 1.8
                && (gusts_stacked || talent(CALL_OF_THE_ANCESTORS_TALENT)),
        ),
        pick(
            LAVA_BURST,
            mote_talent
                && !mote
                && deficit > 15.0
                && maelstrom
                    > 52.0 - 5.0 * eye_of_the_storm * (1.0 + elemental_blast)
                        + 30.0 * elemental_blast,
        ),
        pick(
            LAVA_BURST,
            !mote_talent
                && deficit > 15.0
                && gusts_stacked
                && ((lvb_charges > 1.8 || !obs.has_buff(CALL_OF_THE_ANCESTORS_BUFF))
                    && empowered_lvb
                    || obs.has_buff(LAVA_SURGE)),
        ),
        // Tempest is cast as Lightning Bolt while its buff is up.
        pick(
            LIGHTNING_BOLT,
            obs.has_buff(TEMPEST_BUFF) && (mote || !mote_talent),
        ),
        pick(
            LIGHTNING_BOLT,
            obs.has_buff(STORMKEEPER_BUFF) && mote && talent(TEMPEST_TALENT),
        ),
        pick(ELEMENTAL_BLAST, mote || deficit < 15.0),
        pick(EARTH_SHOCK, mote || deficit < 15.0),
        pick(FLAME_SHOCK, mote && refreshable),
        pick(VOLTAIC_BLAZE, talent(CRACKLING_FURY) && !ascended),
        pick(LIGHTNING_BOLT, true),
        // Moving: Lightning Bolt isn't castable.
        pick(FLAME_SHOCK, true),
        pick(FROST_SHOCK, true),
    ]
    .into_iter()
    .flatten()
    .next();
    choice.map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
