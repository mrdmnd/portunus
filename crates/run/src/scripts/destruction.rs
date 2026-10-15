//! Destruction Warlock.
//!
//! [`destruction`] ports SimC's 12.1 Hellcaller and Diabolist priority
//! lists (`profiles/MID2/MID2_Warlock_Destruction.simc`).

use portunus_core::{AuraId, SimDuration, SpellId};
use portunus_engine::{ActionMask, Choice, Wait};
use portunus_env::Decision;
use portunus_gamedata::stats::ResourceKind;

use crate::observe::SeatObs;

const INCINERATE: SpellId = SpellId(29722);
const IMMOLATE: SpellId = SpellId(348);
const WITHER: SpellId = SpellId(445468);
const CONFLAGRATE: SpellId = SpellId(17962);
const CHAOS_BOLT: SpellId = SpellId(116858);
const RAIN_OF_FIRE: SpellId = SpellId(5740);
const SHADOWBURN: SpellId = SpellId(17877);
const CATACLYSM: SpellId = SpellId(152108);
const SUMMON_INFERNAL: SpellId = SpellId(1122);
const SOUL_FIRE: SpellId = SpellId(6353);
const CHANNEL_DEMONFIRE: SpellId = SpellId(196447);
const HAVOC: SpellId = SpellId(80240);
const MALEVOLENCE: SpellId = SpellId(442726);
const RUINATION: SpellId = SpellId(434635);
const INFERNAL_BOLT: SpellId = SpellId(434506);

const BACKDRAFT_BUFF: AuraId = AuraId(117828);
const FIENDISH_CRUELTY_BUFF: AuraId = AuraId(1245634);
const DEMONIC_ART_BUFF: AuraId = AuraId(428524);

const IMMOLATE_DOT: AuraId = AuraId(157736);
const WITHER_DOT: AuraId = AuraId(445474);

struct Destro<'a> {
    obs: &'a SeatObs,
    legal: &'a ActionMask,
    enemies: usize,
    shards: f64,
    backdraft: u8,
    target_hp: f64,
}

impl<'a> Destro<'a> {
    fn new(d: &'a Decision<SeatObs>) -> Self {
        let (obs, legal) = (&d.obs, &d.legal);
        let enemies = obs.target.as_ref().map_or(0, |t| 1 + t.neighbours.len());
        let shards = obs.resource(ResourceKind::SoulShards);
        let target_hp = obs.target.as_ref().map_or(1.0, |t| t.health_pct);
        Self {
            obs,
            legal,
            enemies,
            shards,
            backdraft: obs.buff_stacks(BACKDRAFT_BUFF),
            target_hp,
        }
    }

    fn pick(&self, s: SpellId, when: bool) -> Option<SpellId> {
        (when && self.legal.is_ready(s)).then_some(s)
    }

    fn dot_refreshable(&self) -> bool {
        let imm = self
            .obs
            .target_aura(IMMOLATE_DOT)
            .is_none_or(|a| a.remaining.is_some_and(|r| r <= SimDuration(5400)));
        let wit = self
            .obs
            .target_aura(WITHER_DOT)
            .is_none_or(|a| a.remaining.is_some_and(|r| r <= SimDuration(5400)));
        imm && wit
    }

    fn rotation(&self) -> Option<SpellId> {
        let demonic_art = self.obs.has_buff(DEMONIC_ART_BUFF);
        let fiendish = self.obs.has_buff(FIENDISH_CRUELTY_BUFF);
        [
            self.pick(SUMMON_INFERNAL, true),
            self.pick(MALEVOLENCE, true),
            self.pick(RUINATION, true),
            self.pick(CATACLYSM, true),
            self.pick(HAVOC, self.enemies >= 2),
            self.pick(WITHER, self.dot_refreshable()),
            self.pick(IMMOLATE, self.dot_refreshable()),
            self.pick(SOUL_FIRE, self.shards <= 4.0),
            self.pick(CONFLAGRATE, self.obs.charges(CONFLAGRATE) >= 2),
            self.pick(RAIN_OF_FIRE, self.enemies >= 3 && self.shards >= 3.0),
            self.pick(SHADOWBURN, fiendish || self.target_hp <= 0.2),
            self.pick(
                CHAOS_BOLT,
                demonic_art || self.shards >= 3.5 || (self.shards >= 2.0 && self.backdraft >= 1),
            ),
            self.pick(CHANNEL_DEMONFIRE, !self.dot_refreshable()),
            self.pick(CONFLAGRATE, self.shards <= 4.4 && self.backdraft == 0),
            self.pick(INFERNAL_BOLT, self.shards <= 3.0),
            self.pick(INCINERATE, true),
        ]
        .into_iter()
        .flatten()
        .next()
    }
}

/// SimC's 12.1 Destruction Warlock priority list (Hellcaller & Diabolist).
pub fn destruction(d: &Decision<SeatObs>) -> Choice {
    Destro::new(d)
        .rotation()
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
