//! Stats, modifiers, crits, and mitigation.

use std::sync::Arc;

use portunus_core::{ActorId, AuraId, PetId, Seat, SpecId, SpellId};
use portunus_engine::mechanics::HitKind;
pub(crate) use portunus_engine::state::predicate;
use portunus_engine::state::{ActorKind, AuraInstance};
use portunus_engine::{AuraRef, RunSetup, StateView};
use portunus_gamedata::aura::AuraDef;
use portunus_gamedata::effect::{Coefficient, ListenFor, ModKind, ModScope, Modifier};
use portunus_gamedata::item::{WeaponDef, WeaponHand};
use portunus_gamedata::pet::PetKind;
use portunus_gamedata::spell::SpellDef;
use portunus_gamedata::stats::{DerivedStats, RatedStat, SchoolMask, Stat, StatBlock};
use portunus_gamedata::GameData;

use crate::{CombatMath, EffectCtx, IncomingHit, Outgoing};

/// Attack power per point of weapon DPS.
const WEAPON_AP_DIVISOR: f64 = 6.0;
const OFF_HAND_MULT: f64 = 0.5;
const STAT_COUNT: usize = 12;
const RATED_COUNT: usize = 4;

#[inline]
const fn stat_idx(stat: Stat) -> usize {
    match stat {
        Stat::Strength => 0,
        Stat::Agility => 1,
        Stat::Intellect => 2,
        Stat::Stamina => 3,
        Stat::CritRating => 4,
        Stat::HasteRating => 5,
        Stat::MasteryRating => 6,
        Stat::VersatilityRating => 7,
        Stat::Armor => 8,
        Stat::Leech => 9,
        Stat::Avoidance => 10,
        Stat::Speed => 11,
    }
}

#[inline]
const fn rated_idx(stat: RatedStat) -> usize {
    match stat {
        RatedStat::Crit => 0,
        RatedStat::Haste => 1,
        RatedStat::Mastery => 2,
        RatedStat::Versatility => 3,
    }
}

fn stat_block_to_array(block: &StatBlock) -> [f64; STAT_COUNT] {
    let mut out = [0.0; STAT_COUNT];
    for (&stat, &val) in &block.0 {
        out[stat_idx(stat)] = val;
    }
    out
}

#[derive(Debug)]
struct SeatStats {
    spec: SpecId,
    /// The template's stats, before any aura.
    stats: [f64; STAT_COUNT],
    weapons: [Option<WeaponDef>; 2],
}

/// A seat's stats at one moment, with its auras applied.
#[derive(Clone, Copy)]
struct SeatSnapshot {
    spec: SpecId,
    stats: [f64; STAT_COUNT],
    /// `RatedPct` points by stat, added after rating conversion.
    rated: [f64; RATED_COUNT],
    /// The product of `AttackPowerPct` modifiers.
    ap_mult: f64,
}

/// Whose stats and modifiers an actor's actions use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Players and enemies: their own.
    Own,
    /// Totems act as their owner.
    Totem { owner: ActorId },
    /// Pets and guardians inherit some of the owner's and take the owner's
    /// pet or guardian damage modifiers.
    Pet { owner: ActorId, guardian: bool },
}

#[inline]
pub(crate) const fn mod_kind_bit(kind: ModKind) -> u32 {
    match kind {
        ModKind::DamageDonePct => 0,
        ModKind::PetDamagePct => 1,
        ModKind::GuardianDamagePct => 2,
        ModKind::DamageTakenPct => 3,
        ModKind::CritChanceAdd => 4,
        ModKind::CritDamagePct => 5,
        ModKind::HastePct => 6,
        ModKind::AttackSpeedPct => 7,
        ModKind::PersistentPct => 8,
        ModKind::CastTimePct => 9,
        ModKind::GcdPct => 10,
        ModKind::CooldownPct => 11,
        ModKind::ChargesAdd => 12,
        ModKind::CostPct => 13,
        ModKind::ResourceMax(_) => 14,
        ModKind::StatPct(_) => 15,
        ModKind::StatFlat(_) => 16,
        ModKind::AttackPowerPct => 17,
        ModKind::RatedPct(_) => 18,
        ModKind::CastWhileMoving => 19,
        ModKind::MoveSpeedPct => 20,
        ModKind::Immune => 21,
        ModKind::CritChanceScalesDamage => 22,
        ModKind::CritChanceAddsDamage => 23,
        ModKind::StatShare(_) => 24,
        ModKind::StatFromStat { .. } => 25,
        ModKind::ArmorPenPct => 26,
        ModKind::MasteryDamagePct => 27,
        ModKind::SourceDamageTakenPct => 28,
        ModKind::ChannelTicksAdd => 29,
        ModKind::ChainTargetsAdd => 30,
        ModKind::ChainSharePct | ModKind::CooldownRatePct | ModKind::TickPeriodPct => 31,
    }
}

#[inline]
pub(crate) const fn listen_for_bit(on: ListenFor) -> u32 {
    match on {
        ListenFor::CastComplete { .. } => 0,
        ListenFor::DamageDealt { .. } => 1,
        ListenFor::DamageTaken => 2,
        ListenFor::Swing { .. } => 3,
        ListenFor::WeaponHit { .. } => 4,
        ListenFor::PeriodicTick(_) => 5,
        ListenFor::ResourceSpent(_) => 6,
        ListenFor::AuraApplied(_) => 7,
        ListenFor::AuraExpired(_) => 8,
        ListenFor::PetExpired(_) => 9,
        ListenFor::Departed => 10,
        ListenFor::EnemyDied { .. } => 11,
        ListenFor::CastStart { .. } => 12,
        ListenFor::ResourceGained(_) => 13,
        ListenFor::Absorbed(_) => 14,
        ListenFor::Interrupted => 15,
        ListenFor::MoveStart => 16,
        ListenFor::MoveEnd => 17,
        ListenFor::HealthBelow { .. } => 18,
        ListenFor::AuraStacksReached { .. } => 19,
        ListenFor::AuraRemovedBy { .. } => 20,
    }
}

const STAT_MOD_MASK: u32 = (1 << mod_kind_bit(ModKind::AttackPowerPct))
    | (1 << mod_kind_bit(ModKind::StatFlat(Stat::Strength)))
    | (1 << mod_kind_bit(ModKind::StatPct(Stat::Strength)))
    | (1 << mod_kind_bit(ModKind::RatedPct(RatedStat::Crit)))
    | (1 << mod_kind_bit(ModKind::StatShare(Stat::Strength)))
    | (1 << mod_kind_bit(ModKind::StatFromStat {
        from: Stat::CritRating,
        to: Stat::MasteryRating,
    }));

const OUTGOING_MOD_MASK: u32 = (1 << mod_kind_bit(ModKind::DamageDonePct))
    | (1 << mod_kind_bit(ModKind::CritChanceScalesDamage))
    | (1 << mod_kind_bit(ModKind::CritChanceAddsDamage))
    | (1 << mod_kind_bit(ModKind::MasteryDamagePct));

const MITIGATE_MOD_MASK: u32 =
    (1 << mod_kind_bit(ModKind::Immune)) | (1 << mod_kind_bit(ModKind::DamageTakenPct));

const CHAIN_MOD_MASK: u32 =
    (1 << mod_kind_bit(ModKind::ChainTargetsAdd)) | (1 << mod_kind_bit(ModKind::ChainSharePct));

#[derive(Debug, Clone)]
pub(crate) struct FastAura {
    pub mod_mask: u32,
    pub listen_mask: u32,
    pub has_kernel_stat_mod: bool,
    pub has_cond_kernel_stat_mod: bool,
    pub def: AuraDef,
}

#[derive(Debug)]
pub(crate) struct FastAuraTable {
    slots: Vec<Option<(AuraId, FastAura)>>,
    shift: u32,
    mask: usize,
}

impl FastAuraTable {
    fn from_data(data: &GameData) -> Self {
        let cap = (data.auras.len() * 4).next_power_of_two().max(64);
        let shift = usize::BITS - cap.trailing_zeros();
        let mask = cap - 1;
        let mut slots = vec![None; cap];
        for (&id, def) in &data.auras {
            let mut mod_mask = 0u32;
            let mut has_kernel_stat_mod = false;
            let mut has_cond_kernel_stat_mod = false;
            for m in &def.modifiers {
                mod_mask |= 1u32 << mod_kind_bit(m.kind);
                let is_kernel_stat = matches!(
                    m.kind,
                    ModKind::HastePct
                        | ModKind::AttackSpeedPct
                        | ModKind::StatFlat(_)
                        | ModKind::StatPct(_)
                        | ModKind::StatShare(_)
                        | ModKind::StatFromStat { .. }
                        | ModKind::RatedPct(RatedStat::Haste)
                );
                if is_kernel_stat {
                    has_kernel_stat_mod = true;
                    if m.condition.is_some() {
                        has_cond_kernel_stat_mod = true;
                    }
                }
            }
            let mut listen_mask = 0u32;
            for l in &def.listeners {
                listen_mask |= 1u32 << listen_for_bit(l.on);
            }
            let fast = FastAura {
                mod_mask,
                listen_mask,
                has_kernel_stat_mod,
                has_cond_kernel_stat_mod,
                def: def.clone(),
            };
            let mut idx = ((id.0 as usize).wrapping_mul(0x9E37_79B9_7F4A_7C15)) >> shift;
            while slots[idx].is_some() {
                idx = (idx + 1) & mask;
            }
            slots[idx] = Some((id, fast));
        }
        Self { slots, shift, mask }
    }

    #[inline]
    pub fn get(&self, id: AuraId) -> Option<&FastAura> {
        let mut idx = ((id.0 as usize).wrapping_mul(0x9E37_79B9_7F4A_7C15)) >> self.shift;
        loop {
            match &self.slots[idx] {
                Some((k, v)) if *k == id => return Some(v),
                Some(_) => idx = (idx + 1) & self.mask,
                None => return None,
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct FastSpellTable {
    slots: Vec<Option<(SpellId, SpellDef)>>,
    shift: u32,
    mask: usize,
}

impl FastSpellTable {
    fn from_data(data: &GameData) -> Self {
        let cap = (data.spells.len() * 4).next_power_of_two().max(64);
        let shift = usize::BITS - cap.trailing_zeros();
        let mask = cap - 1;
        let mut slots = vec![None; cap];
        for (&id, def) in &data.spells {
            let mut idx = ((id.0 as usize).wrapping_mul(0x9E37_79B9_7F4A_7C15)) >> shift;
            while slots[idx].is_some() {
                idx = (idx + 1) & mask;
            }
            slots[idx] = Some((id, def.clone()));
        }
        Self { slots, shift, mask }
    }

    #[inline]
    pub fn get(&self, id: SpellId) -> Option<&SpellDef> {
        let mut idx = ((id.0 as usize).wrapping_mul(0x9E37_79B9_7F4A_7C15)) >> self.shift;
        loop {
            match &self.slots[idx] {
                Some((k, v)) if *k == id => return Some(v),
                Some(_) => idx = (idx + 1) & self.mask,
                None => return None,
            }
        }
    }
}

#[derive(Debug)]
struct Statics {
    data: Arc<GameData>,
    seats: Vec<SeatStats>,
    curves: [Option<portunus_gamedata::stats::RatingCurve>; STAT_COUNT],
    auras: FastAuraTable,
    spells: FastSpellTable,
}

/// The [`CombatMath`] implementation.
///
/// A seat's stats are its template's, plus `StatFlat` modifiers, times
/// `StatPct` modifiers, converted by `portunus_loadout::derive`. Amounts
/// stack in SimC's layers: coefficient, then `DamageDonePct` modifiers
/// (multiplicative with each other), then versatility, then the aura's
/// snapshotted persistent multiplier. Crit chance and crit damage bonuses
/// add up within their kind.
///
/// Pets and guardians use their own auras, plus the owner's
/// `PetDamagePct` or `GuardianDamagePct` and unscoped `CritChanceAdd`
/// modifiers; totems use the owner's stats and auras outright (see
/// [`PetScaling`](portunus_gamedata::pet::PetScaling)).
#[derive(Debug, Clone)]
pub struct Formulas {
    s: Arc<Statics>,
}

/// Which modifiers apply: those on `holder`'s auras whose scope matches
/// `spell` or `school` and whose condition holds.
#[derive(Debug, Clone, Copy)]
struct Query {
    holder: ActorId,
    target: Option<ActorId>,
    spell: Option<SpellId>,
    school: Option<SchoolMask>,
    /// An auto-attack's damage, for `ModScope::AutoAttack` and
    /// `ModScope::Abilities`.
    auto: bool,
}

impl Query {
    fn holder(holder: ActorId) -> Self {
        Self {
            holder,
            target: None,
            spell: None,
            school: None,
            auto: false,
        }
    }

    fn holder_targeted(view: &dyn StateView, holder: ActorId) -> Self {
        Self {
            target: view.target(holder),
            ..Self::holder(holder)
        }
    }
}

impl Formulas {
    pub fn new(setup: &RunSetup) -> Self {
        let seats = setup
            .seats
            .iter()
            .map(|s| SeatStats {
                spec: s.template.spec,
                stats: stat_block_to_array(&s.template.stats),
                weapons: [s.template.main_hand, s.template.off_hand],
            })
            .collect();
        let curves = std::array::from_fn(|i| {
            let stat = match i {
                0 => Stat::Strength,
                1 => Stat::Agility,
                2 => Stat::Intellect,
                3 => Stat::Stamina,
                4 => Stat::CritRating,
                5 => Stat::HasteRating,
                6 => Stat::MasteryRating,
                7 => Stat::VersatilityRating,
                8 => Stat::Armor,
                9 => Stat::Leech,
                10 => Stat::Avoidance,
                _ => Stat::Speed,
            };
            setup.data.curves.ratings.get(&stat).cloned()
        });
        let auras = FastAuraTable::from_data(&setup.data);
        let spells = FastSpellTable::from_data(&setup.data);
        Self {
            s: Arc::new(Statics {
                data: Arc::clone(&setup.data),
                seats,
                curves,
                auras,
                spells,
            }),
        }
    }

    pub(crate) fn data(&self) -> &GameData {
        &self.s.data
    }

    #[inline]
    pub(crate) fn fast_aura(&self, id: AuraId) -> Option<&FastAura> {
        self.s.auras.get(id)
    }

    #[inline]
    pub(crate) fn fast_spell(&self, id: SpellId) -> Option<&SpellDef> {
        self.s.spells.get(id)
    }

    fn role(&self, view: &dyn StateView, actor: ActorId) -> Role {
        let Some(ActorKind::Pet { owner, pet }) = view.actor(actor).map(|a| a.kind) else {
            return Role::Own;
        };
        let Some(&owner) = view.seats().get(usize::from(owner.0)) else {
            return Role::Own;
        };
        match self.s.data.pets.get(&pet).map(|d| d.kind) {
            Some(PetKind::Totem) => Role::Totem { owner },
            Some(PetKind::Pet) => Role::Pet {
                owner,
                guardian: false,
            },
            Some(PetKind::Guardian) | None => Role::Pet {
                owner,
                guardian: true,
            },
        }
    }

    /// Whose auras modify `actor`'s actions: a totem's owner's, else its
    /// own.
    fn modifier_holder(&self, view: &dyn StateView, actor: ActorId) -> ActorId {
        match self.role(view, actor) {
            Role::Totem { owner } => owner,
            Role::Own | Role::Pet { .. } => actor,
        }
    }

    /// An equipped weapon: a seat's from its template, a pet's from its
    /// definition (main hand only).
    pub fn weapon(
        &self,
        view: &dyn StateView,
        actor: ActorId,
        hand: WeaponHand,
    ) -> Option<WeaponDef> {
        let i = match hand {
            WeaponHand::MainHand => 0,
            WeaponHand::OffHand => 1,
        };
        match view.actor(actor)?.kind {
            ActorKind::Player(seat) => {
                let form = view
                    .auras(actor)
                    .iter()
                    .find_map(|x| self.fast_aura(x.aura)?.def.form.as_ref()?.weapon);
                match form {
                    Some(w) => (i == 0).then_some(w),
                    None => self.s.seats.get(usize::from(seat.0))?.weapons[i],
                }
            }
            ActorKind::Pet { pet, .. } if i == 0 => self.s.data.pets.get(&pet)?.melee,
            ActorKind::Pet { .. } | ActorKind::Enemy { .. } => None,
        }
    }

    /// A white hit before modifiers: average weapon damage plus attack
    /// power over 6 per second of weapon speed, halved for the off hand.
    /// As in SimC's `action_t::calculate_weapon_damage`, `AttackPowerPct`
    /// scales the weapon roll too, and the attack power is the actor's own,
    /// without the weapon's (`attack_power_type::NO_WEAPON`).
    pub fn weapon_damage(&self, view: &dyn StateView, actor: ActorId, hand: WeaponHand) -> f64 {
        let Some(w) = self.weapon(view, actor, hand) else {
            return 0.0;
        };
        let speed = f64::from(w.speed.millis()) / 1000.0;
        let ap = self.derived(view, actor).attack_power;
        let roll = (w.min_damage + w.max_damage) / 2.0 * self.ap_mult(view, actor);
        let hit = roll + ap / WEAPON_AP_DIVISOR * speed;
        match hand {
            WeaponHand::MainHand => hit,
            WeaponHand::OffHand => hit * OFF_HAND_MULT,
        }
    }

    /// The attack power an attack-power coefficient scales with: the
    /// actor's own plus its weapon's, `floor(dps * 6)` times its
    /// `AttackPowerPct` modifiers (SimC's
    /// `composite_total_attack_power_by_type` with the default
    /// `WEAPON_MAINHAND`, or `WEAPON_OFFHAND` for an off-hand strike,
    /// halved by the off-hand penalty). An actor without a weapon there
    /// adds none, where SimC counts 0.5 dps unarmed; pets add none.
    fn ability_attack_power(&self, view: &dyn StateView, ctx: &EffectCtx) -> f64 {
        let own = self.derived(view, ctx.caster).attack_power;
        if player_seat(view, ctx.caster).is_none() {
            return own;
        }
        let hand = ctx
            .hand
            .or_else(|| self.fast_spell(ctx.spell?)?.weapon)
            .unwrap_or(WeaponHand::MainHand);
        let weapon = self.weapon(view, ctx.caster, hand).map_or(0.0, |w| {
            let speed = f64::from(w.speed.millis()) / 1000.0;
            let dps = (w.min_damage + w.max_damage) / 2.0 / speed;
            (dps * WEAPON_AP_DIVISOR).floor()
        });
        let total = own + weapon * self.ap_mult(view, ctx.caster);
        match hand {
            WeaponHand::MainHand => total,
            WeaponHand::OffHand => total * OFF_HAND_MULT,
        }
    }

    /// How many more enemies a single-target hit chains to (the sum of its
    /// `ChainTargetsAdd`, rounded) and the share of the hit each takes (the
    /// product of its `ChainSharePct`, as fractions).
    pub(crate) fn chain(
        &self,
        view: &dyn StateView,
        ctx: &EffectCtx,
        school: SchoolMask,
    ) -> (usize, f64) {
        let q = self.spell_query(view, ctx, Some(school));
        let (mut n, mut share) = (0.0, 1.0);
        self.each_mod_masked(view, &q, CHAIN_MOD_MASK, |m, v| match m.kind {
            ModKind::ChainTargetsAdd => n += v,
            ModKind::ChainSharePct => share *= v / 100.0,
            _ => {}
        });
        (n.round().max(0.0) as usize, share)
    }

    /// The product of a seat's `AttackPowerPct` modifiers; 1 for others.
    fn ap_mult(&self, view: &dyn StateView, actor: ActorId) -> f64 {
        self.stat_block(view, actor).map_or(1.0, |s| s.ap_mult)
    }

    /// The chance an auto-attack misses: only an actor wielding two weapons
    /// can miss.
    pub fn white_miss_chance(&self, view: &dyn StateView, actor: ActorId) -> f64 {
        let dual = self.weapon(view, actor, WeaponHand::MainHand).is_some()
            && self.weapon(view, actor, WeaponHand::OffHand).is_some();
        if dual {
            self.s.data.curves.dual_wield_miss_pct / 100.0
        } else {
            0.0
        }
    }

    /// The product of `AttackSpeedPct` modifiers on the actor's own auras.
    pub fn attack_speed_mult(&self, view: &dyn StateView, actor: ActorId) -> f64 {
        self.mod_product(
            view,
            &Query::holder_targeted(view, actor),
            ModKind::AttackSpeedPct,
        )
    }

    /// The spec of a seat, if the setup had one there.
    pub(crate) fn seat_spec(&self, seat: Seat) -> Option<SpecId> {
        self.s.seats.get(usize::from(seat.0)).map(|s| s.spec)
    }

    /// A seat's stats with every active aura applied, and its `RatedPct`
    /// additions; `None` for actors that aren't players.
    fn stat_block(&self, view: &dyn StateView, actor: ActorId) -> Option<SeatSnapshot> {
        self.stat_block_sharing(view, actor, true)
    }

    /// [`Self::stat_block`], taking `StatShare`s from other seats only if
    /// `shared`; a source's own stats are read without them.
    fn stat_block_sharing(
        &self,
        view: &dyn StateView,
        actor: ActorId,
        shared: bool,
    ) -> Option<SeatSnapshot> {
        let seat = player_seat(view, actor)?;
        let base = self.s.seats.get(usize::from(seat.0))?;
        let mut stats = base.stats;
        let mut pct = [1.0; STAT_COUNT];
        let mut has_pct = [false; STAT_COUNT];
        let mut rated = [0.0; RATED_COUNT];
        let mut ap_mult = 1.0;
        let mut shares: Vec<(Stat, f64, ActorId)> = Vec::new();
        let mut conversions: Vec<(Stat, Stat, f64)> = Vec::new();
        self.each_sourced_mod_masked(
            view,
            &Query::holder_targeted(view, actor),
            STAT_MOD_MASK,
            |m, value, source| match m.kind {
                ModKind::AttackPowerPct => ap_mult *= 1.0 + value / 100.0,
                ModKind::StatFlat(stat) => stats[stat_idx(stat)] += value,
                ModKind::StatPct(stat) => {
                    let idx = stat_idx(stat);
                    pct[idx] *= 1.0 + value / 100.0;
                    has_pct[idx] = true;
                }
                ModKind::RatedPct(stat) => rated[rated_idx(stat)] += value,
                ModKind::StatShare(stat) if shared && source != actor => {
                    shares.push((stat, value, source));
                }
                ModKind::StatFromStat { from, to } => conversions.push((from, to, value)),
                _ => {}
            },
        );
        for (stat, value, source) in shares {
            let Some(from) = self.stat_block_sharing(view, source, false) else {
                continue;
            };
            let have = from.stats[stat_idx(stat)];
            stats[stat_idx(stat)] += have * value / 100.0;
        }
        for i in 0..STAT_COUNT {
            if has_pct[i] {
                stats[i] *= pct[i];
            }
        }
        if !conversions.is_empty() {
            let converted = stats;
            for (from, to, value) in conversions {
                let have = converted[stat_idx(from)];
                let mult = pct[stat_idx(to)];
                stats[stat_idx(to)] += have * value / 100.0 * mult;
            }
        }
        Some(SeatSnapshot {
            spec: base.spec,
            stats,
            rated,
            ap_mult,
        })
    }

    fn rating_to_pct(&self, stat: Stat, rating: f64) -> f64 {
        let Some(curve) = &self.s.curves[stat_idx(stat)] else {
            return 0.0;
        };
        let raw = rating / curve.rating_per_pct;
        let mut out = 0.0;
        let mut prev = 0.0;
        let mut keep = 1.0;
        for &(threshold, fraction) in &curve.diminishing {
            if raw <= threshold {
                break;
            }
            out += (threshold - prev) * keep;
            prev = threshold;
            keep = fraction;
        }
        let pct = out + (raw - prev) * keep;
        curve
            .falloff
            .map_or(pct, |f| pct * f.scale * (-f.per_rating * rating).exp())
    }

    /// Percentages and derived values with every active aura applied. Pets
    /// take theirs from their owner's (see [`PetScaling`]); enemies have no
    /// stats beyond their health.
    ///
    /// [`PetScaling`]: portunus_gamedata::pet::PetScaling
    pub fn derived(&self, view: &dyn StateView, actor: ActorId) -> DerivedStats {
        if let Some(ActorKind::Pet { owner, pet }) = view.actor(actor).map(|a| a.kind) {
            if let Role::Totem { owner } = self.role(view, actor) {
                return DerivedStats {
                    max_health: view.actor(actor).map_or(0.0, |a| a.max_health as f64),
                    ..self.derived(view, owner)
                };
            }
            return self.pet_derived(view, actor, owner, pet);
        }
        let spec = self
            .stat_block(view, actor)
            .and_then(|s| Some((self.s.data.specs.get(&s.spec)?, s)));
        match spec {
            Some((spec, s)) => {
                let curves = &self.s.data.curves;
                let stat = |st: Stat| s.stats[stat_idx(st)];
                let pct = |st: Stat| self.rating_to_pct(st, stat(st));
                let primary_for_ap = match spec.primary_stat {
                    Stat::Agility | Stat::Strength => stat(spec.primary_stat),
                    _ => 0.0,
                };
                DerivedStats {
                    crit_pct: curves.base_crit_pct
                        + pct(Stat::CritRating)
                        + s.rated[rated_idx(RatedStat::Crit)],
                    haste_pct: pct(Stat::HasteRating) + s.rated[rated_idx(RatedStat::Haste)],
                    mastery_pct: spec.base_mastery_pct
                        + (pct(Stat::MasteryRating) + s.rated[rated_idx(RatedStat::Mastery)])
                            * spec.mastery_coef,
                    versatility_pct: pct(Stat::VersatilityRating)
                        + s.rated[rated_idx(RatedStat::Versatility)],
                    attack_power: primary_for_ap * curves.attack_power_per_primary * s.ap_mult,
                    spell_power: stat(Stat::Intellect) * curves.spell_power_per_intellect,
                    max_health: stat(Stat::Stamina) * curves.health_per_stamina,
                    leech_pct: pct(Stat::Leech),
                    avoidance_pct: pct(Stat::Avoidance),
                    speed_pct: pct(Stat::Speed),
                }
            }
            None => DerivedStats {
                max_health: view.actor(actor).map_or(0.0, |a| a.max_health as f64),
                ..DerivedStats::default()
            },
        }
    }

    fn pet_derived(
        &self,
        view: &dyn StateView,
        actor: ActorId,
        owner: Seat,
        pet: PetId,
    ) -> DerivedStats {
        let k = self
            .s
            .data
            .pets
            .get(&pet)
            .map(|d| d.scaling)
            .unwrap_or_default();
        let o = view
            .seats()
            .get(usize::from(owner.0))
            .map(|&id| self.derived(view, id))
            .unwrap_or_default();
        DerivedStats {
            crit_pct: o.crit_pct,
            haste_pct: o.haste_pct,
            mastery_pct: o.mastery_pct,
            versatility_pct: o.versatility_pct,
            attack_power: o.attack_power * k.ap_from_ap + o.spell_power * k.ap_from_sp,
            spell_power: o.spell_power * k.sp_from_sp + o.attack_power * k.sp_from_ap,
            max_health: view.actor(actor).map_or(0.0, |a| a.max_health as f64),
            ..DerivedStats::default()
        }
    }

    /// The secondary rating `actor` (a pet's owner) has most of, ties going
    /// to versatility, then mastery, haste and crit, as SimC's
    /// `util::highest_stat` over `secondary_ratings` (ratings with buffs,
    /// not percentages).
    pub fn highest_secondary(&self, view: &dyn StateView, actor: ActorId) -> RatedStat {
        let who = owner_seat(view, actor)
            .and_then(|s| view.seats().get(usize::from(s.0)).copied())
            .unwrap_or(actor);
        let stats = self
            .stat_block(view, who)
            .map(|s| s.stats)
            .unwrap_or([0.0; STAT_COUNT]);
        let rating = |s: Stat| stats[stat_idx(s)];
        let mut best = (RatedStat::Versatility, rating(Stat::VersatilityRating));
        for (r, s) in [
            (RatedStat::Mastery, Stat::MasteryRating),
            (RatedStat::Haste, Stat::HasteRating),
            (RatedStat::Crit, Stat::CritRating),
        ] {
            if rating(s) > best.1 {
                best = (r, rating(s));
            }
        }
        best.0
    }

    /// The attacker's multiplier on its target's armor, from its
    /// `ArmorPenPct` modifiers.
    fn armor_kept(&self, view: &dyn StateView, hit: &IncomingHit) -> f64 {
        let Some(attacker) = hit.attacker else {
            return 1.0;
        };
        let q = Query {
            holder: self.modifier_holder(view, attacker),
            target: None,
            spell: hit.spell,
            school: Some(hit.school),
            auto: false,
        };
        let mut kept = 1.0;
        let mask = 1u32 << mod_kind_bit(ModKind::ArmorPenPct);
        self.each_mod_masked(view, &q, mask, |_, v| {
            kept *= (1.0 - v / 100.0).max(0.0);
        });
        kept
    }

    /// The product of the target's `SourceDamageTakenPct` modifiers from
    /// auras the attacker (or its owner, for a pet or guardian) applied,
    /// scoped to the attacker's spell.
    fn source_taken(&self, view: &dyn StateView, target: ActorId, hit: &IncomingHit) -> f64 {
        let Some(attacker) = hit.attacker else {
            return 1.0;
        };
        let owner = match self.role(view, attacker) {
            Role::Pet { owner, .. } | Role::Totem { owner } => Some(owner),
            Role::Own => None,
        };
        let attacker = self.modifier_holder(view, attacker);
        let q = Query {
            spell: hit.spell,
            school: Some(hit.school),
            ..Query::holder(target)
        };
        let mut total = 1.0;
        let mask = 1u32 << mod_kind_bit(ModKind::SourceDamageTakenPct);
        self.each_sourced_mod_masked(view, &q, mask, |_, v, source| {
            if source == attacker || owner == Some(source) {
                total *= 1.0 + v / 100.0;
            }
        });
        total
    }

    /// A rated secondary in percent.
    pub fn rated_pct(&self, view: &dyn StateView, actor: ActorId, stat: RatedStat) -> f64 {
        let d = self.derived(view, actor);
        match stat {
            RatedStat::Crit => d.crit_pct,
            RatedStat::Haste => d.haste_pct,
            RatedStat::Mastery => d.mastery_pct,
            RatedStat::Versatility => d.versatility_pct,
        }
    }

    fn armor(&self, view: &dyn StateView, target: ActorId) -> f64 {
        match view.actor(target).map(|a| a.kind) {
            Some(ActorKind::Enemy { .. }) => view
                .enemy_info(target)
                .and_then(|e| e.def)
                .map_or(0.0, |d| d.defense.armor),
            Some(ActorKind::Player(_)) => self
                .stat_block(view, target)
                .map_or(0.0, |s| s.stats[stat_idx(Stat::Armor)]),
            Some(ActorKind::Pet { .. }) | None => 0.0,
        }
    }

    fn spell_school(&self, spell: Option<SpellId>) -> Option<SchoolMask> {
        spell.and_then(|s| self.fast_spell(s)).map(|d| d.school)
    }

    /// Calls `f` with every applicable modifier matching `want_mask` and its
    /// value times stacks (for `per_stack` modifiers).
    fn each_mod_masked(
        &self,
        view: &dyn StateView,
        q: &Query,
        want_mask: u32,
        mut f: impl FnMut(&Modifier, f64),
    ) {
        self.each_sourced_mod_masked(view, q, want_mask, |m, value, _| f(m, value));
    }

    /// [`Self::each_mod_masked`], also passing who applied each modifier's aura.
    fn each_sourced_mod_masked(
        &self,
        view: &dyn StateView,
        q: &Query,
        want_mask: u32,
        mut f: impl FnMut(&Modifier, f64, ActorId),
    ) {
        for inst in view.auras(q.holder) {
            let Some(fast) = self.fast_aura(inst.aura) else {
                continue;
            };
            if fast.mod_mask & want_mask == 0 {
                continue;
            }
            for m in &fast.def.modifiers {
                if (1u32 << mod_kind_bit(m.kind)) & want_mask == 0 {
                    continue;
                }
                if !self.applies(view, q, m) {
                    continue;
                }
                let stacks = if m.per_stack {
                    f64::from(inst.stacks)
                } else {
                    1.0
                };
                f(m, m.value * stacks, inst.source);
            }
        }
    }

    fn applies(&self, view: &dyn StateView, q: &Query, m: &Modifier) -> bool {
        let in_scope = match m.scope {
            ModScope::All => true,
            ModScope::Spell(s) => q.spell == Some(s),
            ModScope::School(mask) => q.school.is_some_and(|s| s.0 & mask.0 != 0),
            ModScope::AutoAttack => q.auto,
            ModScope::Abilities => !q.auto,
        };
        in_scope
            && m.condition
                .is_none_or(|p| predicate(view, p, q.holder, q.target, q.spell))
    }

    fn mod_sum(&self, view: &dyn StateView, q: &Query, kind: ModKind) -> f64 {
        let mut total = 0.0;
        let mask = 1u32 << mod_kind_bit(kind);
        self.each_mod_masked(view, q, mask, |m, v| {
            if m.kind == kind {
                total += v;
            }
        });
        total
    }

    /// The product of `1 + value / 100` over matching modifiers.
    fn mod_product(&self, view: &dyn StateView, q: &Query, kind: ModKind) -> f64 {
        let mut total = 1.0;
        let mask = 1u32 << mod_kind_bit(kind);
        self.each_mod_masked(view, q, mask, |m, v| {
            if m.kind == kind {
                total *= 1.0 + v / 100.0;
            }
        });
        total
    }

    fn spell_query(
        &self,
        view: &dyn StateView,
        ctx: &EffectCtx,
        school: Option<SchoolMask>,
    ) -> Query {
        Query {
            holder: self.modifier_holder(view, ctx.caster),
            target: ctx.target,
            spell: ctx.spell,
            school: school.or_else(|| self.spell_school(ctx.spell)),
            auto: ctx.spell.is_none() && ctx.aura.is_none() && ctx.hand.is_some(),
        }
    }

    /// The owner's pet or guardian damage multiplier for a pet's action;
    /// 1 for everyone else.
    fn owner_pet_mult(&self, view: &dyn StateView, ctx: &EffectCtx, school: SchoolMask) -> f64 {
        let Role::Pet { owner, guardian } = self.role(view, ctx.caster) else {
            return 1.0;
        };
        let q = Query {
            holder: owner,
            ..self.spell_query(view, ctx, Some(school))
        };
        let kind = if guardian {
            ModKind::GuardianDamagePct
        } else {
            ModKind::PetDamagePct
        };
        self.mod_product(view, &q, kind)
    }

    /// Crit chance as a fraction, before clamping to `[0, 1]`.
    fn uncapped_crit(&self, view: &dyn StateView, ctx: &EffectCtx) -> f64 {
        let add = self.mod_sum(
            view,
            &self.spell_query(view, ctx, None),
            ModKind::CritChanceAdd,
        ) + self.owner_crit_add(view, ctx.caster);
        (self.derived(view, ctx.caster).crit_pct + add) / 100.0
    }

    /// Crit the owner's unscoped modifiers grant, which pets inherit.
    fn owner_crit_add(&self, view: &dyn StateView, actor: ActorId) -> f64 {
        match self.role(view, actor) {
            Role::Pet { owner, .. } => {
                self.mod_sum(view, &Query::holder(owner), ModKind::CritChanceAdd)
            }
            Role::Own | Role::Totem { .. } => 0.0,
        }
    }
}

impl CombatMath for Formulas {
    fn base(&self, view: &dyn StateView, ctx: &EffectCtx, amount: Coefficient) -> f64 {
        let raw = match amount {
            Coefficient::Flat(x) => x,
            Coefficient::SpellPower(c) => c * self.derived(view, ctx.caster).spell_power,
            Coefficient::AttackPower(c) => c * self.ability_attack_power(view, ctx),
            Coefficient::WeaponDamage(c) => {
                let hand = ctx.hand.unwrap_or(WeaponHand::MainHand);
                c * self.weapon_damage(view, ctx.caster, hand)
            }
            Coefficient::PctMaxHealth(pct) => {
                let who = ctx.target.unwrap_or(ctx.caster);
                pct / 100.0 * view.actor(who).map_or(0.0, |a| a.max_health as f64)
            }
            Coefficient::EventAmount(f) => f * ctx.event_amount.unwrap_or(0.0),
            Coefficient::AuraValue(f) => {
                f * ctx
                    .aura
                    .and_then(|r| instance(view, r))
                    .map_or(0.0, |i| i.value)
            }
            Coefficient::WeaponSpeed(c) => {
                let hand = ctx.hand.unwrap_or(WeaponHand::MainHand);
                self.weapon(view, ctx.caster, hand)
                    .map_or(0.0, |w| c * f64::from(w.speed.millis()) / 1000.0)
            }
            Coefficient::RecentDamageTaken { window, share } => {
                share * view.recent_damage_taken(ctx.caster, window) as f64
            }
        };
        raw * ctx.scale
    }

    fn outgoing(
        &self,
        view: &dyn StateView,
        ctx: &EffectCtx,
        amount: Coefficient,
        kind: Outgoing,
    ) -> f64 {
        let caster_derived = self.derived(view, ctx.caster);
        let base = match amount {
            Coefficient::SpellPower(c) => c * caster_derived.spell_power * ctx.scale,
            _ => self.base(view, ctx, amount),
        };
        let done = match kind {
            Outgoing::Damage(school) => {
                let q = self.spell_query(view, ctx, Some(school));
                let mastery = caster_derived.mastery_pct / 100.0;
                let mut crit_scales = false;
                let mut crit_adds = false;
                let mut by_mastery = 1.0;
                let mut damage_done = 1.0;
                self.each_mod_masked(view, &q, OUTGOING_MOD_MASK, |m, v| match m.kind {
                    ModKind::CritChanceScalesDamage => crit_scales = true,
                    ModKind::CritChanceAddsDamage => crit_adds = true,
                    ModKind::MasteryDamagePct => by_mastery *= 1.0 + v * mastery,
                    ModKind::DamageDonePct => damage_done *= 1.0 + v / 100.0,
                    _ => {}
                });
                let mut by_crit = if crit_scales {
                    self.uncapped_crit(view, ctx).max(0.0)
                } else {
                    1.0
                };
                if crit_adds {
                    let untargeted = EffectCtx {
                        target: None,
                        ..*ctx
                    };
                    by_crit *= 1.0 + self.uncapped_crit(view, &untargeted).max(0.0);
                }
                damage_done * self.owner_pet_mult(view, ctx, school) * by_crit * by_mastery
            }
            Outgoing::Heal => 1.0,
        };
        let vers = 1.0 + caster_derived.versatility_pct / 100.0;
        let pmult = ctx
            .aura
            .and_then(|r| instance(view, r))
            .map_or(1.0, |i| i.pmultiplier);
        base * done * vers * pmult
    }

    fn crit_chance(&self, view: &dyn StateView, ctx: &EffectCtx) -> f64 {
        self.uncapped_crit(view, ctx).clamp(0.0, 1.0)
    }

    fn crit_multiplier(&self, view: &dyn StateView, ctx: &EffectCtx) -> f64 {
        let bonus = self.mod_sum(
            view,
            &self.spell_query(view, ctx, None),
            ModKind::CritDamagePct,
        );
        2.0 * (1.0 + bonus / 100.0)
    }

    /// Armor as in SimC's `player_t::target_mitigation`: only for
    /// `DMG_DIRECT` results, and not for actions with `ignores_armor`.
    fn mitigate(
        &self,
        view: &dyn StateView,
        target: ActorId,
        amount: f64,
        hit: IncomingHit,
    ) -> f64 {
        let q = Query {
            school: Some(hit.school),
            ..Query::holder(target)
        };
        let mut immune = false;
        let mut dmg_taken = 1.0;
        self.each_mod_masked(view, &q, MITIGATE_MOD_MASK, |m, v| match m.kind {
            ModKind::Immune => immune = true,
            ModKind::DamageTakenPct => dmg_taken *= 1.0 + v / 100.0,
            _ => {}
        });
        if immune {
            return 0.0;
        }
        let mut out = amount * dmg_taken * self.source_taken(view, target, &hit);
        let armored = hit.kind == HitKind::Direct && !hit.ignores_armor;
        if hit.school == SchoolMask::PHYSICAL && armored {
            let armor = self.armor(view, target) * self.armor_kept(view, &hit);
            if armor > 0.0 {
                out *= 1.0 - armor / (armor + self.s.data.curves.armor_constant);
            }
        }
        if player_seat(view, target).is_some() {
            let d = self.derived(view, target);
            out *= 1.0 - d.versatility_pct / 200.0;
            if hit.area {
                out *= (1.0 - d.avoidance_pct / 100.0).max(0.0);
            }
        }
        out.max(0.0)
    }

    /// Pets and guardians run at their owner's haste times their own
    /// `HastePct` modifiers; totems at their owner's.
    fn haste_mult(&self, view: &dyn StateView, actor: ActorId) -> f64 {
        match self.role(view, actor) {
            Role::Totem { owner } => self.haste_mult(view, owner),
            Role::Pet { owner, .. } => {
                self.haste_mult(view, owner)
                    * self.mod_product(
                        view,
                        &Query::holder_targeted(view, actor),
                        ModKind::HastePct,
                    )
            }
            Role::Own => {
                let rating = 1.0 + self.derived(view, actor).haste_pct / 100.0;
                rating
                    * self.mod_product(
                        view,
                        &Query::holder_targeted(view, actor),
                        ModKind::HastePct,
                    )
            }
        }
    }
}

pub(crate) fn player_seat(view: &dyn StateView, actor: ActorId) -> Option<Seat> {
    match view.actor(actor)?.kind {
        ActorKind::Player(seat) => Some(seat),
        ActorKind::Enemy { .. } | ActorKind::Pet { .. } => None,
    }
}

/// The seat behind a player or one of its pets.
pub(crate) fn owner_seat(view: &dyn StateView, actor: ActorId) -> Option<Seat> {
    match view.actor(actor)?.kind {
        ActorKind::Player(seat) | ActorKind::Pet { owner: seat, .. } => Some(seat),
        ActorKind::Enemy { .. } => None,
    }
}

pub(crate) fn instance(view: &dyn StateView, r: AuraRef) -> Option<&AuraInstance> {
    view.auras(r.holder)
        .iter()
        .find(|i| i.aura == r.aura && i.source == r.source)
}
