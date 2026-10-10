# Mechanics Roadmap

A work plan for an agent picking up combat-mechanics support in Portunus. It
covers every gap found in the October 2026 audit, ordered so that each phase
builds on the ones before it. Each item says what to add to the data, the
kernel, the mechanics and the checks, how to test it, and what "done" means.

Read the whole of section 1 before starting any phase.

---

## 1. Read this first

### 1.1 Scope

Portunus optimizes **damage dealers** in Mythic+ groups (see `HISTORY.md`).
Healing and tanking models are explicitly out of scope there. This plan
therefore has three tiers:

- **Tier A**: needed by most damage specs. Do these first, in phase order.
- **Tier B**: needed by a handful of specs or for Mythic+ realism.
- **Tier C (deferred)**: healer and tank models and other things the project
  has set aside. Listed in section 14 so nothing is forgotten, but don't
  start them without the user's say-so.

### 1.2 A correction to the earlier audit

The audit said empowered casts already work. They don't. **Channels,
empowers and shared cooldown categories are not implemented.** The data
types (`CastKind::Channel`, `CastKind::Empower`, `CooldownDef.category`), the
decision interface (`Wait::ChannelTick`, `CastOpts::empower`,
`CastOpts::tick_wakes`, `Mechanics::channel_tick`) and the trace fields exist,
but:

- `crates/engine/src/kernel/setup.rs`, `fn unsupported`, refuses any seat
  ability that is a channel or empower or has a cooldown category.
- Nothing in the kernel ever calls `Mechanics::channel_tick`.
- `PartyMechanics::channel_tick` (`crates/mechanics/src/party.rs`) is an
  empty stub.

Phase 1 fixes this. Many specs depend on it (Mind Flay, Fists of Fury,
Rapid Fire, Disintegrate, Drain Soul, Evocation, every Evoker empower).
Channels (1.1), empowers (1.2) and categories (1.3) are done.

The header doc of `crates/engine/src/kernel/mod.rs` is also stale: it still
lists auto-attacks and cast lag as unimplemented. Fix it in Phase 0 and keep
it current after every phase.

### 1.3 Ground rules

- **Lint gates. All four must pass before an item counts as done:**
  ```sh
  cargo fmt --all
  cargo clippy --workspace --all-targets      # zero warnings
  RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
  cargo test --workspace
  ```
- **Strong types.** Use enums over booleans where a third case is plausible,
  newtypes for ids, and `Option` over sentinel values. No `as` casts that can
  truncate without a comment-free reason visible in the code (use
  `u8::try_from`, `usize::from`).
- **Commits.** Only commit when the user asks. Use the trailer
  `Co-authored-by: Cursor <cursoragent@cursor.com>`, write the message to a
  file and `git commit -F` it.
- **Comments.** Match the codebase: doc comments on every public item, and
  inline comments only for constraints the code can't show. Don't narrate.

### 1.4 Architecture rules that every change must respect

- **Data first.** New behaviour should be expressible in `data/game.ron`
  through the vocabulary in `crates/gamedata`. A spec kit (`SpecKit`, a
  `HookKey` effect, or `Mechanics::gate`) is the escape hatch for true
  one-offs, not the default.
- **Mechanics hold no mutable state.** Everything that changes during a run
  lives in kernel primitives (auras, resources, cooldowns, timers, proc
  records) in `World`, so forks stay cheap and the state is observable. See
  the rule at the top of `crates/engine/src/mechanics.rs`.
- **Mechanics never call back into themselves.** `EngineIo` calls queue
  `Followup`s that `Kernel::drain` delivers after the current callback. If
  you add a kernel-side change that causes aura removals or deaths outside a
  mechanics callback, call `self.drain()` afterwards (as `start_cast` does
  for form and stealth changes).
- **Determinism.** The event queue orders by `(time, EventClass, seq)`.
  Use `BTreeMap`/`BTreeSet`, never iterate a `HashMap`. New random draws
  need their own `StreamKey`; keys 0 (crit), 1 (random effects) and 2
  (auto-attack misses) are taken in `crates/mechanics/src/interp.rs`. Take
  the next free number and add it next to those constants.
- **Observability.** Anything a player could see in game must be readable
  through `StateView` (`crates/engine/src/state.rs`) and surfaced in
  `SeatObs` (`crates/run/src/observe.rs`). Anything a player could not see
  (sampled future timings, pending spawns) must stay hidden from
  `InfoSet::Realistic` observers (`crates/env/src/observe.rs`).
- **Wakes.** If a change can make an ability usable or unusable, a waiting
  seat needs a wake (`World::notify` with a `WakeReason`). Unexpected wakes
  are subject to cast lag; expected ones are not.

### 1.5 The checklist for every new feature

1. **Data.** Add the field or variant in `crates/gamedata` with
   `#[serde(default)]` where sensible, so existing RON files still load.
   Document it with a real game example.
2. **Struct literals.** New fields break every literal. Find them with
   `rg -n "blocked_by: None" crates` (aura literals) and
   `rg -n "requires: Vec::new\(\)" crates` (spell literals) and similar.
   Known sites: `crates/gamedata/src/spell.rs` tests,
   `crates/mechanics/tests/{common/mod.rs,pets.rs,melee.rs}`,
   `crates/loadout/src/compile.rs` tests.
3. **Validation.** Check every new id reference and every value range in
   `crates/ingest/src/check.rs`, with a new `DataIssue` variant when no
   existing one fits. `crates/ingest/tests/data_files.rs` loads the real
   data and must keep passing.
4. **Hook discovery.** If you add a new effect list anywhere, walk it in
   `fn walk`'s caller in `crates/mechanics/src/kits.rs` so `Hook` keys
   inside it are found.
5. **Kernel and mechanics.** Implement. Kernel owns timing, state and
   legality; mechanics own formulas and effect semantics.
6. **Observation.** Extend `StateView`, `World`'s implementation of it, and
   `SeatObs`. If it affects legality, it must show in `ActionMask`.
7. **Action space.** If `Choice` changes, update `crates/env/src/action_space.rs`.
8. **Trace.** Add a `TraceEvent` if a human debugging a run would want to
   see it (`crates/engine/src/trace.rs`).
9. **Tests.** Kernel-only behaviour goes in `crates/engine/tests/` against
   the stub mechanics there. Semantics go in `crates/mechanics/tests/` with
   synthetic data grafted onto the real game data (copy the pattern in
   `crates/mechanics/tests/melee.rs`: `fixture()`, `druid()`, `script()`).
10. **Docs.** Update the kernel header doc and any refusal list.
11. **Gates.** Run all four.

### 1.6 Testing pitfalls already hit

- Class raid buffs leak into synthetic fixtures. Clear `data.classes` in
  every mechanics fixture.
- Scenarios vary pull health by ±20%. Pin `spec.pulls[i].health =
  Dist::Fixed(1.0)` when a test counts hits or needs an exact fight length.
- Casting a spell a form doesn't allow drops the form, and any aura with
  `ends_with` that form goes too. Synthetic druid data must list every spell
  meant to keep Prowl in Cat Form's `allows`.
- A pull-time aura gain sends an unexpected `AuraGained` wake. The dedup in
  `collect` prefers expected wakes; don't break that.
- Poison-style proc rates are noisy on one seed. Pool counts over seeds 1–5.

---

## 2. Phase 0: Housekeeping (Tier A, small)

### 0.1 Forms and stealth (done)
Committed together with this plan. Their follow-ups are 1.7 and 1.8.

### 0.2 Fix the kernel header doc (done)
`crates/engine/src/kernel/mod.rs` lines 11–14. List what is actually refused
today: shared cooldown categories (since done, 1.3), enemy adds, spells
triggered for enemies, aura value caps, thresholds and absorbs, and pet
autocast spells with no GCD, cooldown or cast time.

Note: setup only screens *seat* abilities. A pet autocasting a channel was
silently run as a hard cast with its effects at the end; Phase 1.1 fixes
that because pets share `begin_cast`.

### 0.3 Bleeds must ignore armor (done)
Confirmed in SimC: `player_t::target_mitigation` applies armor only for
`result_amount_type::DMG_DIRECT`, and `action_t::ignores_armor` comes from
the spell's "treat as periodic" attribute (set by hand on Windstrike,
Touch of Death, Shattering Throw). Implemented as `HitKind { Direct,
Periodic }` in `portunus_engine::mechanics` (carried on `RolledHit` and
`EffectCtx.hit`; aura ticks are `Periodic`, everything else `Direct`),
`Effect::Damage.ignores_armor`, and an `IncomingHit { school, kind,
ignores_armor }` argument to `CombatMath::mitigate` so Phase 10's armor
penetration can extend it. No DPS change: every enemy in `data/enemies.ron`
has 0 armor. Test: `melee.rs::armor_reduces_direct_physical_hits_but_not_bleeds`.

**Bug (original description).** `Interpreter` calls `math.mitigate` for every hit
(`crates/mechanics/src/interp.rs` around line 431), and `Formulas::mitigate`
applies armor to all physical damage, so periodic physical damage (Rend,
Rupture, Rake, Garrote, Deep Wounds) is wrongly reduced by armor.

- In SimC, armor applies only to direct physical damage
  (`action_t::calculate_direct_amount`); ticks skip it. Confirm in the SimC
  source before changing anything.
- Thread "is this periodic?" into `mitigate` (a `HitKind { Direct, Periodic }`
  enum parameter is clearer than a bool), and skip armor for `Periodic`.
- Also add `#[serde(default)] ignores_armor: bool` on `Effect::Damage` for
  direct physical hits that bypass armor in game.
- **Test:** a physical DoT and a physical direct hit of the same base amount
  against an armored enemy; the tick is unreduced, the hit is reduced by
  `armor / (armor + armor_constant)`.

### 0.4 Exhaustion and Sated persist through death (done)
`AuraDef.persists_through_death`, honoured in `World::kill`; recovery only
re-applies passives, so nothing else was needed. Only Sated (57724) exists
in the data (every lust is modelled as Bloodlust plus Sated). Test:
`movement.rs::sated_survives_death_and_recovery`. Runs where a seat dies
before a later pull no longer re-lust that seat, which is the intended fix.

`World::kill` removes every aura with `AuraRemoval::HolderDied`. In game,
Exhaustion, Sated, Temporal Displacement and Fatigued survive death, so a
death currently lets Bloodlust be reapplied early.

- Add `#[serde(default)] persists_through_death: bool` to `AuraDef`.
- In `World::kill`, skip those auras. Between-pull recovery (dead seats come
  back "with full health and their passive auras") must keep them too.
- Set it on 57724 (Sated) in `data/game.ron` and on any other lockout aura.
- **Test:** a seat with Sated dies and recovers; Sated is still there, and
  the next pull's Bloodlust is blocked.

---

## 3. Phase 1: Casting foundations (Tier A)

### 1.1 Channels (done)

Implemented as planned, with three corrections from SimC:
- **Channel ticks are periodic, not direct.** SimC assesses a channel's own
  ticks with `amount_type(state, true)`, i.e. `DMG_OVER_TIME`, so armor
  doesn't apply. `PartyMechanics::channel_tick` runs the spell's effects
  with `HitKind::Periodic`. Channels whose damage is a separate spell
  (SimC's `tick_action`: Fists of Fury, Rapid Fire) should model it as a
  `TriggerSpell` per tick, which hits as direct.
- **Swings during a channel are wasted, not paused.** SimC's
  `interrupt_auto_attack` (default on) lets a swing that comes due during a
  channel go by with no effect, keeping the timer's rhythm; hard casts do
  pause swings. Some channels opt out (Celestial Conduit sets
  `interrupt_auto_attack = false`; SimC models Bladestorm as not
  channeled), so `CastKind::Channel` gained `#[serde(default)] swings:
  bool`.
- **The cast succeeds at the start.** Costs, cooldown, `last_cast`, and
  `Mechanics::cast_completed` (so `CastComplete` and `ResourceSpent`
  listeners) happen as the channel starts, as the game's
  `SPELL_CAST_SUCCESS` does; `PartyMechanics::cast_completed` skips a
  channel's effects. Documented on `ListenFor::CastComplete` and
  `Mechanics::cast_completed`.

Other details: tick `i` lands at `start + round(duration * i / ticks)`, so
the last lands exactly at the end; haste and duration are fixed at the start
(SimC drops `STATE_HASTE` from channels' update flags). `Casting` gained
`channel: Option<ChannelState>`; `CastOpts` now reaches `begin_cast` (pets
pass the default). `CastView` gained `ticks: Option<ChannelProgress>` and a
filled `next_tick`; `SeatObs.channel: Option<ChannelObs>` exposes them to
scripts. `TraceEvent::ChannelTick` records each tick. Ingest reports
`DataIssue::InvalidChannel` for zero ticks or zero duration. Pet channels
now really channel (no pet in the data has one). No DPS change: no spell in
`data/game.ron` is a channel. Tests: `crates/engine/tests/channel.rs`, three
`melee.rs` channel tests, `run_file.rs::seats_see_their_channels_progress`,
`data_files.rs::channels_without_ticks_or_duration_are_reported`.

**Game rules (verify against SimC `action_t` channel handling):**
- A channel has a duration and a tick count; ticks are evenly spaced. Haste
  is read once at the start: both the duration and tick spacing are hasted
  if `hasted`, and later haste changes don't affect a channel in progress.
- Effects run on each tick, not at completion.
- Channels trigger the GCD at the start like any cast.
- Moving cancels a channel unless the spell is castable while moving
  (`castable_while_moving` or a `CastWhileMoving` modifier).
- The player can stop early (`Choice::StopCast`); ticks already dealt stay.
  Re-casting the same channel clips the current one ("chaining").
- Auto-attacks pause during a player's channel, as with hard casts.

**Kernel (`crates/engine/src/kernel/mod.rs`, `cast.rs`, `queue.rs`):**
- Remove the channel refusal from `setup.rs::unsupported`.
- In `begin_cast`, when `CastKind::Channel`, record
  `Casting { ev, ends, seq, channel: Some(ChannelState { ticks, period,
  next_tick, index }) }` (extend `Casting` in `world.rs`).
- New `Event::ChannelTick { actor, seq, index }` in the `World` event class.
  Its handler checks the actor is still casting the same `seq`, calls
  `m.channel_tick(io, &ev, index)`, schedules the next tick, and sends a
  `WakeReason::ChannelTick` to the seat if it asked (`CastOpts::tick_wakes`
  or a pending `Wait::ChannelTick`).
- On the last tick, end the channel: record `CastEnd`, clear `casting`,
  resume swings, settle the seat's phase. Channels don't call
  `cast_completed` at the end (decide: SimC fires `last_tick`; a
  `ListenFor::CastComplete` listener should probably fire on channel *start*
  in game terms, because that's when the cast "succeeds". Check what SimC
  and the game do for `CastComplete`-style procs on channels and document
  the choice on `ListenFor::CastComplete`).
- Costs: paid at channel start.
- `Wait::ChannelTick` is currently rejected with `IllegalChoice::NoChannelTick`
  whenever chosen. Make it legal while channeling.
- `StopCast` during a channel: record `CastEnd` with a `Stopped` reason, keep
  ticks dealt, clear casting.
- Forced movement and voluntary `Move` cancel the channel through the
  existing "a cast that can't continue while moving stops" path.
- Stealth already breaks at the start of non-instant hostile casts
  (`start_cast`); channels go through that path. Keep it.

**Mechanics (`crates/mechanics/src/party.rs`):**
- Implement `channel_tick`: run the spell's `effects` with the cast's
  context (target, spell, caster), with `scale` from any spend scaling.
  Damage listeners see each tick as a hit from that spell.
- Decide whether channel ticks are `DamageDealt` with `spell: Some(id)` (yes)
  and whether they count as "periodic" for armor (no: channel ticks are
  direct damage in SimC; verify).

**Observation:** `CastView` (`state.rs`) gains `ticks_done` and
`ticks_total`. Realistic observers may see these (a cast bar shows them).

**Tests (`crates/engine/tests/combat.rs` or a new `channels.rs`):**
- A 3 s, 3-tick channel deals three ticks at 1 s, 2 s, 3 s.
- With 50% haste at start, ticks land at 0.667 s intervals; a haste change
  mid-channel doesn't move them.
- `StopCast` after the second tick keeps two ticks.
- Re-casting the same channel mid-way clips it and starts fresh.
- Forced movement cancels a channel that isn't castable while moving.
- Swings pause during the channel and resume at its end.
- `Wait::ChannelTick` and `tick_wakes` wake the seat after each tick.

### 1.2 Empowers (done)

Verified in SimC's Evoker module (`empowered_charge_t`): the charge is a
channel, so costs and cooldown are paid as it starts; haste (and here
`CastTimePct`) scales stage times and the hold alike, fixed at the start;
`last_tick` releases a separate spell at the stage reached, skipping it at
`EMPOWER_NONE` (stage 0 fizzles, costs already paid) and restarting the GCD
(`start_gcd`). SimC's `dot_t::cancel` also runs `last_tick`, so an empower
cut short by movement goes off at the stage reached, same as `StopCast`;
that's what the kernel does (`World::stop_cast`, a new
`Followup::Released`). Death and combat end don't release. SimC has no
"stage effects": the release spell reads the stage in its formulas, so the
data's `effects` then `stage_effects[n - 1]` order is ours to pick.

Implementation: `Casting.empower: Option<EmpowerState>` with the haste
snapshot as a `CastScale`; `Event::EmpowerStage` and
`WakeReason::EmpowerStage(n)` (sent when `CastOpts::tick_wakes` is set,
reused rather than adding an option); the hold runs out through the
existing `CastComplete` event. `CastOpts::empower` clamps to the spell's
stages. `cast_time_of` now reports an empower's full charge time, so
movement and stealth treat it as a non-instant. `ListenFor::CastComplete`
fires on release. `CastView.empower_stage`/`next_stage_at` are filled;
`SeatObs.empower: Option<EmpowerObs>`. `TraceEvent::EmpowerStage`.
`DataIssue::InvalidEmpower` covers stage times that aren't positive and
strictly increasing, and stage effects that don't match the stages. No
DPS change: no spell in the data is an empower. Tests:
`crates/engine/tests/empower.rs`, `melee.rs::empowers_pay_as_they_start_and_add_their_stage_on_release`,
`run_file.rs::seats_see_their_empowers_progress`,
`data_files.rs::malformed_empowers_are_reported`.

**Game rules:** charge through stages (cumulative times, hasted if
`hasted`); release on reaching a chosen stage, on `StopCast` (fires the
stage reached), or automatically after holding the final stage for `hold`.
Stage 0 (released before stage 1) fizzles in game: verify and document.
Effects: the spell's own `effects` plus `stage_effects[n - 1]`. Moving
cancels unless castable while moving (Hover via `CastWhileMoving`).

**Kernel:**
- Remove the empower refusal.
- `Casting` gets `empower: Option<EmpowerState { stage_at: Vec<SimTime>,
  release_at: Option<SimTime>, auto_release: SimTime }>`.
- `Event::EmpowerStage { actor, seq, stage }` to wake the seat
  (`WakeReason::EmpowerStage(stage)`, new) and to auto-release when
  `CastOpts::empower == Some(stage)`.
- On release, call `cast_completed` with `CastEvent.empower = Some(stage)`.
- `CastView.empower_stage` already exists; fill it.

**Mechanics:** in `cast_completed`, when `ev.empower` is set, run
`stage_effects[stage - 1]` after the base effects (or instead: check SimC's
Evoker module for Fire Breath and Eternity Surge).

**Validation:** `stages` non-empty and strictly increasing; `stage_effects`
length equals `stages` length (some of this may exist; check `check.rs`).

**Tests:** release at stage 2 by option; release early with `StopCast` at
stage 1; auto-release after the hold; haste shortens stage times; stage
effects run once; moving cancels unless a `CastWhileMoving` modifier is up.

### 1.3 Shared cooldown categories (and potions) (done)

Done as planned. `CooldownKey { Spell, Category }` (in `world.rs`) keys
`Actor.cooldowns` and `Event::CooldownReady`; `World::cooldown_key` resolves
a spell to its key, so readiness, `AdjustCooldown`, `CooldownView` and
conditions all see the shared cooldown. A cooldown regained by an
adjustment wakes the seat once for every ability on it. A full category
cooldown starts with the modifiers of the spell that spent it.

Potions, checked against SimC: `potion_t` (`engine/player/consumable.cpp`)
uses the player's shared "potion" cooldown, with its duration from the item
effect's cooldown group (or the spell's category cooldown), started on use
like any action's. Nothing waits for combat to end, so there's no
`starts_after_combat`. Documented on
`CooldownDef.category`. `DataIssue::CategoryMismatch` also checks `hasted`.
No DPS change: no spell in the data has a category. Tests:
`crates/engine/tests/cooldown.rs`,
`melee.rs::adjusting_one_spells_category_cooldown_adjusts_them_all`,
`data_files.rs::categories_whose_spells_disagree_are_reported`.

**Game rules:** spells in a category share one cooldown (and charges).
Examples: potions (shared 5-minute cooldown across all combat potions),
some trinkets, Shaman shock spells in older versions. Verify the current
potion rules (whether the cooldown only starts after leaving combat, or
potions are limited to one per combat) and implement what the game does.

**Kernel (`cast.rs`, `world.rs`):**
- Remove the refusal.
- Cooldowns are keyed by spell today. Introduce a `CooldownKey { Spell(SpellId),
  Category(u32) }` and key the cooldown map by it; a spell with a category
  uses `Category`.
- All readiness checks, `AdjustCooldown`, `CooldownView`, `Scalar::Cooldown*`
  and wakes (`Event::CooldownReady`) go through the key.
- If potions need "starts after combat", add
  `CooldownDef.starts_after_combat: bool` and start the cooldown in
  `end_combat`.

**Validation:** spells in the same category must agree on duration and
charges (new `DataIssue::CategoryMismatch`).

**Tests:** two spells in one category: casting one puts both on cooldown;
`AdjustCooldown` on one affects both; charges are shared.

### 1.4 Strikes with both weapons (done)

Done as planned, plus two fixes found on the way. `Effect::Damage.hand`
sets `EffectCtx.hand` for that hit (`Interpreter::striking`); with no weapon
in that hand the hit is skipped. `RolledHit.weapon` (new) carries the
strike's hand, so `WeaponHit` listeners fire for it on landing, including
for a travelling strike. The fixes: `Coefficient::WeaponDamage` always used
the main hand; it now uses the weapon in play (`EffectCtx.hand`, else the
main hand), like `WeaponSpeed`. And a spell's own `weapon` now sets the
cast's `EffectCtx.hand`, so an off-hand strike scales with the off hand.
No DPS change: no spell in the data has a `weapon` or uses
`WeaponDamage`. Nothing to validate, observe, or trace beyond the existing
damage records. Test:
`melee.rs::strikes_with_both_hands_hit_with_each_weapon`.

**Game rules:** Stormstrike, Mutilate, Windstrike, Dual Strike etc. hit
with the main hand and the off hand separately, each a melee hit that
triggers on-hit effects for its own hand (poisons, Windfury).

**Data (`crates/gamedata/src/effect.rs`):**
- Add `#[serde(default)] hand: Option<WeaponHand>` to `Effect::Damage`. When
  set, the hit uses that hand's weapon for `Coefficient::WeaponDamage` and
  `WeaponSpeed`, and counts as a `WeaponHit` for that hand, regardless of
  the spell's own `weapon`.
- Keep `SpellDef.weapon` for single-hand strikes.

**Mechanics (`interp.rs`):** set `EffectCtx.hand` per effect from the
effect's `hand`; fire `Happening::WeaponHit { hand }` per hit. With no off
hand (a two-hander, or in Cat Form), an off-hand effect does nothing.

**Tests:** a spell with an MH and an OH damage effect deals two hits, each
scaled to its weapon, and a main-hand-only poison procs only off the MH hit.

### 1.5 Swing-timer interactions (Tier B)

- `Effect::ResetSwing { hand: Option<WeaponHand> }`: restart that hand's
  timer from now (abilities that reset the swing).
- Parry haste is irrelevant: players hit from behind. Document that in
  `StatCurves::dual_wield_miss_pct`'s neighbourhood.
- **Test:** reset at mid-swing; the next swing is a full interval later.

### 1.6 Channels that change mid-channel (Tier B, after 1.1)

- Mind Flay → Mind Flay: Insanity is an aura override at cast start; it
  already works through `overrides` once channels exist.
- A buff that changes a channel already in progress (rare): leave to kits.
- Haste is locked at channel start (1.1). Document.

### 1.7 Forms: follow-ups

- **Form GCDs.** Energy users in a form (Cat Form) use a 1.0 s unhasted
  GCD. Add `#[serde(default)] gcd: Option<GcdDef>` to `FormDef`; while the
  form is held, it replaces the GCD of spells that trigger one. Read in
  `World::spell_gcd`.
- **Shapeshift on pets:** not needed. Document that forms are player-only.
- **Separate resources per form** (Bear's rage, Cat's energy): both
  resources are declared on the spec; generation is data. Nothing to do
  beyond rage decay (Phase 3.4).
- **Shadowform, Metamorphosis, Voidform, Moonkin Form:** forms with
  `allows_all: true` (plus modifiers). Metamorphosis also replaces spells
  through `overrides`. Nothing new needed; add one test of an `allows_all`
  form if not covered.

### 1.8 Stealth: follow-ups

- **Shadowmeld:** a stealth aura castable in combat that drops combat for
  the caster in game. Without threat, treat it as "stealth in combat":
  nothing new needed.
- **Vanish:** a timed stealth aura whose `on_expire` applies Stealth. Add a
  test: Vanish expires into Stealth, and an Ambush requiring
  `AnyAura([STEALTH, VANISH])` is usable throughout.
- **Stealth-only snapshots (Garrote, Rake from Prowl):** `PersistentPct`
  modifiers on the stealth aura snapshot into the DoT's `pmultiplier` at
  application. The instant opener applies the DoT before stealth breaks
  (stealth breaks after `cast_completed`), so this already works; add a
  test that a DoT applied from Prowl keeps its bonus for its whole
  duration.
- **Subterfuge-style lingering:** done via `on_break`.

---

## 4. Phase 2: Aura values (caps, thresholds, absorbs) (Tier A)

Unblocks Seed of Corruption, Ignite-style caps, "after spending X" counters,
boss absorb shields (damage dealers must break them), and player absorbs
(survival in Mythic+).

### 2.1 Why the kernel can't do this alone today

`AuraValue.cap` and `threshold` are `Coefficient`s (attack power, spell
power, max health), which only mechanics can evaluate.
`World::add_aura_value` refuses any aura with a cap, threshold or absorb
(`crates/engine/src/kernel/aura.rs` around line 485).

### 2.2 Design

- **Limits computed by mechanics, enforced by the kernel.** Change
  `EngineIo::add_aura_value(aura, delta)` to
  `add_aura_value(aura, delta, limits: ValueLimits)` with
  `ValueLimits { cap: Option<f64>, threshold: Option<f64> }`. The
  interpreter evaluates the coefficients with the aura's source as caster
  (Seed's threshold uses the warlock's spell power) and passes them.
- **Cap:** clamp after adding.
- **Threshold:** while `value >= threshold`, subtract `threshold` and queue
  `Followup::Threshold(AuraRef)` once per crossing. Mechanics handle it by
  running `on_threshold` with the aura's context. Guard against a zero or
  negative threshold (validation, and a runtime `unsupported`).
- **Absorbs:** in `World::damage`, before reducing health, walk the target's
  `Absorb` auras whose school mask intersects the hit's school, in a
  defined order, subtracting from each value. Remove each emptied one with
  a new `AuraRemoval::Depleted`. Return the post-absorb amount (the
  `EngineIo::apply_damage` doc already promises this).
  - **Order:** check SimC's `absorb_buff_t` priority rules; default to
    oldest-first and add `AuraDef.absorb_priority: i8` only if SimC needs it.
  - **Applying an absorb:** `ApplyAura` creates the instance;
    `AddAuraValue` sets the shield size. Consider a convenience
    `Effect::Shield { aura, amount, target }` that does both.
  - **Trace:** `TraceEvent::Absorbed { target, aura, amount }`.
  - **Outcome:** add `absorbed` to the seat's outcome. For enemies, damage
    into a shield does not reduce health, so the fight lasts longer; decide
    whether `damage_done` counts it (SimC counts absorbed damage as damage
    done; keep `damage_done` as health removed plus absorbed and add a
    separate `health_removed` if the reward needs it). Check
    `crates/env/src/reward.rs` and keep the reward consistent.
- **Listeners:** `DamageTaken` and `DamageDealt` see the post-absorb amount
  plus a new `absorbed` field on the occurrence; add
  `ListenFor::Absorbed { aura: Option<AuraId> }` (Phase 6).

### 2.3 Validation
- Absorb, cap and threshold coefficients must be positive.
- `on_threshold` effects are checked like other effect lists (already).

### 2.4 Tests
- Counter with threshold 100: adding 250 fires `on_threshold` twice and
  leaves 50.
- Cap clamps.
- A 500-point absorb on an enemy eats the first 500 damage, then is removed
  as `Depleted`; school filtering works.
- **Seed of Corruption end to end:** a debuff whose holder-side
  `DamageTaken` listener adds `EventAmount(1.0)` to its value; threshold is
  `SpellPower(x)` of the source; `on_threshold` deals AoE damage and
  removes the aura.
- A boss with a `SelfAura` absorb (enemy rule) takes longer to kill by
  exactly the shield size, with health variation pinned.

---

## 5. Phase 3: Conditions and resources (Tier A)

### 3.1 More predicates

`Predicate` (`effect.rs`) is `Copy`; keep it that way (no boxed
combinators). Add explicit variants:

| Variant | Example |
|---|---|
| `TargetHpAbove(f64)` | Careful Aim, Firestarter |
| `CasterHpBelow(f64)`, `CasterHpAbove(f64)` | defensive talents |
| `TargetStacksAtLeast { aura, stacks, from_self }` | "at 5 stacks of Festering Wound" |
| `CasterStacksAtLeast { aura, stacks }` | stack-gated effects |
| `AuraValueAtLeast { aura, value }` | counters |
| `TargetHpBelowCasterMaxHp` | Touch of Death |
| `RecentCasts { spell, count }` | Steady Focus (two Steady Shots in a row) |

- `RecentCasts` needs history: keep the seat's last few completed casts
  (a fixed array, e.g. `[Option<LastCast>; 4]`) in `SeatState`, updated
  where `last_cast` is updated. Expose via `StateView::recent_casts(seat)`.
- Evaluate in the mechanics predicate evaluator (find where
  `Predicate::TargetHpBelow` is matched in `crates/mechanics/src`) and in
  `kernel/condition.rs` if the kernel evaluates predicates anywhere.
- **Tests:** one per variant, through `Effect::If` and a conditional
  `Modifier`.

### 3.2 More spell requirements

Extend `Requirement` (`spell.rs`), evaluated in
`World::requirements_met` (`kernel/cast.rs`):

- `TargetHpBelow(f64)` and `TargetHpAbove(f64)` (Execute, Kill Shot,
  Hammer of Wrath when not proc-enabled).
- `CasterStacksAtLeast { aura, stacks }`.

**Wakes:** a waiting seat must learn when the target crosses a health
threshold. In `World::damage`, if any seat has an ability with a health
requirement on that target and the hit crossed the threshold, notify an
unexpected wake (`WakeReason::TargetHealth`, new). Precompute per seat the
set of thresholds at setup (`Statics`) so this is cheap.

**Tests:** Execute blocked above 20%, legal below; a seat idling with
`Wait::Until(far)` is woken when the target crosses 20%.

### 3.3 Death Knight runes

**Game rules (verify in SimC `death_knight.cpp`):** six runes; up to three
recharge at once; each takes 10 s, hasted; spending takes ready runes;
Runic Power comes from spending (data, via `ResourceSpent(Runes)`
listeners or a spell's resource effect).

**Data:** `ResourceDef.recharge: Option<RechargeDef { concurrent: u8,
period: SimDuration, hasted: bool }>`. With `recharge`, `regen_per_sec`
must be 0 (validation).

**Kernel:** resources currently evolve linearly between events. A
recharging resource needs its own representation:
- `RuneState { ready: u8, recharging: Vec<SimTime /* ready_at */> }`, with
  at most `concurrent` entries recharging and the rest queued.
- Value at time t = `ready` + number of `ready_at <= t`. Settle lazily like
  linear resources.
- Haste changes rescale the remaining time of recharging runes (like
  `rescale_swings`).
- `Wait::Condition` on `Scalar::Resource(Runes) >= n`: solve exactly as the
  time the n-th rune is ready.
- Readiness for a spell costing runes: `need.until(time n runes ready,
  WakeReason::ResourceReady)`.
- `ResourceView` gains `next_ready: Vec<SimTime>` for observers.

**Tests:** spend 2 of 6 → two recharge, ready 10 s later; spend 6 → three
recharge at once, the other three start as those finish; 30% haste
shortens to 7.69 s; a haste change halfway rescales the rest; a
`Wait::Condition` for 3 runes wakes at the right time.

### 3.4 Other resource behaviour

- **Durations scaled by resource spent** (Rupture, Slice and Dice, Kidney
  Shot by combo points): add `#[serde(default)] per_unit_spent:
  Option<SimDuration>` to `Effect::ApplyAura`. Duration = base (or override)
  + `per_unit_spent * spent`. The amount spent is already in
  `CastEvent.spent`; carry it into `EffectCtx` (check whether `scale`
  already holds it, and add an explicit `spent: Option<f64>` if not).
  **Test:** Rupture at 3 and 5 combo points lasts 4 + 4·cp seconds.
- **Rage decay out of combat:** `ResourceDef.out_of_combat: Option<f64>`
  (regen per second outside combat, negative for decay; also covers
  Insanity, Fury, Runic Power decay). Between pulls matters in Mythic+.
  **Test:** rage drains during travel and stops at 0.
- **Mana-limited specs:** verify `Cost` can express percent-of-base-mana
  costs (it has absolute amounts plus `CostPct` modifiers). If not, add
  `Cost.pct_of_base: bool` with base mana on the spec. Evocation is a
  channel (Phase 1.1) restoring mana per tick. **Test:** a mana-limited
  script runs out and waits for regen.
- **Combo points carried between finishers, charged combo points**
  (Echoing Reprimand style): kit hooks. No generic work.

---

## 6. Phase 4: Multiple targets (Tier A)

### 4.1 Enemy adds (`EnemyAction::SpawnAdds`)

**Today:** refused at setup (`setup.rs::unsupported`, "enemy adds") and at
runtime (`rules.rs`, `SpawnAdds` arm).

**Design:**
- Actors can already be created mid-run (pets, `kernel/pet.rs`). Adds are
  enemies created by a rule.
- `ActorKind::Enemy { combat, spawn: SpawnIndex }` indexes the resolved
  spawn list. Give adds indices past the resolved spawns and keep a
  per-combat `World.adds: Vec<AddSpawn { enemy: EnemyKey, max_health: u64,
  forces: u32, spawner: ActorId }>`. Write one helper
  `World::spawn_info(actor) -> SpawnInfo` that every caller uses so the two
  sources don't leak into the rest of the code.
- **Health:** the add's `EnemyDef.health` times the pull's health
  multiplier (already drawn per pull; store it in the combat state).
- **Distance:** each seat's distance to the spawner (or a new
  `SpawnAdds.distance: Option<Dist<f64>>`).
- **Engagement:** engaged at once; its rule clocks start at spawn.
- **Combat end:** a combat ends when every enemy is dead, adds included.
  Add `#[serde(default)] despawn_with_spawner: bool` on `SpawnAdds` for
  adds that vanish when the boss dies.
- **Mechanics armor lookup:** `Formulas` precomputes `enemy_armor` by
  `(combat, spawn)` (`crates/mechanics/src/math.rs` around line 110). Change
  it to look up armor by the actor's `EnemyKey`. Add
  `StateView::enemy_key(actor) -> Option<&EnemyKey>`.
- **Observation:** a realistic observer sees an add once it exists, never
  before. Check `TimelinePriors` (`crates/scenario`) for whether priors
  should include expected add waves.
- **Action space:** `ActionSpace` indexes retarget choices into the mask's
  `targets` list; make sure a larger target list is handled (cap or error
  clearly).
- **Plans:** `crates/plan` anchors to rule names; adds' rules count too.

**Validation:** `SpawnAdds` keys exist and are `EnemyKind::Add`.

**Tests (`crates/engine/tests/`):** a boss rule spawns 2 adds at 10 s; they
take damage, can be targeted, run their own rules, and the combat ends only
when all die; `despawn_with_spawner` adds disappear when the boss dies;
an add's armor reduces physical damage.

### 4.2 Enemy-death triggers

- `ListenFor::EnemyDied { killed_by_self: bool, had_aura: Option<AuraId> }`:
  fires on the listener holder's auras when an enemy dies. Event target =
  the dead enemy, so effects at `Target` do nothing, but effects at
  `Caster`, `AllEnemies` or `EnemiesWithAura` work (shard on kill, execute
  refunds, Shadow Word: Death's cooldown reset).
- `DamageDealt` gains `#[serde(default)] killing_blow: bool` as a filter.
- Mechanics: `actor_died` fires a new `Happening::EnemyDied` to every seat
  (and pet) holder whose auras listen.
- **Tests:** a listener that refunds a cooldown on a killing blow; one that
  grants a resource whenever an enemy with my DoT dies.

### 4.3 Target sets

New `EffectTarget` variants (`effect.rs`), resolved in `Interpreter::targets`
(`interp.rs` around line 243):

- `EnemiesWithAura { aura, from_self: bool }` (Malefic Rapture).
- `OtherEnemies` (every engaged enemy except the context target: damage
  copying, cleave).
- `AlliesWithAura { aura, from_self: bool }` (Atonement-style; Tier C use).

Counting:
- `Effect::ForEach { target: EffectTarget, then: Vec<Effect> }`: run `then`
  once per resolved target with `ctx.target` set to it.
- On `Effect::Damage`: `#[serde(default)] per_count: Option<CountScale {
  count: TargetCount, pct: f64 }>`, multiplying by `1 + pct/100 * count`,
  where `TargetCount` is `EnemiesWithAura { aura, from_self }` or
  `TargetsHit` (Mark of the Crane, Meat Cleaver).
- `ApplyAura.stacks_per_target_hit: bool` for "a stack per enemy hit" if a
  real spell needs it (Meat Cleaver).

**Validation:** aura ids in the new variants exist. Walk `ForEach.then` in
`kits.rs` and `check.rs`.

**Tests:** Malefic Rapture hits only enemies with my debuff; Mark of the
Crane scaling with 0, 1 and 3 marked enemies; `ForEach` runs per target.

### 4.4 One target per caster

`AuraDef.unique_per_source: bool` (Unstable Affliction in some versions,
Hunter's Mark): applying it removes the same caster's instance from any
other holder. Implement in `apply_aura_now` before the new instance is
pushed (scan enemies for the same `(aura, source)`).
**Test:** apply to A then B; A loses it.

### 4.5 Damage copying (Havoc, Blade Flurry, Sweeping Strikes)

Expressible after 4.3: a `DamageDealt` listener on the buff with
`Coefficient::EventAmount(share)` and target `OtherEnemies` (with an
`AoeRule.max_targets`), or `EnemiesWithAura(HAVOC)`.
- Must not copy its own copies: give the copy effect a distinct spell
  identity (`TriggerSpell` of a "copy" spell) and filter the listener with
  `spell: Some(...)` exclusions, or rely on the interpreter's listener
  `depth` guard. Write down which one and test it.
**Tests:** Blade Flurry copies 35% of single-target damage to up to 4
others and never copies a copy.

### 4.6 Spreading auras

`Effect::SpreadAura { aura, to: EffectTarget, max: Option<u8> }`: copies
the context target's instance (remaining duration, stacks, `pmultiplier`)
to targets that lack it. Disease spread, Contagion-style spells.
**Test:** spread copies remaining time and snapshot, and skips targets that
already have it.

### 4.7 Enemy positions within a pack (Tier B)

Today each enemy has one distance per seat. Cleave geometry ("enemies near
the target", chains bouncing by proximity, ground-effect radius) needs
relative positions.

- Minimal model: each enemy gets a 1-D `offset` within its pack
  (`WaveSpec.spread: Option<Dist<f64>>`, sampled per spawn). "Within r
  yards of enemy X" = `|offset_a - offset_b| <= r`.
- `EffectTarget::NearTarget { yards }`.
- `Reposition` moves the enemy's offset too.
- Keep chain spells (Chain Lightning) on `AoeRule.max_targets`; proximity
  bouncing is not worth modelling.
- **Tests:** NearTarget picks only the enemies within range.

---

## 7. Phase 5: Placed ground effects (Tier A for several specs)

Death and Decay, Consecration, Rain of Fire, Sigils, Efflorescence,
Starfall's placement variants, totems that stay put.

**Today:** ground effects are caster auras whose periodic effects hit
`AllEnemies`; anything that cares whether you "stand in it" checks the
caster aura. Nothing knows where the effect is.

**Design (depends on 4.7 for enemy positions):**
- `AuraDef.ground: Option<GroundDef { radius: f64 }>`.
- When applied, record an **anchor** in the aura instance: the targeted
  enemy's offset (or the caster's position if self-cast).
- Periodic effects with target `AllEnemies` on a ground aura only hit
  enemies within `radius` of the anchor.
- **Caster standing in it:** track the seat's movement since placement.
  Movement is 1-D and undirected, so approximate: the caster is inside
  while total yards moved since placement ≤ `radius`. Add
  `Predicate::InOwnGround(AuraId)` for "while standing in your Death and
  Decay" modifiers.
- Enemies that `Reposition` leave the area.
- Sigils: a ground aura with a short duration whose `on_expire` does the
  damage (the `EngineIo::schedule` doc already recommends this).
- Totems that stay put: `PetKind::Totem` with a ground passive aura.

**Observation:** expose anchors and the "inside" flag.

**Tests:** enemies outside the radius aren't hit; the caster leaving the
area loses its standing-in buff; a repositioned enemy leaves the area.

---

## 8. Phase 6: More events to react to (Tier A/B)

Add `ListenFor` variants (`effect.rs`), fire them from the right place, and
add the matching `Happening` in mechanics:

| Variant | Fired from | Example |
|---|---|---|
| `CastStart { spell, school }` | `begin_cast` → `cast_started` | cast-start procs |
| `EnemyDied {..}` | `actor_died` (4.2) | shards on kill |
| `ResourceGained(kind)` | `add_resource` with positive delta | "whenever you gain Holy Power" |
| `Absorbed { aura }` | `World::damage` absorbs (2.2) | absorb-consumed procs |
| `Interrupted` | `EngineIo::interrupt` returning true | interrupt talents |
| `MoveStart`, `MoveEnd` | movement start/stop in `movement.rs` | movement buffs |
| `HealthBelow(f64)` on self | `World::damage` crossing | low-health triggers |
| `AuraStacksReached { aura, stacks }` | `changed()` in `aura.rs` | stack-threshold procs |
| `AuraRemovedBy { aura, reason }` | `aura_removed` | consumed vs expired |

- `AuraRemovedBy.reason` needs a data-facing removal enum
  (`Expired | Removed | Depleted | Broken`), mapped from `AuraRemoval`.

### 6.1 Cheating death (Tier B; damage dealers die in Mythic+)

Cauterize, Cheat Death, Ardent Defender-style.
- `AuraDef.prevents_death: Option<PreventDeath { heal_to_pct: f64,
  lockout: Option<AuraId>, on_prevent: Vec<Effect> }>`.
- In `World::damage`, when a hit would kill a player holding such an aura
  (and lacking the lockout), set health to `heal_to_pct` of max instead,
  apply the lockout and queue `on_prevent`.
- **Test:** lethal hit leaves the seat alive once; the second lethal hit
  during the lockout kills.

### 6.2 Rolling windows (Tier B)

Death Strike heals for a share of damage taken in the last 5 s.
- Kernel keeps a per-player ring buffer of `(time, amount)` damage taken,
  pruned to the longest window any spell uses (precomputed in `Statics`).
- `Coefficient::RecentDamageTaken { window: SimDuration, share: f64 }`.
- Expose the window total in `StateView` (a player can see recent damage).
- **Test:** hits at −6 s, −3 s, −1 s; a 5 s window sums the last two.

### 6.3 Standing still (Tier B)

`Predicate::Stationary { for_at_least: SimDuration }` from the seat's
movement state, and `MoveStart`/`MoveEnd` listeners for buffs that build
while standing still or break when moving.

---

## 9. Phase 7: Pets (Tier B)

### 7.1 Staggered summons
`Effect::Summon` gains `#[serde(default)] stagger: SimDuration`: summon one
copy every `stagger` until `count` (Army of the Dead). Implement with a
kernel event `Event::SummonNext { owner, pet, left, duration }`.
**Test:** 8 ghouls over 4 s at 0.5 s steps.

### 7.2 Pets that leave when their resource runs out
Wild Imps cast until their energy is gone. Add
`PetDef.expires_when_empty: Option<ResourceKind>`; when that resource hits
zero after a spend, the pet expires (as a timeout, firing `Departed` and
`PetExpired`).
**Test:** an imp with 100 energy casting a 20-energy bolt casts five times
and departs.

### 7.3 Pet autocast spells with no GCD, cooldown or cast time
`kernel/pet.rs` refuses these because the pet would spin at one timestamp.
Allow them when the spell has a resource cost the pet can't regenerate
instantly, or require `PetDef.autocast_interval: Option<SimDuration>` as a
minimum gap. Keep refusing the truly unbounded case.
**Test:** an instant, free, no-GCD spell with an interval casts once per
interval.

### 7.4 Clone pets (Storm, Earth, and Fire)
Ancestors already echo the owner's casts through `CommandPet` listeners.
Model the spirits as guardians with listeners on the owner's
`CastComplete` that command them to cast the same spell (data). Verify
damage scaling (spirits deal a fixed share) via `GuardianDamagePct`. Add
a test; generic work only if something can't be expressed.

### 7.5 Pet health and positioning (Tier C)
Nothing damages pets and they always reach their target. Defer.

---

## 10. Phase 8: Enemy side (Tier B)

### 8.1 Untargetable phases
`AuraDef.untargetable: bool` on enemy auras (applied by `SelfAura`). While
held, the enemy is not a live target (`World::is_live_target`), seats
targeting it are woken to retarget, and auto-attacks stop. DoTs keep
ticking unless the enemy is also `Immune`.
**Test:** boss goes untargetable 10–20 s; casts at it are illegal; swings
resume after.

### 8.2 Boss absorb shields
Covered by Phase 2 absorbs on enemies plus `EnemyAction::SelfAura`. Add a
scenario in `data/scenarios/` with a shield phase once Phase 2 lands.

### 8.3 Crowd control on enemies (Mythic+)
Stuns stop casts and delay enemy rules; most bosses are immune.
- `Effect::Stun { duration, target }`: cancels an enemy's cast like an
  interrupt (even if not interruptible), and pauses its rule clocks for the
  duration.
- `EnemyDef.cc_immune: bool` (bosses).
- Diminishing returns: track per-enemy DR category state in `World`
  (100% → 50% → 25% → immune, reset after 18 s; verify current numbers).
**Tests:** a stun cancels a non-interruptible cast; DR halves the second
stun; bosses ignore it.

### 8.4 Dispels, purges and soothes
- `AuraDef.dispel: Option<DispelType>` (`Magic | Curse | Poison | Disease |
  Enrage`).
- `Effect::Dispel { target, types: Vec<DispelType>, count: u8 }` removes up
  to `count` matching auras (newest first; verify).
**Tests:** a purge removes a magic buff from an enemy; soothe removes an
enrage; nothing happens without a matching type.

### 8.5 Spells cast by enemies from game data (Tier C)
`EnemyAction::Effects` covers most needs. Implementing
`trigger_spell` for enemies (refused in `kernel/mod.rs` around line 679)
can wait.

---

## 11. Phase 9: Group play (Tier B)

### 9.1 Bloodlust as a decision
Today Bloodlust is an `on_pull` class provision applied automatically.
- Add a run-level setting `BloodlustMode { Auto, Policy }` (in `RunSetup`
  or `Externals`). In `Policy` mode, drop the automatic `on_pull` entry
  for Bloodlust and give the providing seat's kit the spell; the policy
  decides when.
- Keep `Auto` the default so existing runs don't change.
**Test:** in `Policy` mode, nothing applies Bloodlust until a seat casts
it; the whole party gets it; Sated blocks a second cast.

### 9.2 Buffing other players
Power Infusion, Innervate, Blessing of Summer, Source of Magic.
- Ally targeting exists (`Targeting::Ally`, `TargetSel::Actor`). Check the
  action space can address allies, and add a scripted policy rule to the
  run scripts that gives Power Infusion to the highest-damage ally.
- Observation: allies' auras and cooldowns visible (they are in game).

### 9.3 Augmentation Evoker
- **Stat sharing** (Ebon Might, Prescience): `ModKind::StatShareFromSource
  { stat, pct }` on the buff, whose value comes from the aura *source's*
  stat. Evaluate in `Formulas::stat_block` without recursion (read the
  source's base stats, not its shared-in stats).
- **Damage credit:** the extra damage allies deal because of the Evoker's
  buffs should be credited to the Evoker, as SimC does. Track per damage
  event the share attributable to each buff source and add it to that
  seat's `outcome.damage_contributed`. Large; do last.

### 9.4 Stamina changing health mid-run
Power Word: Fortitude or a Stamina proc can't change max health after
setup. Add `EngineIo::set_max_health(actor, max)` keeping the health
fraction; mechanics call it when a `StatPct(Stamina)` or `StatFlat(Stamina)`
aura changes on a player.
**Test:** Fortitude raises max health by 5% and current health
proportionally.

---

## 12. Phase 10: Stats and math (Tier B)

- **Tertiary stats:** add `Stat::Leech`, `Stat::Avoidance`, `Stat::Speed`
  with rating curves in `StatCurves.ratings`. Speed adds to run speed
  (`World::speed`). Leech heals for a share of damage done (self-healing
  only matters for survival). Avoidance reduces AoE damage taken; enemy
  rules need an `aoe: bool` on `EnemyAction::Damage` to know which.
- **Stat conversions:** `ModKind::StatFromStat { from: Stat, to: Stat, pct }`
  evaluated in `Formulas::stat_block` after flat and percent modifiers.
- **Highest-secondary effects:** `ModKind::RatedHighest(pct)` adds to
  whichever secondary is highest at evaluation time (check whether the game
  snapshots it at proc time; if so, resolve at application into a concrete
  `RatedPct`).
- **Armor penetration:** `ModKind::ArmorPenPct` on the attacker reduces the
  target's armor in `mitigate`.
- **Positionals:** players are always behind enemies. Document this in
  `crates/gamedata/src/stats.rs` next to the miss rule; add
  `Requirement::Behind` only if a spec needs "front" (none do for damage
  dealers).

**Tests:** one per modifier against hand-computed numbers.

---

## 13. Phase 11: Survival in Mythic+ (Tier B)

- **Battle resurrection and Soulstone:** `Effect::Resurrect { health_pct }`
  on a dead seat in combat (with a shared group charge pool, as in Mythic+:
  verify current rules). Soulstone: a pre-applied aura with
  `persists_through_death` and a death hook that resurrects. Seats today
  only recover between pulls.
- **Defensive absorbs:** Phase 2.
- **Cheat death:** Phase 6.1.
- **Potions:** Phase 1.3.

---

## 14. Tier C: deferred (don't start without the user)

Listed for completeness, matching `HISTORY.md`'s decision to focus on
damage dealers:

- **Healer model:** raid health pool, damage on non-simulated raiders,
  overhealing, smart-heal spreading, Atonement, Beacon of Light, Echo.
  Needs `ListenFor::HealingDone` and `AlliesWithAura` (4.3).
- **Tank model:** threat, taunts, tank swaps, dodge/parry/block, avoidance
  events (Revenge!, Overpower procs from avoidance), Ignore Pain, Shield
  Block, Stagger caps (Phase 2 makes caps possible).
- **Pet health, positioning and death.**
- **Line of sight, fixates, Mythic+ affixes, enemy healing.**
- **Spells cast by enemies from game data.**

---

## 15. Class and spec coverage map

After the engine work, each class needs real data (`data/game.ron`), a spec
kit for one-offs (`crates/mechanics/src/kits.rs`), a loadout
(`data/loadouts/`), a scripted policy (`crates/run/src/scripts.rs`) and a
SimC comparison. That data work is a separate track per spec; this table
says which phases each needs first.

| Class | Needs |
|---|---|
| Death Knight | 3.3 runes; 5 ground (Death and Decay, standing-in buffs); 6.2 Death Strike window; 2.2 Blood Shield; 4.6 disease spread; 7.1 Army of the Dead; 1.1 channels (none major) |
| Demon Hunter | Soul Fragments as delayed pick-ups (a timed aura per fragment with `on_expire` collection, or a counter aura plus a kit hook); 1.7 Metamorphosis as a form; 5 Sigils; Fel Rush and Vengeful Retreat as `Displace` (direction is irrelevant in 1-D) |
| Druid | Forms (done) and 1.7 form GCDs; 1.8 Prowl snapshots; 3.4 combo-point durations (Rip, Rake); 5 Starfall placement; Eclipse via stacks (already expressible) |
| Evoker | 1.2 empowers; 1.1 Disintegrate channel; 9.3 Augmentation; Deep Breath movement as `Displace` |
| Hunter | 3.1 `RecentCasts` (Steady Focus); 5 traps and bombs; Auto Shot, focus and pets already work |
| Mage | 3.4 mana; 1.1 Evocation, Arcane Missiles, Ray of Frost; 5 Blizzard, Flamestrike; 3.1 `TargetHpAbove` (Firestarter); Ice Floes as charges of a `CastWhileMoving` buff (works) |
| Monk | 1.1 Fists of Fury, Spinning Crane Kick; 3.1 `TargetHpBelowCasterMaxHp` (Touch of Death); 4.3 Mark of the Crane counting; 7.4 Storm, Earth, and Fire |
| Paladin | 5 Consecration; 3.2 Hammer of Wrath requirement (or the existing gate); 2.2 absorbs; 9.2 Blessings on allies |
| Priest | 1.1 Mind Flay, Void Torrent; 2.2 Power Word: Shield; 4.2 Shadow Word: Death on kill; 1.7 Shadowform and Voidform; 9.4 Fortitude; Shadowy Apparitions as travelling triggered spells (works) |
| Rogue | Stealth (done) and 1.8 Vanish; 3.4 combo-point durations; 4.5 Blade Flurry; 1.4 Mutilate both hands; 8.1-style untargetable for Killing Spree (self-immunity: `Immune` modifier); poisons work |
| Shaman | Elemental done; Enhancement needs 1.4 Stormstrike, 5 placed totems, Windfury chains (ExtraSwing exists); Restoration is Tier C |
| Warlock | 2.2 Seed of Corruption thresholds; 4.4 unique Unstable Affliction (verify current rules); 4.5 Havoc; 4.3 Malefic Rapture; 7.2 Wild Imps; 1.1 Drain Soul, Drain Life; 11 Soulstone |
| Warrior | 1.4 for Fury strikes that hit with both weapons (Raging Blow, Rampage); 4.3 Meat Cleaver; 1.1 Bladestorm as a channel with self-`Immune`; 4.5 Sweeping Strikes; 3.2 Execute requirement; rage from auto-attacks already works; Ignore Pain is Tier C |

---

## 16. Suggested order of work

1. Phase 0 (all four items).
2. Phase 1.1 channels, 1.2 empowers, 1.3 categories, 1.4 both-hand strikes.
3. Phase 2 aura values and absorbs.
4. Phase 3 predicates, requirements, runes, resource behaviour.
5. Phase 4.1 adds, 4.2 enemy death, 4.3 target sets, 4.4, 4.5, 4.6.
6. Phase 6 events (except 6.2 and 6.3), then 6.1 cheat death.
7. Phase 4.7 pack positions, then Phase 5 ground effects.
8. Phases 7, 8, 9, 10, 11 in any order, as specs need them.
9. Remaining follow-ups (1.5, 1.6, 1.7, 1.8, 6.2, 6.3).

Commit (when the user asks) at the end of each numbered step, with a
message listing what was added and what is still refused.

## 17. Definition of done for the whole plan

- `setup.rs::unsupported` refuses nothing in Tier A or B.
- No `self.unsupported(...)` call remains for a Tier A or B feature.
- Every new vocabulary item is validated in `check.rs`, documented with a
  game example, observable where a player could see it, and covered by at
  least one test that would fail if the feature were removed.
- The kernel header doc lists exactly what is still unsupported.
- All four lint and test gates pass.
