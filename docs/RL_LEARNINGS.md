# What the Burn RL & MCTS Experiments Taught Us

A synthesis of the self-play reinforcement learning experiments across
single-target (`elemental_stormbringer_dummy.ron`), 5-target $\times$ 40M HP
(`elemental_farseer_5t.ron`, `elemental_stormbringer_5t.ron`), and 10-target
$\times$ 40M HP (`elemental_farseer_10t.ron`, `elemental_stormbringer_10t.ron`)
encounters (October 2026).

This document records:
1. Where the wall-clock time actually goes during training (`SimEnv` rollouts
   vs. Burn neural network forward/backward passes).
2. How network depth (`--num-layers 1..4`), network width (`--hidden-width 32..256`),
   and MCTS search depth/horizon (`12 sims x 5s` $\rightarrow$ `96 sims x 20s`)
   scale.
3. Which algorithmic changes unlocked the jump from `+4.98%` (`+28.1k DPS`) to
   `+14.60%` (`+82.5k DPS`) over SimulationCraft's 12.1.0 AoE APL.
4. What infrastructure to build next so the loop generalizes across all 26
   specs and multi-pull Mythic+ scenarios.

---

## 1. Where the time goes: the bottleneck is `SimEnv` rollouts, not model size

We instrumented `Mcts::search` (`crates/policy/src/lib.rs`) and `train_rl`
(`crates/run/src/train.rs`) to separate wall-clock and thread-sum time across
four phases:
- **`mcts_env`:** cloning `SimEnv`, stepping the discrete-event combat kernel +
  mechanics interpreter (`sim.step`), and featurizing `SeatObs` (`R^132`).
- **`mcts_nn`:** Burn `PolicyValueNet` forward inference (`self.model.evaluate`)
  at root expansion, batched leaf expansion, and 2-step rollout re-evaluation.
- **`burn_backprop`:** Burn `Autodiff<Flex>` forward + backward passes and
  `AdamW` parameter steps (`BurnTrainer::train`).
- **`eval_rollout`:** full-fight (`310s–570s`) candidate evaluation and
  trace-based per-ability damage breakdown (`eval_policy_metrics` +
  `eval_policy_breakdown`).

### Empirical timing profile (`elemental_farseer_5t.ron`, 5 targets $\times$ 40M HP)

| Configuration | `mcts_env` (thread-sum) | `mcts_nn` (thread-sum) | `burn_backprop` (wall) | `eval_rollout` (wall) | MCTS NN Share of Search | Holdout DPS ($n=30$) |
|---|---:|---:|---:|---:|---:|---:|
| `1x64` MLP, `24 sims x 10.0s` (5 iters, baseline loop) | `425.22s` | `0.92s` | `0.30s` | `11.15s` | `0.22%` | `646,355.0` (`+14.32%`) |
| `2x64` ResMLP, `24 sims x 10.0s` (6 iters, baseline loop) | `510.14s` | `1.52s` | `0.51s` | `13.20s` | `0.30%` | **`647,925.3` (`+14.60%`)** |
| `3x64` ResMLP, `24 sims x 10.0s` (5 iters, baseline loop) | `432.25s` | `1.63s` | `0.60s` | `11.26s` | `0.38%` | `645,158.2` (`+14.11%`) |
| `4x64` ResMLP, `24 sims x 10.0s` (5 iters, baseline loop) | `433.32s` | `1.98s` | `0.76s` | `11.23s` | `0.45%` | `642,634.0` (`+13.66%`) |
| `2x64` ResMLP, `96 sims x 20.0s` (4 iters, baseline loop) | `1,997.65s` | `6.29s` | `0.38s` | `9.82s` | `0.31%` | `643,674.2` (`+13.85%`) |
| **`4x64` ResMLP + Fast Stack Math + Gumbel + Playout Cap (5 iters)** | **`140.31s` (`3.1x` faster)** | **`0.34s` (`5.8x` faster)** | **`0.60s`** | **`9.63s`** | **`0.24%`** | **`645,226.8` (`+14.12%`)** |
| **`4x64` ResMLP + All 5 Engine Optimizations (`FlatMap` + Bitmask + Direct Obs)** | **`61.82s` (`7.0x` faster)** | **`0.50s` (`4.0x` faster)** | **`0.65s`** | **`3.44s` (`4.9x` faster)** | **`0.80%`** | **`634,534.8` (`+12.23%`)** |
| **`4x128` ResMLP Stormbringer 5T + All 5 Engine Optimizations (5 iters)** | **`77.46s` (`5.6x` faster)** | **`0.70s`** | **`1.22s`** | **`4.09s` (`4.1x` faster)** | **`0.90%`** | **`591,813.1` (`+10.83%`, `600.4k` eval, `99.3%` student)** |

### Nine speedups we implemented (`7.0x` faster MCTS + `4.9x` faster full-fight eval)

1. **Zero-allocation stack `SeatSnapshot` + single-pass `Formulas::outgoing` (`crates/mechanics/src/math.rs`):**
   - Replaced the 4 `BTreeMap` allocations (`stats`, `pct`, `rated`, `converted`) in `Formulas::stat_block_sharing` with fixed stack arrays (`[f64; 12]` and `[f64; 4]`), pre-indexed rating curves by `Stat` discriminant in `Statics`, and fused the 4 modifier queries in `outgoing` and `mitigate` into a single aura pass.
2. **Pre-indexed Fibonacci open-addressed `FastAuraTable` & `FastSpellTable` with `mod_mask` and `listen_mask` (`crates/mechanics/src/math.rs`, `crates/engine/src/kernel/world.rs`):**
   - Replaced $O(\log N)$ `BTreeMap` lookups on `data.auras` and `data.spells` inside `Formulas`, `Interpreter`, and `World::each_mod` / `World::resolved` with $O(1)$ open-addressed tables storing 32-bit `mod_mask` and `listen_mask` bitfields per aura.
   - Auras without the requested `ModKind` bit or `ListenFor` bit are skipped in a single CPU bitwise-AND instruction before touching `def.modifiers` or `predicate(...)`.
3. **Eliminated per-hit `refresh_conditional_stats` / `refresh_stats` no-op scans and `Vec<AuraRef>` heap allocations (`crates/mechanics/src/interp.rs`):**
   - Precomputed `has_kernel_stat_mod` and `has_cond_kernel_stat_mod` on `FastAura` so damage hits and non-stat buff/debuff applications immediately skip `refresh_stats` (which previously ran `3 * 40 = 120` `BTreeMap` lookups per hit).
   - Replaced the per-event `Vec<AuraRef>` heap allocation in `Interpreter::fire_nested` with a stack `[AuraRef; 32]` buffer filtered by `fast.listen_mask & ev_bit != 0`.
4. **Replaced `BTreeMap` in `Actor::cooldowns`, `World::streams`, and `World::decks` with contiguous sorted `FlatMap<K, V>` (`crates/engine/src/kernel/world.rs`):**
   - Eliminated B-tree node allocations when cloning `World` (`clone mid-fight: 1,612 ns`, `full 484s rollout: 2.694 ms` = **`180,000x` real-time** on a single core) and gated `Trace::absorb` (`postcard` + FNV-1a) on `record_trace`.
5. **Direct `Observer<Obs = Vec<f32>>` on `SeatObsFeaturizer` (`crates/run/src/observe.rs`, `crates/run/src/train.rs`):**
   - During MCTS self-play, `SimEnv<SeatObsFeaturizer>` writes directly from `StateView` into the `R^132` feature vector without constructing `SeatObs` or its 7 `BTreeMap`s.
6. **Truncated $k$-step (`8`-step / `6.0s`) value-bootstrapped rollouts (`Mcts::rollout` in `crates/policy/src/lib.rs`):**
   - `Mcts::rollout` runs at most 8 decisions (`6.0s`), re-evaluates `self.model` at steps 2 and 5 to obtain fresh priors and $V_\theta(s_{t+k})$, and blends $0.78 \, G_{\text{MC}} + 0.22 \, V_\theta$.
7. **Gumbel AlphaZero Sequential Halving + Completed-Q values (`Mcts::search` in `crates/policy/src/lib.rs`):**
   - Root selection draws deterministic Gumbel noise $g_a$, narrows candidates across 3 sequential-halving stages (`8 -> 4 -> 2`), and imputes `completed_v` to unvisited actions.
8. **KataGo Playout Cap Oscillation (`SelfPlayCollector::rollout_mcts_seed` in `crates/learn/src/lib.rs`):**
   - Alternates between full-budget Gumbel MCTS searches and fast playout-cap searches (`simulations / 4`, `horizon / 2`) across `96` steps/episode.
   - Combined, these optimizations reduced `4x64` `mcts_env` thread-sum time from **`433.32s` to `61.82s` (`7.01x` speedup)** and `eval_rollout` wall-clock time from **`16.8s` to `3.44s` (`4.88x` speedup)** — completing a 5-iteration `4x64` 5-target AoE training run in **`~19.7 seconds` wall-clock**, and pushing `4x128` Stormbringer 5T to **`600,357.4 DPS` (`113.0%` eval) / `591,813.1 DPS` (`+10.83%` holdout, `99.3%` student match)** in **`~24.7 seconds` wall-clock**.

---

## 2. Scaling laws: depth, width, search horizon, and target count

### A. Network depth (`--num-layers 1..4` at `hidden_width = 64`, Farseer 5T)

| Depth (`num_layers`) | Architecture | Iter 1 Eval DPS (`vs_script`) | Peak Eval DPS | Holdout DPS ($n=30$) | Holdout Gain (`95% CI`) | Student Match |
|---|---|---:|---:|---:|---:|---:|
| `1` | `132 -> 64 -> 35` (no skip) | `597,152.6` (`104.5%`) | `647,761.4` (Iter 4) | `646,355.0 ± 3,636.0` | `+80,978.9` (`+14.32%`) | `97.8%` |
| `2` | `132 -> 2x64 ResMLP -> 35` | `613,651.9` (`107.4%`) | **`647,985.7`** (Iter 6) | **`647,925.3 ± 3,361.6`** | **`+82,549.2` (`+14.60%`)** | `98.2%` |
| `3` | `132 -> 3x64 ResMLP -> 35` | `630,051.9` (`110.3%`) | `645,820.3` (Iter 2) | `645,158.2 ± 3,857.4` | `+79,782.1` (`+14.11%`) | `97.5%` |
| `4` | `132 -> 4x64 ResMLP -> 35` | **`645,193.8` (`112.9%`)** | `647,271.3` (Iter 5) | `642,634.0 ± 3,659.5` | `+77,258.0` (`+13.66%`) | **`98.7%`** |

- **Depth accelerates early-iteration representation learning:** At Iteration 1
  (after just `240` MCTS samples), the `4x64` residual network jumps straight to
  `645,193.8 DPS` (`112.9%` of the SimC AoE APL), whereas `1x64` is still at
  `597,152.6 DPS` (`104.5%`) and needs 4 iterations to reach the same plateau.
- **Asymptotic generalization peaks at `2–3` residual layers:** On 30 held-out
  seeds, `2x64` (`647,925.3 DPS`, `+14.60%`) and `3x64` (`645,158.2 DPS`,
  `+14.11%`) slightly outperform `4x64` (`642,634.0 DPS`, `+13.66%`), which
  becomes mildly over-confident (`Lava Burst` cut all the way to `4.5` casts) on
  a 3,200-sample buffer.

### B. Network width (`--hidden-width 32..256` at `num_layers = 2`, Farseer 5T)

| Width (`hidden_width`) | Parameter Count | Peak Eval DPS | Holdout DPS ($n=30$) | Holdout Gain (`95% CI`) | Clear Time Delta |
|---|---:|---:|---:|---:|---:|
| `2x32` | `~6.5k` | `648,708.2` | `643,744.0 ± 3,288.1` | `+78,367.9` (`+13.86%`) | `-43.16s` |
| `2x64` | `~17.5k` | `647,985.7` | **`647,925.3 ± 3,361.6`** | **`+82,549.2` (`+14.60%`)** | **`-45.16s`** |
| `2x128` | `~51.5k` | `647,561.2` | `645,300.5 ± 4,015.8` | `+79,924.4` (`+14.14%`) | `-43.88s` |
| `2x256` | `~168.0k` | `641,517.2` | `636,117.8 ± 3,841.6` | `+70,741.7` (`+12.51%`) | `-39.41s` |

### C. MCTS search depth & lookahead horizon (`Farseer 5T`)

| Search Budget (`sims x horizon`) | Rollout GCDs | Iterations to `>645k` Eval DPS | Peak Eval DPS | Holdout DPS ($n=30$) | `mcts_env` Time |
|---|---:|---:|---:|---:|---:|
| `12 sims x 5.0s` | `~4 GCDs` | Never (`638.9k` max) | `638,955.4` | `643,592.6` (`+13.83%`) | `~185s` |
| `24 sims x 10.0s` | `~8 GCDs` | `5–6 iters` | `647,985.7` | **`647,925.3` (`+14.60%`)** | `~430s` |
| `48 sims x 15.0s` | `~12 GCDs` | **`3 iters`** | `647,516.4` | `642,810.9` (`+13.70%`) | `~920s` |
| `96 sims x 20.0s` (+ $V_\theta$ bootstrap) | `~16 GCDs` | `4 iters` (`3 ep/iter`) | `645,023.2` | `643,674.2` (`+13.85%`) | `1,997.7s` |

- **Why `10.0s–15.0s` is the critical threshold for Elemental Shaman:** A `5.0s`
  horizon (`~4` GCDs) is shorter than a single 6-second `Earthquake` duration
  and cannot see a full `Stormkeeper` $\rightarrow$ `Chain Lightning` $\rightarrow$
  `Earthquake` build-and-spend cycle. Extending the horizon to `10.0s–15.0s`
  exposes the full Maelstrom + `Ancestor` payoff, while `24–32` simulations per
  decision maximizes sample throughput per wall-clock minute.

### D. Scaling across hero trees and AoE pack sizes (`5 Targets` vs. `10 Targets`)

| Scenario & Hero Tree | Pull Health | SimC AoE APL Holdout ($n=30$) | Burn RL Holdout ($n=30$) | Paired Holdout Gain (`95% CI`) | Clear Time Saved |
|---|---:|---:|---:|---:|---:|
| **Farseer — 5 Targets $\times$ 40M** | `200M HP` | `565,376.1 ± 4,294.8` | **`647,925.3 ± 3,361.6`** | **`+82,549.2` (`+14.60%`, `[+78.6k, +86.5k]`)** | **`-45.16s`** (`363.9s -> 318.8s`) |
| **Stormbringer — 5 Targets $\times$ 40M** | `200M HP` | `533,992.4 ± 3,702.6` | **`590,763.0 ± 4,557.7`** | **`+56,770.6` (`+10.63%`, `[+53.0k, +60.5k]`)** | **`-35.98s`** (`384.7s -> 348.8s`) |
| **Farseer — 10 Targets $\times$ 40M** | `400M HP` | `710,160.6 ± 3,891.8` | **`766,074.1 ± 4,324.4`** | **`+55,913.5` (`+7.87%`, `[+50.9k, +60.9k]`)** | **`-41.12s`** (`573.4s -> 532.3s`) |
| **Stormbringer — 10 Targets $\times$ 40M** | `400M HP` | `876,839.8 ± 9,471.7` | **`951,892.4 ± 10,297.9`** | **`+75,052.6` (`+8.56%`, `[+63.2k, +86.9k]`)** | **`-36.00s`** (`466.6s -> 430.6s`) |

---

## 3. Four algorithmic lessons that made continuous learning work

1. **State-dependent rollout priors (`Mcts::rollout` in `crates/policy/src/lib.rs`):**
   Reusing a leaf node's static prior vector across a 10s–15s rollout fails
   whenever a builder cast changes legality mid-rollout (e.g., Maelstrom crossing
   `55` to unlock `Earthquake`, or `Tempest` proccing). Re-evaluating `self.model`
   every 2 rollout steps at low temperature (`0.15`) made MCTS rollouts state-aware
   while adding `<0.3%` neural-network overhead.
2. **Breaking warm-start lock-in (`p^0.75` root prior softening + teacher buffer decay):**
   Warm-starting from a deterministic SimC script produces `0.995` priors on
   scripted choices. Without softening root priors (`p = p.powf(0.75)` before
   Dirichlet noise) and decaying the warm-start imitation buffer (`warm_keep =
   warm_buffer.len() / (iter + 1)`), PUCT under-explores alternatives and gradient
   steps pull the policy back toward suboptimal APL rules.
3. **Sharpened Q-value policy targets (`w = visits * exp(5.0 * nq)`):**
   With 24–32 simulations over 12–15 legal actions, visit counts alone are too
   flat when a newly discovered action only gets 2–3 visits despite having the
   highest return. Weighting visit targets by `exp(5.0 * normalized_q)` and
   pruning single-visit low-Q noise (`visits == 1 && nq < 0.65`) lets the policy
   network lock onto high-Q discoveries in 1–2 iterations.
4. **Staggered full-fight phase coverage + scale-invariant rewards:**
   Fast-forwarding self-play episodes by `0, 40, 80, 120, 160` steps with the
   current best policy ensures MCTS searches opener/Bloodlust, dry mid-fight
   sustain, 2-minute cooldown windows, and late-pull target-death cleanup, while
   scaling `DenseReward::per_k_damage` by `10_000 / scripted_dps` keeps value
   targets invariant across 1T (`250k DPS`) and 10T (`950k DPS`).

---

## 4. Infrastructure roadmap for all 26 specs and Mythic+ scenarios

### Priority 1: `SimEnv` copy-on-write / flat-array state (`3x–5x` MCTS speedup)
Because `99.6%` of MCTS time is spent inside `mcts_env` (`env.clone()` +
`sim.step()`), replacing `BTreeMap` aura/cooldown tables and event queues in
`portunus-engine` / `portunus-mechanics` with `SmallVec` or copy-on-write arena
slices (`Arc<[AuraInstance]>`) will directly multiply MCTS simulation throughput
by `3x–5x` with zero change to semantics or trace hashes.

### Priority 2: Multi-target debuff distribution & `target_if` action macros
For multi-DoT and debuff-spreading specs (**Affliction Warlock**, **Shadow
Priest**, **Balance Druid**, **Assassination Rogue**, **Feral Druid**, and
**Stormbringer's `Lightning Rod`**):
- Add `pack_aura_coverage_frac` and `min_pack_aura_remaining` per tracked aura
  to `SeatObsFeaturizer` (`crates/run/src/observe.rs`).
- Add target-selection decoding (`CastOnMinDebuff(spell)` / `CastOnMaxHealth(spell)`)
  to `SpecActionSpace` (`crates/env/src/action_space.rs`) so policies can spread
  DoTs across 5–10 targets without multiplying action-space width by target count.

### Priority 3: Observable Time-To-Die (`TTD`) and multi-pull cooldown holding
Add `estimated_pull_ttd_sec` (live pack HP divided by rolling party DPS) and
`pack_hp_frac` to `SeatObsFeaturizer` so policies on multi-pull dungeon
scenarios (`crates/scenario`) learn when to hold 2-minute cooldowns (`Ascendance`,
`Combustion`, `Avatar`, `Metamorphosis`) at the end of a dying pack for the next
pull's engage.

### Priority 4: Closed-loop Policy $\leftrightarrow$ Stat Co-Optimization
Wire `portunus-run optimize` (`RacingOptimizer`) to accept a trained Burn RL
checkpoint dir (`--policy target/rl_...`) so secondary-stat allocations (`Crit`,
`Haste`, `Mastery`, `Versatility`) can be re-optimized for the RL-learned
rotation (e.g., shifting out of Crit on Farseer AoE once RL replaces 100%-crit
`Lava Burst` with `Chain Lightning` + `Earthquake`).

---

## 5. Subtlety Rogue Single-Target Talent-Setup $\times$ RL Optimization Study & All-Spec Talent Verification (`40M` HP Boss)

### 5.1 How RL + `SpellDataDump` Caught Four Subtlety Rogue Mechanics Bugs

Running Burn RL (`Mcts` + `BurnTrainer`) across Subtlety Rogue talent builds and
cross-checking `data/game.ron` against `target/simc/engine/class_modules/sc_rogue.cpp`
and `target/simc/SpellDataDump/rogue.txt` uncovered **four classic game-data
bugs** that aggregate total-DPS parity (`≤ 0.5%`) had masked:

1. **`The First Dance` (`382505` / `470678`) — Permanent Passive Checked Instead of Consumable Ready Buff:**
   - `Shadow Dance` (`185313`) previously checked `when: caster_has_aura(382505)`
     (the permanent talent passive), extending **every** `Shadow Dance` in combat
     by `+4s`. In `sc_rogue.cpp:7414-7443, 11770-11773` and
     `SpellDataDump/rogue.txt:14600-14622`, `382505` grants the consumable ready
     buff `470678` out of combat (on pull and on `Stealth` `1784`), which is
     consumed (`remove_aura(aura: 470678, target: caster)`) on the **first**
     `Shadow Dance` cast.
2. **`Deepening Shadows` (`185314` / `1271702`) & `Improved Secret Technique` (`1279444`) — Deprecated Pre-12.1 Cooldown Reduction:**
   - `data/game.ron` had both the 12.1 Midnight base cooldowns (`20s` `Shadow Dance`,
     `25s` `Secret Technique`) **and** the legacy Dragonflight/TWW per-CP
     cooldown-reduction listeners (`0.7s/CP` on `185314` and `1.0s/CP` on
     `1279444`), causing `Shadow Dance` (`21` casts vs SimC's `8` casts) and
     `Secret Technique` (`15+` casts vs SimC's `5` casts) to cycle $2.5\times$ too
     fast. In 12.1 (`SpellDataDump/rogue.txt:2935, 17237, 17531`), `Deepening Shadows`
     increases `Shadow Dance` duration by `150%` of Haste rating% (`+1.2s–1.5s`),
     and `Improved Secret Technique` increases `Secret Technique` damage by `+15%`.
3. **`Unseen Blade` (`441146` / `441144`) — Missing `20s` Cooldown Lockout (`459485`), `Thousand Cuts` (`441346`), and `Disorienting Strikes` (`441274`):**
   - Previously, `Backstab`, `Gloomblade`, and `Shadowstrike` triggered
     `Unseen Blade` (`441144`) on **100% of builder casts** (`69+` hits/fight),
     which RL exploited by spamming builders into `Coup de Grace`. In
     `sc_rogue.cpp:8766-8793, 11036-11051, 11473-11500` and
     `SpellDataDump/rogue.txt:12206, 12335, 12400, 13951, 17382`, `Unseen Blade`
     applies a `20s` lockout aura (`459485`, reduced to `15s` with `25%`
     double-strike chance by `Flashing Steel` `1276630`), which is refreshed by
     `Thousand Cuts` (`441346`, `4.5` hasted RPPM on auto-attacks) and bypassed
     for 2 strikes after `Secret Technique` / `Killing Spree` by
     `Disorienting Strikes` (`441274` / `441275`).
4. **`Clear the Witnesses` (`1248793`) & `5+ CP` Finisher Gate on `Deathstalker's Mark` (`457052` / `457157`):**
   - When RL trained on Deathstalker, it immediately discovered two specification
     loopholes: (a) `Eviscerate` (`196819`) could be cast at `1 CP` to trigger
     full `Deathstalker's Mark` (`457157`) + `Darkest Night` (`457280`, `+30`
     Energy) + `Eviscerate (Shadow)` (`328082`), and (b) `Shuriken Storm`
     (`197835`) had a stray `trigger_spell(spell: 457157)` on `1248793` (*Clear
     the Witnesses*, which in `SpellDataDump/rogue.txt:15436` is simply `+60%`
     `Shuriken Storm` / `Fan of Knives` damage). Fixing `1248793`, guarding
     `Shadowstrike`'s mark application on `!target_has_aura(457129) && !caster_has_aura(457280)`,
     requiring `5+` CP on finishers (`457052#2` and `457058`), and wiring 12.1
     `Danse Macabre` (`382528` -> `1264397` *Lashe Macabre*), `Death Perception`
     (`469642`), `The Rotten` (`382015`/`382016`), `Finality` (`382525`),
     `Veiltouched` (`382017`), `Replicating Shadows` (`382506`), `Hunt Them Down`
     (`457054`/`457193`), `Symbolic Victory` (`457062`/`457167`), and
     `Unshakeable Drive` (`1248774`/`1248775`) aligned both total DPS (`100.0%–100.4%`)
     and per-spell cast counts with `simc`.

### 5.2 Subtlety Rogue Single-Target Talent-Setup $\times$ RL Matrix (`40M–60M` HP Uniform Distribution + `target_abs_hp` Feature)

To prevent RL policies from overfitting cooldown timings (`Goremaw's Bite` `45s` CD, `Shadow Blades` `90s` CD, `Vanish` `120s` CD) to a single fixed fight duration, we:
1. Updated `data/scenarios/single_target_boss.ron` to `health: uniform(lo: 40.0, hi: 60.0)` (`40M–60M` HP, mean `50M` HP, matching SimC's `vary_combat_length = 0.20`), and
2. Added `TargetObs::health` / `max_health` (`SeatObs::target_health()`) and normalized remaining absolute health (`feat[7] = (health / 50_000_000.0).clamp(0.0, 2.0)`) to `SeatObsFeaturizer` (`crates/run/src/observe.rs`) and `FlatObserver` (`crates/env/src/observe.rs`), expanding Subtlety's feature vector from `154 -> 155` features.

Evaluating all six 34-point Spec + 13-point Hero talent setups across the `40M–60M` HP boss distribution (`4x64` residual `PolicyValueNet`, 5 iterations $\times$ 32 episodes $\times$ 24 MCTS sims, `n=30` paired eval seeds `1000..1030`):

| Rank | Talent Setup & Hero Tree | Run File | SimC Script DPS (`40M–60M`, `n=30`) | Burn RL DPS (`40M–60M`, `n=30`) | RL Gain ($\Delta$ DPS) | RL Gain (%) | Mean Kill Time (`Script` $\rightarrow$ `RL`) | Fixed `40M` RL DPS |
| :-: | :--- | :--- | :---: | :---: | :---: | :---: | :---: | :---: |
| **1** | **Deathstalker — Danse / Clone / Finisher Amp (`Death Perception` 2/2 + `Danse Macabre` + `The Rotten` + `Replicating Shadows`, drops `The First Dance`)** | `subtlety_deathstalker_danse_boss.ron` | `340,235.9 ± 5,254.0` | **`378,628.0`** | **`+38,392.1`** | **`+11.28%`** | `160.02s` $\rightarrow$ **`144.86s`** | `390,100.4` |
| **2** | **Trickster — Dark Brew Finisher Burst (`Dark Brew` + `Finality` 2/2 + `Death Perception` 2/2 + `The Rotten`, drops `The First Dance`)** | `subtlety_trickster_burst_boss.ron` | `333,657.5 ± 7,485.7` | **`349,893.8`** | **`+16,236.3`** | **`+4.87%`** | `163.22s` $\rightarrow$ **`156.06s`** | `364,687.6` |
| **3** | **Deathstalker — Dark Brew Shadow/Plague Amp (`Dark Brew` + `Finality` 2/2 + `The Rotten` + `Veiltouched`, drops `The First Dance`)** | `subtlety_deathstalker_dark_brew_boss.ron` | `304,899.7 ± 3,666.1` | **`342,212.8`** | **`+37,313.1`** | **`+12.24%`** | `177.25s` $\rightarrow$ **`159.30s`** | `356,788.3` |
| **4** | **Trickster — Unseen / Shadow Clone (`Backstab` + `Weaponmaster` + `Replicating Shadows` + `Danse Macabre`)** | `subtlety_trickster_unseen_boss.ron` | `316,027.4 ± 5,373.8` | **`338,526.6`** | **`+22,499.2`** | **`+7.12%`** | `171.54s` $\rightarrow$ **`161.13s`** | `349,146.1` |
| **5** | **Deathstalker — Canonical SimC MID2 Build (`The First Dance` + `Finality` 1/2 + `Death Perception` 1/2)** | `subtlety_deathstalker_boss.ron` | `288,292.0 ± 3,901.2` | **`316,657.8`** | **`+28,365.8`** | **`+9.84%`** | `186.84s` $\rightarrow$ **`171.18s`** | `324,589.3` |
| **6** | **Trickster — Canonical SimC MID2 Build (`The First Dance` + `Finality` 1/2 + `Death Perception` 1/2)** | `subtlety_trickster_boss.ron` | `280,346.0 ± 5,551.9` | **`297,066.7`** | **`+16,720.7`** | **`+5.96%`** | `192.06s` $\rightarrow$ **`181.73s`** | `306,366.6` |

- **Why the `40M–60M` HP distribution + `target_abs_hp` (`feat[7]`) matters:**
  - Across `40M–60M` HP bosses (`135s–182s` mean fight length), SimC's script only casts `1.0` `Vanish` (`120s` CD) per fight because its `cp <= 2.0 && energy >= 50` gate misses or delays the second window, whereas the RL policy consistently lands **`1.8–2.0` `Vanish` casts**, **`3.6–4.4` `Goremaw's Bite` (`45s` CD) casts**, and **`2.0–2.5` `Shadow Blades` (`90s` CD) casts** depending on the sampled boss HP.
  - Adding `target_abs_hp` (`feat[7]`) breaks the collinearity between `combat_sec` (`feat[1]`) and `target_hp` (`feat[6]`), giving both the policy and value heads direct visibility into remaining boss health from $t = 0\text{s}$ onward.
  - The **exact same build ranking (#1 through #6)** holds across both fixed `40M` HP and the `40M–60M` HP distribution.

### 5.3 Automated All-Spec Talent & Cast-Rate Verification (`audit_talents` + `cast_rate_divergences`)

To detect broken talents systematically across all 26 specs, we implemented two automated verifiers:
1. **`portunus_ingest::audit_talents(&GameData, Option<&SpellDump>)` (`crates/ingest/src/dump.rs` & `crates/ingest/tests/data_files.rs`):**
   - Scans every `TalentDef`, `AuraDef`, and `SpellDef` across all 26 specs and hero trees against `target/simc/SpellDataDump/*.txt` for four bug classes:
     - `TalentFindingKind::UnconsumedPassiveOverride` (*The First Dance* pattern: checking a permanent passive `duration: None` to override buff duration when the dump specifies `"next "` or `"out of combat"`),
     - `TalentFindingKind::DeprecatedCooldownReduction` (*Deepening Shadows / Improved Secret Technique* pattern: `AdjustCooldown` listener on a talent aura whose 12.1 dump entry has no cooldown reduction),
     - `TalentFindingKind::MissingDumpCooldownOrRppm` (*Unseen Blade / Thousand Cuts* pattern: dump specifies `Real PPM` or `"may occur once every ..."` lockout missing in `game.ron`), and
     - `TalentFindingKind::InertOffensiveTalent` (a talent grants a `PassiveAura` with no modifiers, listeners, periodic ticks, or predicate references, while its dump description is offensive).
   - Subtlety Rogue (`Spec 261`, `Trickster 51`, `Deathstalker 53`), Elemental Shaman (`Spec 262`, `Stormbringer 54`, `Farseer 55`), Arms Warrior (`71`), Fury Warrior (`72`), and Retribution Paladin (`70`) have **0 findings** across all four categories.
2. **`SimcParityReport::cast_rate_divergences` (`crates/run/src/parity.rs` & `crates/run/src/report.rs`):**
   - Surfaces any core rotational ability (`portunus_casts >= 5`, `simc_casts >= 5`, `simc_share >= 3%`) whose cast count diverges from SimC by `> 35%`, preventing spec-wide DPS scalars from hiding cooldown or resource bugs.

---

## 6. Multi-Pull Mythic+ Dungeon Wing RL Study (`mythic_plus_wing.ron`)

### 6.1 Scenario & Observation/Reward Architecture

In a real Mythic+ dungeon key, pulls are **heterogeneous** (a `12M` HP `Dungeon Lieutenant` surrounded by four `4M` HP `Dungeon Trash` mobs, followed by another trash+lieutenant pack, followed by a `32M` HP `Dungeon Boss`), separated by `8s–12s` travel windows, and the objective is **minimizing total wing completion time** rather than maximizing raw pad AoE DPS.

We added three Mythic+ archetypes to `data/enemies.ron` (`dungeon_lieutenant`: `12M` HP / `8` forces, `dungeon_trash`: `4M` HP / `2` forces, `dungeon_boss`: `32M` HP / `cc_immune`) and created `data/scenarios/mythic_plus_wing.ron`:
- **Pull 1 (`courtyard_pack`):** `1x Dungeon Lieutenant` (`12M` HP) + `4x Dungeon Trash` (`4M` HP each = `28M` pack HP), `health: uniform(lo: 0.85, hi: 1.15)`, `8s` travel (`3s` prepull).
- **Pull 2 (`hallway_pack`):** `1x Dungeon Lieutenant` (`12M` HP) + `4x Dungeon Trash` (`4M` HP each = `28M` pack HP), `health: uniform(lo: 0.85, hi: 1.15)`, `10s` travel (`3s` prepull).
- **Pull 3 (`wing_boss`):** `1x Dungeon Boss` (`32M` HP), `health: uniform(lo: 0.85, hi: 1.15)`, `12s` travel (`3s` prepull), `forces_required: Some(32)`.

To make learned policies multi-pull and priority-target aware, we added:
1. **`SeatObs` & `SeatObsFeaturizer` / `FlatObserver` (`crates/run/src/observe.rs` & `crates/env/src/observe.rs`):**
   - `pulls_remaining` (`feat[9] = (pulls_remaining / 5.0).clamp(0.0, 2.0)`): `2` on Pull 1, `1` on Pull 2, `0` on the Boss.
   - `pack_health` (`feat[10] = (pack_health / 100_000_000.0).clamp(0.0, 4.0)`): total remaining HP across all engaged enemies in the current pull.
   - `priority_health` (`feat[11] = (priority_health / 50_000_000.0).clamp(0.0, 2.0)`): highest remaining HP among engaged enemies in the current pull (the mob holding up pull completion).
2. **`DenseReward::per_k_priority_damage` (`crates/env/src/reward.rs` & `crates/run/src/train.rs`):**
   - Weighted `70%` total pack damage + `30%` priority-target (`priority_health`) damage + `+50.0` per pull cleared `- 2.5` per second elapsed, directly rewarding funneling single-target damage into the `12M` HP Lieutenant while cleaving the `4M` HP trash.
3. **Travel-Invariant MCTS Lookahead (`crates/policy/src/lib.rs`):**
   - Capped single-step elapsed time contribution toward `MctsConfig::horizon` at `2.5s` so an `8s–12s` inter-pull travel gap does not prematurely terminate an MCTS simulation before seeing the next pull's opening GCDs.

### 6.2 3-Pull Mythic+ Wing Results (`4x64` Residual `PolicyValueNet`, `n=30` Paired Seeds)

| Spec & Hero Tree | Run File | SimC Script DPS (`n=30`) | Burn RL DPS (`n=30`) | RL DPS Gain (%) | Total Wing Time (`Script` $\rightarrow$ `RL`) | Time Saved |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: |
| **Subtlety Rogue [Deathstalker — Danse / Finisher Amp]** | `subtlety_deathstalker_danse_mplus.ron` | `446,865.7 ± 8,923.2` | **`525,712.7`** | **`+78,847.0` (`+17.64%`)** | `228.56s` $\rightarrow$ **`198.22s`** | **`-30.34s` (`-13.3%`)** |
| **Subtlety Rogue [Trickster — Dark Brew Finisher Burst]** | `subtlety_trickster_burst_mplus.ron` | `568,311.2 ± 14,031.3` | **`611,241.6`** | **`+42,930.4` (`+7.55%`)** | `186.19s` $\rightarrow$ **`175.04s`** | **`-11.15s` (`-6.0%`)** |
| **Elemental Shaman [Stormbringer]** | `elemental_stormbringer_mplus.ron` | `381,513.5 ± 5,202.6` | **`394,023.7`** | **`+12,510.2` (`+3.28%`)** | `261.82s` $\rightarrow$ **`254.63s`** | **`-7.19s` (`-2.7%`)** |
| **Elemental Shaman [Farseer]** | `elemental_farseer_mplus.ron` | `405,739.6 ± 5,767.6` | **`406,572.4`** | **`+832.8` (`+0.21%`)** | `248.24s` $\rightarrow$ **`247.85s`** | **`-0.39s` (`-0.2%`)** |

### 6.3 Why Subtlety Rogue Saves `30.3 Seconds` (`+17.64%` DPS) Across the 3-Pull Wing

SimC's `subtlety` priority list uses a simple target-count rule (`if enemies > 1 { Shuriken Storm }`, `if enemies >= 2 { Black Powder }`). On a Mythic+ pack with `1x 12M HP Lieutenant + 4x 4M HP Trash`:
- **What SimC's script does (`228.56s` total wing time):** Spams `Shuriken Storm` (`53.8` casts) and `Black Powder` (`28.8` casts) for as long as 2+ mobs are alive. The four `4M` HP trash mobs die halfway through the pull, leaving the `12M` HP Lieutenant alive at `~55%` HP with `Deathstalker's Mark` undetonated.
- **What Burn RL learns (`198.22s` total wing time, `-30.34s`):**
  1. **Priority-Target Funneling on Packs 1 & 2:** Cuts `Shuriken Storm` from `53.8 -> 21.6` casts (`-32.2` casts) and replaces it with **`Shadowstrike` (`17.9 -> 35.1` casts, `+23,759 DPS`)** and **`Eviscerate` (`38.2 -> 47.5` casts, `+27,114 DPS`)** directly into the `12M` HP `Dungeon Lieutenant`, while letting **`Secret Technique` (`+18,563 DPS`)**, **`Goremaw's Bite` (`+4,014 DPS`)**, and `24.0` **`Black Powder`** casts passively cleave the four `4M` HP trash mobs down at the exact same moment the Lieutenant dies!
  2. **Detonating `+16.8` More `Deathstalker's Mark` Stacks (`35.6 -> 52.4` hits, `+21,870 DPS`):** By weaving `Shadowstrike` + `Eviscerate` onto the Lieutenant during trash pulls instead of spamming `Shuriken Storm` (which does not apply `Deathstalker's Mark`), RL triggers `+16.8` more `Deathstalker's Mark` explosions and `+52.5` more `Hunt Them Down` procs.
  3. **Inter-Pull `Stealth` (`3.7–4.0` casts across 3 pulls) + `The First Dance` (`470678`) + `Vanish` (`1.0 -> 2.0` casts):** Enters every single pull in `Stealth` and uses `Vanish` twice across the wing (`Pull 1` and `Pull 3` Boss).



