//! Windwalker Monk.
//!
//! [`windwalker`] ports SimC's 12.1 Conduit of the Celestials and Shado-Pan
//! priority lists (`profiles/MID2/MID2_Monk_Windwalker.simc`).

use portunus_core::{AuraId, SpellId};
use portunus_engine::{ActionMask, Choice, MoveGoal, Wait};
use portunus_env::Decision;
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::stats::ResourceKind;

use crate::observe::SeatObs;

const TIGER_PALM: SpellId = SpellId(100780);
const BLACKOUT_KICK: SpellId = SpellId(100784);
const RISING_SUN_KICK: SpellId = SpellId(107428);
const RUSHING_WIND_KICK: SpellId = SpellId(1250566);
const SPINNING_CRANE_KICK: SpellId = SpellId(101546);
const FISTS_OF_FURY: SpellId = SpellId(113656);
const STRIKE_OF_THE_WINDLORD: SpellId = SpellId(392983);
const WHIRLING_DRAGON_PUNCH: SpellId = SpellId(152175);
const ZENITH: SpellId = SpellId(1249625);
const ZENITH_STOMP: SpellId = SpellId(1249626);
const SLICING_WINDS: SpellId = SpellId(1217413);
const TOUCH_OF_DEATH: SpellId = SpellId(322109);
const INVOKE_XUEN: SpellId = SpellId(123904);
const CELESTIAL_CONDUIT: SpellId = SpellId(443028);

const COMBO_BREAKER_BUFF: AuraId = AuraId(116768);
const DANCE_OF_CHIJI_BUFF: AuraId = AuraId(325202);
const UNBROKEN_RHYTHM_BUFF: AuraId = AuraId(1250567);

struct Ww<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    chi: f64,
    last: Option<SpellId>,
}

impl<'a> Ww<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let chi = obs.resource(ResourceKind::Chi);
        let last = obs.recent_casts.first().copied();
        Self {
            obs,
            legal,
            chi,
            last,
        }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
    }

    /// True if `s` is a Combo Strike (differs from the previous cast).
    fn combo(&self, s: SpellId, when: bool) -> Option<SpellId> {
        self.pick(s, when && self.last != Some(s))
    }

    fn rotation(&self) -> Option<SpellId> {
        let cb = self.obs.has_buff(COMBO_BREAKER_BUFF);
        let chiji = self.obs.has_buff(DANCE_OF_CHIJI_BUFF);
        let unbroken = self.obs.has_buff(UNBROKEN_RHYTHM_BUFF);
        [
            self.pick(INVOKE_XUEN, true),
            self.pick(ZENITH, true),
            self.pick(TOUCH_OF_DEATH, true),
            self.combo(ZENITH_STOMP, self.chi <= 4.0),
            self.pick(CELESTIAL_CONDUIT, true),
            self.pick(WHIRLING_DRAGON_PUNCH, true),
            self.pick(STRIKE_OF_THE_WINDLORD, true),
            self.pick(FISTS_OF_FURY, true),
            self.combo(RUSHING_WIND_KICK, true),
            self.combo(RISING_SUN_KICK, true),
            self.combo(SPINNING_CRANE_KICK, chiji || unbroken),
            self.combo(BLACKOUT_KICK, cb || self.chi >= 3.0),
            self.pick(SLICING_WINDS, true),
            self.combo(TIGER_PALM, self.chi <= 4.0),
            self.combo(BLACKOUT_KICK, true),
            self.combo(SPINNING_CRANE_KICK, self.chi >= 2.0),
            self.pick(TIGER_PALM, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Windwalker Monk priority list (Conduit of the Celestials & Shado-Pan).
pub fn windwalker(d: &Decision<SeatObs>) -> Choice {
    if let Some(t) = &d.obs.target {
        if t.distance > MELEE_RANGE && d.obs.movement.is_none() && d.legal.can_move {
            return Choice::Move(MoveGoal::Approach {
                target: t.actor,
                within: MELEE_RANGE,
            });
        }
    }
    Ww::new(d)
        .rotation()
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
