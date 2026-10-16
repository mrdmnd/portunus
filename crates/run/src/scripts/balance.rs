//! Balance Druid.
//!
//! [`balance`] ports SimC's 12.1 Keeper of the Grove and Elune's Chosen
//! priority lists (`profiles/MID1/MID1_Druid_Balance.simc`).

use portunus_core::{AuraId, HeroTreeId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, Wait};
use portunus_env::Decision;
use portunus_gamedata::stats::ResourceKind;

use crate::observe::SeatObs;

const WRATH: SpellId = SpellId(190984);
const STARFIRE: SpellId = SpellId(194153);
const MOONFIRE: SpellId = SpellId(8921);
const SUNFIRE: SpellId = SpellId(93402);
const STARSURGE: SpellId = SpellId(78674);
const STARFALL: SpellId = SpellId(191034);
const ECLIPSE: SpellId = SpellId(1239669);
const FORCE_OF_NATURE: SpellId = SpellId(205636);
const CELESTIAL_ALIGNMENT: SpellId = SpellId(194223);
const WILD_MUSHROOM: SpellId = SpellId(88747);
const FURY_OF_ELUNE: SpellId = SpellId(202770);
const NEW_MOON: SpellId = SpellId(274281);
const CONVOKE_THE_SPIRITS: SpellId = SpellId(391528);

const ECLIPSE_SOLAR_BUFF: AuraId = AuraId(48517);
const ECLIPSE_LUNAR_BUFF: AuraId = AuraId(48518);
const TOUCH_THE_COSMOS_BUFF: AuraId = AuraId(450357);

const MOONFIRE_DOT: AuraId = AuraId(164812);
const SUNFIRE_DOT: AuraId = AuraId(164815);

struct Bal<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    ap: f64,
    elunes_chosen: bool,
}

impl<'a> Bal<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let enemies = obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len());
        let ap = obs.resource(ResourceKind::AstralPower);
        let elunes_chosen = obs.hero_tree == Some(HeroTreeId(24));
        Self {
            obs,
            legal,
            enemies,
            ap,
            elunes_chosen,
        }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
    }

    fn dot_refreshable(&self, aura: AuraId, threshold_ms: u32) -> bool {
        self.obs
            .target_aura(aura)
            .is_none_or(|a| a.remaining.is_some_and(|r| r <= SimDuration(threshold_ms)))
    }

    fn rotation(&self) -> Option<SpellId> {
        let eclipse_up =
            self.obs.has_buff(ECLIPSE_SOLAR_BUFF) || self.obs.has_buff(ECLIPSE_LUNAR_BUFF);
        let free_spender = self.obs.has_buff(TOUCH_THE_COSMOS_BUFF);
        [
            self.pick(MOONFIRE, self.dot_refreshable(MOONFIRE_DOT, 6600)),
            self.pick(SUNFIRE, self.dot_refreshable(SUNFIRE_DOT, 5400)),
            self.pick(FORCE_OF_NATURE, true),
            self.pick(FURY_OF_ELUNE, true),
            self.pick(CELESTIAL_ALIGNMENT, true),
            self.pick(ECLIPSE, !eclipse_up),
            self.pick(CONVOKE_THE_SPIRITS, self.ap < 60.0),
            self.pick(
                STARFALL,
                self.enemies >= 2 && (free_spender || self.ap >= 50.0),
            ),
            self.pick(STARSURGE, free_spender || self.ap >= 40.0),
            self.pick(NEW_MOON, self.ap <= 80.0),
            self.pick(WILD_MUSHROOM, self.obs.charges(WILD_MUSHROOM) >= 2),
            self.pick(STARFIRE, self.elunes_chosen || self.enemies >= 3),
            self.pick(WRATH, true),
            self.pick(STARFIRE, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Balance Druid priority list (Keeper of the Grove & Elune's Chosen).
pub fn balance(d: &Decision<SeatObs>) -> Choice {
    Bal::new(d)
        .rotation()
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
