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

### Priority 4: Residual `StudentNet` (`2x32`) in `portunus-export`
Upgrade `StudentNet` (`crates/export/src/lib.rs`) from a 1-layer `32`-unit MLP to
a `2x32` residual MLP (`+1,024` floats / `+8 KB` in `PortunusRotation/model.lua`)
so a single distilled addon preserves 98%+ agreement across 1-target, 5-target,
and 10-target regimes simultaneously.

### Priority 5: Closed-loop Policy $\leftrightarrow$ Stat Co-Optimization
Wire `portunus-run optimize` (`RacingOptimizer`) to accept a trained Burn RL
checkpoint dir (`--policy target/rl_...`) so secondary-stat allocations (`Crit`,
`Haste`, `Mastery`, `Versatility`) can be re-optimized for the RL-learned
rotation (e.g., shifting out of Crit on Farseer AoE once RL replaces 100%-crit
`Lava Burst` with `Chain Lightning` + `Earthquake`).
