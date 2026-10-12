# Arms Warrior (Slayer) plan

Second spec after Elemental. Scope agreed with the user:

- **Hero tree:** Slayer first; Colossus later.
- **Targets:** multi-target play now (Sweeping Strikes, Cleave, Reap the
  Storm, SimC's `slayer_aoe` list).
- **Numbers:** the 12.0 spell dump's coefficients as they are, labelled
  uncalibrated like Elemental's. Mechanics must be faithful to SimC;
  magnitudes need not be.

Reference build: SimC's `profiles/MID2/MID2_Warrior_Arms.simc` (talent
string decoded against `trait_data.inc`), with its `slayer_st`,
`slayer_execute` and `slayer_aoe` lists. SimC is the 12.1 module in
`target/simc/engine/class_modules/sc_warrior.cpp`; where 12.0 and 12.1
differ, we follow 12.1 (the dump's own version checks).

Each phase follows the per-feature checklist in `MECHANICS_PLAN.md`
section 1.5 and ends with the four gates and a short report.

## Phase A: engine additions (done)

Each is general (not Arms-only), with a synthetic test that fails if the
feature is removed: `crates/mechanics/tests/arms_engine.rs`, plus ingest
checks in `crates/ingest/tests/data_files.rs` and the kit walk in
`crates/mechanics/tests/elemental.rs`. Every feature was mutation-checked.
No existing result moved (241 tests before, 255 after, all passing).

1. **Weapon attack power.** `Coefficient::AttackPower` for a seat holding
   a weapon adds `floor(weapon DPS × 6)` (times `AttackPowerPct`), as
   SimC's `composite_total_attack_power_by_type` for `WEAPON_MAINHAND`
   (the off hand's for an off-hand spell). White hits keep using AP
   without it (SimC's `NO_WEAPON`, since the weapon is already in their
   base), but, as in SimC's `calculate_weapon_damage`, `AttackPowerPct`
   now scales their weapon roll too; the plan had missed that. Pets and
   seats without weapons are unchanged, so no existing result moves.
2. **PRD proc chance.** `ProcChance::Prd { chance }`: the `n`th attempt
   since the last proc succeeds with `C × n`, `C` from SimC's
   `prd::find_constant(chance)`. `ProcView` gains `attempts`, the count
   since the last proc (Slayer's Dominance, Master of Warfare).
3. **Mastery-scaled damage.** `ModKind::MasteryDamagePct`: damage done ×
   `1 + value × mastery% / 100` (Master of Arms).
4. **Auto-attack scopes.** `ModScope::AutoAttack` (white hits only) and
   `ModScope::Abilities` (everything else). The Arms spec aura gives
   abilities +15% and autos +476%; Avatar and the mastery give both.
5. **Damage taken from the aura's source.**
   `ModKind::SourceDamageTakenPct` on a target's aura multiplies damage
   its source deals to its holder (Colossus Smash, Overwhelmed; SimC's
   "Modify Damage Taken% from Caster's Spells").
6. **Channel shapes.** `CastKind::Channel` gains `tick_zero` (a tick as
   it starts, SimC's `tick_zero`) and `on_end` (effects when it runs its
   course, SimC's `last_tick`; the kernel calls the new
   `Mechanics::channel_ended`), and `ModKind::ChannelTicksAdd` adds ticks
   (Imminent Demise on Bladestorm). The plan said over the same duration;
   SimC's Bladestorm lengthens by a tick's time per added tick
   (`tick_time × (ticks + stacks)`), so that's what the kernel does.
7. **Count scales.** `CountScale.form` (`Linear` as now, `Inverse`:
   `1 + pct/100 / n`, `Times`: `pct/100 × n`), and two counts:
   `TargetStacks { aura }` (the hit target's stacks from the caster) and
   `Enemies` (engaged enemies). Fatality is ×Fatal Mark stacks; Culling
   Cyclone is ×(1 + 0.2 / enemies).
8. **Predicates.** `EnemiesAtLeast(n)` (Sweeping Strikes spends a stack
   only when a second target was there; Reap the Storm needs 3) and
   `TargetIsCurrent` (Slayer's Dominance: "your primary target").
9. **Crit-only swings.** `ListenFor::Swing { crit_only }` (Seasoned
   Soldier's extra rage, Wild Strikes).
10. **Scaled cooldown reduction.** `CooldownChange::Reduce` scales with
    the effect's scale, so a `ResourceSpent` listener reduces by the
    amount spent (Anger Management: 1 s per 20 rage).
11. **Extra chain targets.** `ModKind::ChainTargetsAdd` lets a
    single-target damage effect in scope also hit that many more engaged
    enemies, each for `ChainSharePct` of the damage, each its own hit
    (procs, crits) as SimC's `n_targets` and chain multiplier (Sweeping
    Strikes). The extra targets are the nearest to the target (by pack
    position), then in engagement order. Effects with no spell (listener
    effects) and effects aimed at a set of enemies don't chain.

## Phase B: core Arms (done)

Spec 71 in `game.ron`: rage (100, +30 Overwhelming Rage, decay out of
combat), the spec, class and mastery auras, a 2H weapon item, Mortal
Strike, Overpower (2 charges with Improved Overpower), Execute (20–40
rage, Massacre at 35%, Sudden Death's free 2× Execute), Slam / Heroic
Strike (Master of Warfare), Colossus Smash, Avatar, Bladestorm, Rend and
Cleave, Deep Wounds (rolling bank), Fatality, Storm Bolt, Wrecking Throw,
Charge, Battle Stance, the spec tree, damage-relevant class talents as
baseline, and the `arms` policy (SimC's lists). Tests in
`crates/mechanics/tests/arms.rs` against the shipped data.

As built:

- Data: `data/loadouts/arms.ron` (the reference build's 27 spec talents,
  Master of Warfare at 4), `data/runs/arms_dummy.ron`, a do-nothing
  `Arms` kit in `Kits::standard()` (everything is data), and the `arms`
  policy in `crates/run/src/scripts.rs`: `slayer_st` read without
  Slayer, so the Imminent Demise, Executioner, and Opportunist conditions
  take their untalented values. Rend's lines are dead with Cleave
  talented. SimC's `gcd` is read as 1.5 s.
- Execute: SimC's `execute_arms_t` spends 20–40 rage and triggers
  `execute_damage_t` at 2 × spent / 40. Here the cast deals the damage
  itself at spent / 20, which reads the same in the breakdown. Sudden
  Death's free Execute is its own spell (260798, twice the coefficient,
  `cost_rage == 0`), and the buff overrides both Executes with it.
- Sudden Death rolls on every special attack's cast, sharing one RPPM
  state and a 3 s internal cooldown across them. Master of Warfare's PRD
  rolls the same way on hits (Mortal Strike, Slam, Overpower, the
  Executes). Executioner's Precision has one shared 500 ms ICD.
- Heroic Might's buff (910101) is made up: SimC splits its accumulator
  into a buff when Colossus Smash lands, and raises the debuff's 1.3 by
  0.03 per stack. Here the buff multiplies by 1 + 0.03 / 1.3 per stack,
  only against the warrior's Colossus Smash target, which comes to the
  same thing.
- Tests: 14 in `arms.rs`, one loadout check in `data_files.rs`, and one
  run-file check that the policy presses every button. Each data
  encoding was mutation-checked; Sudden Death's measured rate (1.9 a
  minute) scales with the data's 1.7.
- 271 tests in total, all passing. Elemental is unchanged.

## Phase C: Slayer

Slayer's Dominance and Strike, Executioner, Imminent Demise, Overwhelming
Blades / Overwhelmed, Culling Cyclone, Fierce Followthrough, Deadly
Focus, Violent Euphoria, Show No Mercy, Slayer's Malice, Unhinged,
Unrelenting Onslaught. Relentless Pursuit and Death Drive aren't in SimC
either.

## Phase D: multiple targets

Sweeping Strikes, Cleave's Rend spread, Reap the Storm, the AoE and
two-target lines of the policy, and a pack run file.

## Known divergences (kept up to date)

- Only the reference build's choices are modelled; untaken talents are
  in the tree but grant nothing.
- A seat without a weapon adds no weapon attack power; SimC counts an
  unarmed 0.5 DPS (3 AP). Pets' attack power never includes their
  owner's weapon. Total attack power isn't rounded to a whole number.
- A channel cut short (moved, stopped) skips its `on_end`; SimC's
  cancelled dot still runs `last_tick`.
- The class tree isn't modelled. The reference build's damage talents in
  it (Two-Handed Weapon Specialization, Barbaric Training, Cruel
  Strikes, Wild Strikes, War Machine, Seasoned Soldier, Frothing
  Berserker, Overwhelming Rage, Anger Management) are baseline auras.
  Rend, Storm Bolt, and Wrecking Throw are baseline spells.
- Battle Stance is always on. Stance swapping isn't modelled.
- Bloodsurge rolls 30% for each Rend; SimC rolls once at 30% × √n.
  These are the same on one target.
- Refreshing Deep Wounds restarts its tick rhythm. SimC keeps it.
- The free Execute ignores Execute's cooldown. That's only right with
  Improved Execute, which the reference build has.
- Auto-attack rage isn't rounded to a tenth as SimC's is.
- Charge's 8-yard minimum and Storm Bolt's stun aren't modelled.
- Master of Warfare's second spell (1269306) is 5% at the two ranks it
  holds. That figure is derived from the trait's curve, not read from
  the dump.
- Bladestorm's strike (50622) shows up in the breakdown under the same
  name as the cast.
