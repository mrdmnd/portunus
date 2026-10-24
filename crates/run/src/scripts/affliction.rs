//! Affliction Warlock.
//!
//! [`affliction`] ports SimC's 12.1 Hellcaller and Soul Harvester priority
//! lists (`profiles/MID2/MID2_Warlock_Affliction.simc`).

use portunus_core::{AuraId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, Wait};
use portunus_env::Decision;
use portunus_gamedata::stats::ResourceKind;

use crate::observe::SeatObs;

const SHADOW_BOLT: SpellId = SpellId(686);
const DRAIN_SOUL: SpellId = SpellId(198590);
const MALEFIC_GRASP: SpellId = SpellId(1261149);
const AGONY: SpellId = SpellId(980);
const CORRUPTION: SpellId = SpellId(172);
const WITHER: SpellId = SpellId(445468);
const UNSTABLE_AFFLICTION: SpellId = SpellId(1259790);
const SEED_OF_CORRUPTION: SpellId = SpellId(27243);
const HAUNT: SpellId = SpellId(48181);
const DARK_HARVEST: SpellId = SpellId(1257052);
const SUMMON_DARKGLARE: SpellId = SpellId(205180);
const MALEVOLENCE: SpellId = SpellId(442726);

const NIGHTFALL_BUFF: AuraId = AuraId(264571);
const SHARD_INSTABILITY_BUFF: AuraId = AuraId(1260265);
const DARKGLARE_BUFF: AuraId = AuraId(205180);

const AGONY_DOT: AuraId = AuraId(980);
const CORRUPTION_DOT: AuraId = AuraId(146739);
const WITHER_DOT: AuraId = AuraId(445474);
const SEED_DOT: AuraId = AuraId(27243);

struct Aff<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    shards: f64,
    nightfall: u8,
}

impl<'a> Aff<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let enemies = obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len());
        let shards = obs.resource(ResourceKind::SoulShards);
        Self {
            obs,
            legal,
            enemies,
            shards,
            nightfall: obs.buff_stacks(NIGHTFALL_BUFF),
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

    fn filler(&self, when: bool) -> Option<SpellId> {
        self.pick(MALEFIC_GRASP, when)
            .or_else(|| self.pick(DRAIN_SOUL, when))
            .or_else(|| self.pick(SHADOW_BOLT, when))
    }

    fn rotation(&self) -> Option<SpellId> {
        let corr_refresh =
            self.dot_refreshable(WITHER_DOT, 5400) && self.dot_refreshable(CORRUPTION_DOT, 4200);
        [
            self.pick(HAUNT, true),
            self.pick(AGONY, self.dot_refreshable(AGONY_DOT, 5400)),
            self.pick(
                SEED_OF_CORRUPTION,
                self.enemies >= 2 && corr_refresh && self.obs.target_aura(SEED_DOT).is_none(),
            ),
            self.pick(WITHER, corr_refresh),
            self.pick(CORRUPTION, corr_refresh),
            self.pick(DARK_HARVEST, true),
            self.pick(MALEVOLENCE, self.obs.target_aura(WITHER_DOT).is_some()),
            self.pick(SUMMON_DARKGLARE, true),
            self.filler(self.nightfall >= 2),
            self.pick(
                UNSTABLE_AFFLICTION,
                self.shards >= 1.0 || self.obs.has_buff(SHARD_INSTABILITY_BUFF),
            ),
            self.pick(MALEFIC_GRASP, self.obs.has_buff(DARKGLARE_BUFF)),
            self.filler(true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Affliction Warlock priority list (Hellcaller & Soul Harvester).
pub fn affliction(d: &Decision<SeatObs>) -> Choice {
    Aff::new(d)
        .rotation()
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
