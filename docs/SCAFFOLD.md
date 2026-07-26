# Scaffolding Prompt: Speedrun-Optimizing Combat Simulation System ("Project")

This document is meant for the coding agent implementing the first pass on this project.

You are scaffolding a Rust workspace implementing a next-generation combat simulation
and joint-optimization system for WoW-like MMO combat, targeting dungeon speedrun time
minimization for a five-player team. It succeeds SimulationCraft's static
action-priority-list model with: (1) an encounter DSL that defines _distributions_ over
fights, (2) state-conditional control policies with a first-class WAIT primitive, (3) a
team planner producing a human-executable Plan Artifact, and (4) nested joint
optimization of character configuration, plans, and policies under pluggable
(quantile/risk) objectives.

Scope for this scaffold: **CPU only, event-driven reference simulator**. No GPU kernel,
no neural policies, no live-game automation. Design so those can be added later without
interface breakage.

---

## 0. Governing invariants (violations are bugs, enforce in code review and CI)

1. **Determinism.** Given (codex build, resolved instance, roster, plan, policies,
   seed), a rollout is bit-identical across runs and platforms. All time is integer
   (`u64` microseconds). No float time, no wall-clock, no HashMap iteration order in
   any semantic path (use `IndexMap`/sorted structures or explicit ordering).
2. **The engine kernel contains zero game-specific knowledge.** Three-layer HAL:
   the _kernel_ (`sim-engine`) implements only abstract machinery — event queue,
   schedulable/expiring entities, analytic resources, predicates, decision points.
   The _mechanics vocabulary_ (`sim-mechanics`, §3b) implements game semantics as
   effect primitives against the kernel. The _codex data_ expresses abilities as
   compositions of those primitives (effect IR). Test: the kernel crate must
   compile and pass its unit tests with a synthetic toy codex exercising the
   vocabulary through the same IR path as real content.
3. **Events are the only mutation sites.** Between events, state evolves analytically
   (`(value_at_t0, t0, rate)` triples for linear resources). Skip-to-next-event is
   exact, not approximate.
4. **Simultaneity is deterministic.** Event ordering key is `(time, priority, seq)`.
   Priority classes are defined once, documented as gameplay-semantic decisions
   (e.g., aura-expiry before cast-completion at equal time), and never bypassed.
5. **Lazy invalidation via generation counters.** No surgical heap deletion. Every
   schedulable entity carries a `Gen(u32)`; stale events are discarded at pop.
6. **RNG is counter-based (Philox) keyed on semantic streams**, not draw order:
   `key = hash(seed, actor_id, stream_id, occurrence_index)`. This is what makes
   common-random-number paired comparisons work. Never draw from a shared sequential
   stream in any semantic path.
7. **Full sim state lives in one contiguous arena.** `snapshot() == memcpy`. The
   pending-event queue and generation tables are part of the arena. MCTS and
   time-travel debugging depend on this.
8. **Sim state ≠ observation.** Policies never see `SimState`; they see
   `PlayerObservation` produced by the Projector under an `InfoSet`. Privileged
   values are _replaced with beliefs_ (DSL priors, estimates), not merely masked.
9. **Optimizers never touch sim state.** They propose `(config, plan, policy)` and
   read scores from the Evaluator. All scoring goes through `sim-eval`; no component
   computes its own objective.
10. **Plan mediation.** Planner and policies never couple directly; policies read the
    immutable `Plan` as data. This keeps per-player policies factorized and swappable.
11. **Patch changes land by tier.** Tier 1 (coefficient hotfixes) and tier 2 (new
    abilities/talents that recombine existing effect semantics — the large majority
    of "new" content) are pure codex data deltas. Tier 3 (genuinely novel mechanics:
    new resource behaviors, new proc mechanisms, borrowed-power state machines) lands
    as new primitives in `sim-mechanics` behind stable interfaces — new driver, never
    a kernel patch. The kernel is never modified by game content, ever. `codex diff`
    between builds is a first-class output.
12. **Human-executable outputs only.** No component may drive live-game input. The
    calibration loop uses addon-exported logs with human actuators and the public log
    corpus. This is a hard product constraint, not a style preference.

---

## 1. Workspace layout

```
Cargo.toml            # workspace
crates/
  sim-types/          # shared plain types; zero heavy deps; serde on everything
  sim-codex/          # codex schema, compiler (raw data + annotations -> CompiledActor)
  sim-engine/         # kernel: event loop, arena, predicates — game-agnostic
  sim-mechanics/      # HAL vocabulary: EffectOp primitives + effect-IR interpreter (§3b)
  sim-encounter/      # encounter/dungeon DSL: parser -> ScenarioSampler -> ResolvedInstance
  sim-observe/        # Projector, InfoSet, PlayerObservation layout builder
  sim-policy/         # Policy trait; rule-DSL interpreter; SimC APL adapter; scripted stubs
  sim-planner/        # route graph, pull composition, CD mapping -> Plan
  sim-oracle/         # MCTS over Engine snapshots (upper-bound policy + trace generator)
  sim-eval/           # objectives, CRN pairing, budget allocation, Evaluator service
  sim-optimize/       # CMA-ES over rule params; QD/MAP-Elites over configs; best response
  sim-calibrate/      # log ingest, replay-validation, parameter fitting, protocol generator
  sim-trace/          # deterministic replay files, decision-regret counterfactuals, diffing
  simctl/             # CLI binding everything: run manifests in, scores/traces out
docs/
  DESIGN.md           # this document's expanded form
  PRIORITY_CLASSES.md # the simultaneity tie-break table (semantic decisions log)
  CODEX_SCHEMA.md
fixtures/
  toy_codex/          # synthetic spec for engine tests ("Trainee": 5 abilities, 1 proc)
  golden/             # golden replay traces for regression
```

Dependency rule: `sim-types <- everything`; `sim-engine` (kernel) depends only on
`sim-types`; `sim-mechanics` depends on `sim-engine` + `sim-types`; `sim-codex`
compiles _to_ the mechanics IR but the kernel never depends on codex or mechanics;
nothing depends on `simctl`.

---

## 2. Core shared types (`sim-types`)

```rust
pub type SimTime = u64;                 // microseconds since pull start
pub struct GameBuild(pub String);       // e.g. "11.2.5.61188"
#[derive(Copy, Clone, ...)] pub struct ActorId(pub u8);      // 0..=4 players, 5.. enemies
#[derive(Copy, Clone, ...)] pub struct SlotId(pub u16);      // codex-compiled slot index
#[derive(Copy, Clone, ...)] pub struct TargetSlot(pub u8);
#[derive(Copy, Clone, ...)] pub struct Gen(pub u32);
pub struct Seed(pub u64);

pub enum TargetSel { Primary, Slot(TargetSlot), LowestHp, InterruptTarget }

pub enum Action {
    Cast { slot: SlotId, target: TargetSel },
    Wait(WaitSpec),
}

pub enum WaitSpec {
    NextEvent,                          // bare WAIT: wake on next decision-relevant event
    Until(SimTime),
    UntilPredicate(PredicateId),        // engine-compiled; see §3 predicates
    GcdEnd,
}
// NOTE deliberately absent: Wait::For(duration). Relative waits are banned (drift,
// haste-mutation bugs). All waits are interruptible leases: the actor's default
// decision-point triggers remain live during any wait.

pub struct ActionMask {
    pub castable_now: u64,                  // bit per slot
    pub usable_in: [f32; MAX_ACTIONS],      // seconds until legal; enables micro-waiting
    pub wait_menu: WaitMenu,                // engine-suggested semantic waits
}

pub struct Outcome {
    pub kill_time: Option<SimTime>,         // None = wipe/timeout
    pub deaths: u32,
    pub per_actor: Vec<ActorOutcome>,       // damage, uptime, resource waste, etc.
}
```

`PlayerObservation` is the flat struct from the design discussion (~430 f32-equiv,
~1.7KB): self core state, cooldown table (remaining/charges fractional/recharge),
self auras (remaining/stacks), up to 8 targets (hp frac, log-scaled absolute HP,
smoothed TTD, cast bar + interruptibility, per-target debuff table), 4 compact ally
summaries (hp, mana, incoming-damage rate, burst-window remaining, external-ready),
group context (elapsed, lust state, phase, enemies alive), movement block (speed,
distance_to_target, up to 2 displacement demands with deadlines + penalty class,
planned_movement_in), a plan-events block (`upcoming[8]`: type, time_until,
duration, confidence — fuzzed per InfoSet), and the ActionMask. Layout is
codex-compiled per (spec, build); field offsets are stable within a `codex_ref`.

---

## 3. `sim-engine` — contracts

```rust
pub trait Engine {
    fn reset(&mut self, cx: RolloutContext) -> Result<()>;
    /// Run event loop until some actor must decide (or combat ends).
    fn next_decision(&mut self) -> Step;           // Step::Decide(DecisionPoint) | Step::Done(Outcome)
    fn legal(&self, actor: ActorId) -> ActionMask;
    fn submit(&mut self, actor: ActorId, action: Action) -> Result<()>;
    fn now(&self) -> SimTime;
    fn snapshot(&self) -> Snapshot;                // O(state size) memcpy
    fn restore(&mut self, s: &Snapshot);
    fn state(&self) -> &SimState;                  // for Projector/trace only
}

pub struct RolloutContext<'a> {
    pub instance: &'a ResolvedInstance,            // from sim-encounter
    pub roster: &'a [CompiledActor],               // from sim-codex
    pub plan: &'a Plan,
    pub seed: Seed,
}

pub struct DecisionPoint {
    pub actor: ActorId,
    pub trigger: WakeReason,   // GcdEnd | CastEnd | ProcApplied(SlotId) | DemandArrived
                               // | PredicateMet(PredicateId) | PlanWindow(..) | ...
    pub anticipated: bool,     // true iff the policy scheduled this wake itself
                               // (drives reaction-latency application in Projector)
}
```

Event machinery internals (required patterns, not suggestions): binary heap keyed
`(SimTime, Priority, u64 seq)`; four event families (actor-, aura-, encounter-,
bookkeeping-scheduled); generation-counter lazy invalidation; dirty-flag derived-stat
pipeline (haste mutation re-anchors analytic triples and reschedules tick trains by
Gen bump); analytic resources as `(value, t0, rate)` with on-demand materialization.

**Predicates.** `compile_predicate(expr) -> PredicateId` with two backends:
analytic crossing-solve (e.g., `energy >= 80` → exact wake time, re-solved on rate
mutation) and event-subscription + recheck (e.g., rage, which arrives in lumps from
hits; aura-gained conditions). Predicate language subset: comparisons over
resources, cooldown remaining, aura remaining/stacks, time, plan-window state.

**Livelock rule.** A decision consumes its triggering event; re-invocation at the
same timestamp requires a new event. Bare `Wait(NextEvent)` twice on the same
trigger is an engine-detected error.

**Decision-point completeness (why skip-to-next-event loses nothing).** Between
events, state changes only by monotone analytic drift, so a deterministic policy's
choice is piecewise-constant: any interior instant at which acting could become
rational must be anchored by (a) an event already in the queue, (b) a monotone
threshold crossing (computable wake time), or (c) a known offset from a queued
event — there is no fourth anchor, by invariant 3. Interior-time action is
therefore not forbidden, merely required to _name its time_ via `Wait::Until` /
`UntilPredicate`; unmotivated interior action has zero expected value by
construction. Responsibility split enforcing this: the **engine owns legality
transitions** — it MUST emit a decision point whenever an actor's `ActionMask`
would change (cooldown ready, resource crossing a masked ability's cost,
usability-granting aura, interruptible cast starting), auto-scheduling the
earliest such crossing, so bare `Wait(NextEvent)` is complete w.r.t. mask changes;
the **policy owns value transitions** — preference thresholds invisible to the
mask (pooling targets, timing offsets) are declared via wait predicates.
**Review obligation:** every new codex mechanic must be checked against this
partition — a mechanic whose decision-relevance changes without an event, a
crossing, or a declarable predicate breaks completeness and is a design bug, not
a policy problem. CI: property test that randomly probes interior times of
recorded traces and asserts the mask is constant between consecutive engine-emitted
decision points.

---

## 3b. `sim-mechanics` — the HAL vocabulary layer

The middle layer of the three-layer HAL (invariant 2): game semantics implemented
as **effect primitives** against the kernel, driven by codex data. The game's own
client data represents spells as typed effect records over a bounded vocabulary;
this crate is effectively a reimplementation of that interpreter, which is why the
vocabulary stays small (~30–40 primitives) and why most "new" patch content is
expressible without touching it.

```rust
/// Enum dispatch on the hot path — additive, exhaustiveness-checked. No dyn.
pub enum EffectOp {
    DealDamage(DamageParams),          // school, coefficient, target rule, aoe cap
    ApplyAura(AuraParams),             // duration, stacks, pandemic, snapshot rules
    ModifyStat(StatModParams),
    GrantResource(ResourceParams),
    SpendResource(ResourceParams),
    TriggerProc(ProcParams),           // rppm|ppm|chance, icd, blp, rng stream id
    Periodic(PeriodicParams),          // tick train scheduling, hasted or fixed
    SpendCharges(ChargeParams),
    Interrupt(InterruptParams),
    Displace(DisplaceParams),          // scalar movement model
    Conditional { pred: PredicateId, then_ops: EffectRange, else_ops: EffectRange },
    Scripted(ScriptRef),               // escape hatch — see below
    // ... grown additively as tier-3 mechanics demand
}

/// Effect IR: abilities/talents/procs in the codex are compositions —
/// flat op lists (ranges into a per-actor op arena), not code.
pub struct CompiledEffect { pub ops: EffectRange }

/// Primitives are STATELESS pure functions. All persistence lives in
/// codex-declared arena slots, allocated by the codex compiler into the flat
/// state layout. This preserves snapshot=memcpy, determinism, and CRN.
pub fn apply(op: &EffectOp, ctx: EffectCtx,
             state: &mut ArenaState, sched: &mut EventScheduler);

pub struct DeclaredState {      // in codex, per mechanic needing persistence
    pub slots: Vec<StateSlotDecl>,   // e.g. accumulator for a stacking system
}
```

Rules:

- **Statelessness is an invariant, not a style.** A primitive holding its own
  state (`Box<dyn Mechanic>` with fields) silently breaks invariants 1 and 7.
  Any mechanic needing memory declares arena slots in the codex; the compiler
  allocates them; the primitive reads/writes them by handle.
- **Tier-3 procedure:** new `EffectOp` variant + params struct + `apply` arm +
  (if needed) `DeclaredState` shape + doc entry in `docs/MECHANICS_VOCAB.md`.
  Kernel untouched. Each new variant must also pass the decision-point
  completeness review (§3): its decision-relevance must change only via events,
  crossings, or declarable predicates.
- **Escape hatch with promotion pressure:** `Scripted(ScriptRef)` references a
  hand-written one-off for patch-day mechanics that resist decomposition, tagged
  in codex provenance. Track **IR coverage** (fraction of abilities expressible
  without `Scripted`) as a first-class health metric alongside patch-turnaround
  time; recurring script patterns get promoted to proper primitives. A degrading
  IR-coverage trend is the early warning that maintenance costs are heading
  toward the legacy per-spec-module failure mode.

---

## 4. `sim-codex` — contracts

```rust
pub struct CodexRef { pub spec: SpecId, pub build: GameBuild, pub layout_ver: u32 }

pub trait CodexCompiler {
    /// raw extracted game data + curated annotations + (spec, talents, gear)
    /// -> flat runtime tables the engine executes.
    fn compile(&self, spec: SpecId, talents: &TalentStr, gear: &GearSet,
               build: &GameBuild) -> Result<CompiledActor>;
    fn diff(&self, a: &GameBuild, b: &GameBuild) -> CodexDiff;   // patch delta report
}

pub struct CompiledActor {
    pub codex_ref: CodexRef,
    pub abilities: Vec<AbilityDef>,     // slot-indexed: cost, cd, charges, gcd flag,
                                        // + CompiledEffect (effect-IR ops, §3b)
    pub auras: Vec<AuraDef>,            // duration, stacks, pandemic params, stat mods,
                                        // snapshot rules
    pub procs: Vec<ProcDef>,            // trigger mask, rppm/ppm/chance, icd, blp,
                                        // rng stream id
    pub stats: ResolvedStats,           // post-DR rating conversions
    pub obs_layout: ObsLayout,          // field offsets for PlayerObservation
    pub provenance: Vec<Provenance>,    // per-entry: {extracted | annotated |
                                        //   calibrated{session, n, ci} | scripted{ref}}
}
```

Every codex entry carries provenance. `sim-calibrate` writes `calibrated`
annotations; nothing else may.

---

## 5. `sim-encounter` — contracts

```rust
pub trait ScenarioSampler {
    fn sample(&self, seed: Seed) -> ResolvedInstance;   // distributions -> concrete
    fn priors(&self) -> TimelinePriors;                 // what "realistic" InfoSet
}                                                       // substitutes for truth

pub struct ResolvedInstance {
    pub pulls: Vec<ResolvedPull>,          // concrete mobs, HP, unrolled scripted events
    pub encounters: Vec<ResolvedBoss>,     // per-phase event lists; conditional phase
                                           // triggers armed as HP watchers (lazy)
    pub graph: RouteGraphResolved,         // sampled edge times, skip outcomes
}
```

DSL (parsed from HOCON-ish source per the design doc) expresses: route graph
(nodes/edges/skips with time + risk), pack templates with sampled composition,
mob intake profiles (`melee_stream(dps, var)`, spikes), interruptible casts,
boss phases with conditional (HP) triggers, events with `every T ± j` schedules,
displacement demands `(distance, window, penalty_class)`, forced downtime,
soft enrages, `execution_model` blocks (reaction latency dist, timeline fuzz),
victory conditions, and count requirements. Movement is scalar demands + deadlines
— never coordinates. Damage intake is scripted profiles — never emergent AI.

---

## 6. `sim-observe` — contracts

```rust
pub struct InfoSet {
    pub timeline: TimelineKnowledge,   // Exact | Fuzzy{prior_substitution}
    pub hidden_rng: Visibility,        // proc ICD/RPPM internals
    pub enemy_state: Visibility,       // CastsOnly | Full
    pub ally_state: AllyDetail,        // Compact | Full
    pub reaction: LatencyModel,        // applied to non-anticipated wakes only
}
pub trait Projector {
    fn observe(&self, sim: &SimState, actor: ActorId, dp: &DecisionPoint,
               iset: &InfoSet, priors: &TimelinePriors) -> PlayerObservation;
}
```

Named presets: `oracle` (exact, 0ms), `realistic` (fuzzy priors, lognormal
latency, hidden RNG masked), `debug` (oracle + sim internals). Realistic mode
substitutes beliefs for truth (priors for event times, smoothed TTD) — required,
not optional.

---

## 7. `sim-policy` — contracts

```rust
pub trait Policy: Send {
    fn decide(&mut self, obs: &PlayerObservation, plan: &PlanView,
              mask: &ActionMask) -> Action;
    fn describe(&self) -> PolicyDesc;   // kind, params hash, source ref
}
```

Implementations in scaffold order:

1. `ScriptedPolicy` (fixed sequence; for engine tests).
2. `RuleDslPolicy` — interprets `.rules` files: ordered `when <predicate-expr> [and
plan.<window/allows>] -> use <slot> [target] | wait <wait-expr>`; first match
   wins; numeric literals may be named params `θ_i` exposed as an `f32` vector for
   `sim-optimize` (structure human-readable, parameters machine-tunable).
3. `AplAdapter` — parses the SimC APL subset sufficient for the chosen first spec;
   exists for baseline diffing, not completeness.

Action-space conventions (document in codex, enforce in mask): keep dominated
abilities in the space (the optimizer earns the pruning); off-GCD weaving needs no
special code (mid-GCD decision points expose only off-GCD + waits); target head
active only for slots whose codex def demands it.

---

## 8. `sim-planner` — contracts

```rust
pub trait Planner {
    fn plan(&self, dungeon: &DungeonSpec, roster: &[CharacterConfig],
            eval: &dyn Evaluator, budget: Budget) -> Plan;
}
pub struct Plan {   // immutable artifact; serde to YAML = the human deliverable
    pub route: Vec<RouteStep>,
    pub pulls: Vec<PullPlan>,       // packs, lust, per-actor CD schedule,
                                    // interrupt rotation, target priority,
                                    // externals, movement-CD holds
    pub constraints: PlanConstraints,   // death_prob_max per pull, mana budgets
}
```

Planner v0 scores candidates through the Evaluator using _coarse pull models_
(fitted throughput/danger scalars), not full rollouts; the interface is identical
either way — fidelity of the evaluator backend is the planner's dial, not its code.

---

## 9. `sim-eval` — contracts

```rust
pub trait Objective { fn score(&self, outcomes: &[Outcome]) -> Score; }
// provided: Mean, Quantile(q), CVaR(q), ProbUnder(threshold) — all over kill_time,
// with wipe handling explicit; chance-constraint checks (P(death) per pull) reported
// alongside, never silently folded into the score.

pub struct EvalRequest {
    pub roster: Vec<CharacterConfig>, pub plan: PlanRef, pub policies: Vec<PolicyRef>,
    pub sampler: SamplerRef, pub iset: InfoSetRef,
    pub n: u32, pub seed_base: Seed,
    pub paired_with: Option<RunId>,   // CRN: reuse identical seed set + streams
}
pub trait Evaluator {
    fn evaluate(&self, req: EvalRequest) -> EvalReport;  // score, CI, per-seed outcomes,
}                                                        // paired diff if requested
```

Also owns: successive-halving budget allocation for candidate sets, and the
rule that _all_ scores in the system flow through here (invariant 9).

---

## 10. `sim-oracle`, `sim-optimize`, `sim-calibrate`, `sim-trace` — brief contracts

- **oracle**: `fn search(engine, cx, iset, budget) -> (Action, SearchStats)` — MCTS
  using `snapshot/restore`; runs on `oracle`/`debug` InfoSet by design; emits traces
  for distillation and the standing oracle-vs-readable gap metric.
- **optimize**: `PolicyOptimizer` (CMA-ES over RuleDsl θ-vectors; iterated best
  response across seats via Evaluator paired runs) and `ConfigOptimizer`
  (MAP-Elites archive binned by build archetype; warm-start policies from nearest
  archive neighbor; cold-start on talent-topology changes).
- **calibrate**: log ingest (combat-log event stream -> trajectories); _replay
  validation_ (feed logged human action sequences through the engine, compare
  per-mechanic distributions, emit `Divergence{codex_entry, effect, ci}`);
  _fitting_ (intake profiles, demand distributions, edge times, reaction-latency
  by percentile → DSL params + InfoSet models); _protocol generator_ (divergence →
  discriminating dummy-test protocol with sample-size math, exported as an
  addon-readable checklist; ingests the addon's structured export; writes
  provenance-stamped codex annotations). Human actuators only.
- **trace**: replay files (context + full decision log), `counterfactual(trace,
decision_idx, alt_action) -> ΔOutcome` via snapshot-restore-forceroll (exact
  per-decision regret), and policy/plan diffing ("v13 holds trinket 8s longer in P2").

---

## 11. `simctl` run manifest (the single top-level input)

```yaml
run:
  engine: reference
  codex_build: "11.2.5.61188"
  dungeon: specs/halls_of_example.enc
  roster:
    [
      chars/feral.yaml,
      chars/mage.yaml,
      chars/vdh.yaml,
      chars/rdruid.yaml,
      chars/bm.yaml,
    ]
  plan: plans/halls_v4.plan.yaml # or `plan: solve` to invoke sim-planner
  policies:
    { feral: rules:policies/feral_v12.rules, mage: apl:simc/mage.simc, ... }
  information_set: realistic
  n_rollouts: 50000
  seed_base: 42
  objective: quantile(0.05)
  paired_baseline: runs/incumbent.runid # optional CRN diff
  record_traces: top_k(50) + bottom_k(50)
```

Commands: `simctl run`, `simctl replay <trace>`, `simctl counterfactual`,
`simctl diff <runA> <runB>`, `simctl codex diff <buildA> <buildB>`,
`simctl calibrate ingest|validate|protocol`, `simctl plan solve`.

---

## 12. Build order and acceptance gates

- **G0** `sim-types` + `sim-engine` + `sim-mechanics` + `toy_codex` (toy spec
  defined entirely in effect IR, no Scripted): event loop, waits/predicates,
  generation invalidation, snapshot/restore, determinism CI (same seed → identical
  trace hash across 2 platforms), livelock detection. Property tests: no event
  processed out of order; analytic materialization exact at random probe times.
- **G1** `sim-codex` for ONE mechanically simple spec (recommend Fury-warrior-like:
  instant casts, lumpy resource, charges, one maintenance buff) + `AplAdapter`:
  Patchwerk DPS distribution matches SimC within CI for that spec; golden replays.
- **G2** `sim-observe` + `sim-policy` RuleDsl + `sim-oracle` + `sim-encounter` v0
  (single boss, stochastic events, scalar movement): produce the first oracle-gap
  report (oracle vs. APL vs. rules under `realistic`), with counterfactual top-regret
  decision list. This is the first shippable artifact.
- **G3** (parallel) `sim-planner` + route graph for one dungeon with coarse models:
  emitted Plan validates against a published route's timing.
- **G4** `sim-eval` full (CRN, quantiles, halving) + `sim-optimize`: QD archive over
  one spec's talent/gear space; every headline claim reproducible from a manifest.
- **G5** five-actor team sim: scripted intake, chance-constraints, factored policies
  - plan interface, best response; end-to-end (route, plan, five policies) output.

Non-goals for this scaffold (do not build): GPU/vectorized kernel, neural policies,
2D positioning, live-game input of any kind, more than one spec before G4.

---

## 13. What "done" looks like

`simctl run` on a manifest produces: a scored EvalReport with CIs, a
human-readable Plan Artifact, the rule-policy files, top-regret decision traces
with exact counterfactuals, and provenance-complete codex references — such that a
skeptical theorycrafter can replay any trace deterministically and audit every
claim. The system's health metrics, tracked from G1 onward: patch-turnaround time
of the codex/calibrate loop, and effect-IR coverage (fraction of abilities
expressible without `Scripted` escape hatches, §3b).
