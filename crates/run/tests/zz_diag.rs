use std::path::PathBuf;

use portunus_env::Env;
use portunus_eval::SeedSet;
use portunus_run::{default_simc_profile, Bundle};

#[test]
#[ignore]
fn diag() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let simc = root.join("target/simc");
    let run = std::env::var("RUN").expect("RUN");
    let mut b = Bundle::load(&root.join(format!("data/runs/{run}.ron"))).expect("bundle");
    let diffs = b
        .sync_with_dump(&simc.join("SpellDataDump"))
        .unwrap_or_default();
    if let Ok(ids) = std::env::var("DIFFS") {
        let ids: Vec<u32> = ids.split(',').filter_map(|s| s.parse().ok()).collect();
        for d in &diffs {
            if ids.is_empty() || ids.contains(&d.id) {
                println!(
                    "diff {} {} {} {} {} -> {}",
                    d.owner_kind, d.id, d.name, d.field, d.current, d.dump
                );
            }
        }
    }
    let tpl = &b.templates[0];
    let profile = default_simc_profile(tpl.spec, tpl.hero_tree, &simc).expect("profile");
    if std::env::var("CLASS").is_ok() {
        let table = portunus_ingest::talent_string::TraitTable::from_file(
            &simc.join("engine/dbc/generated/trait_data.inc"),
        )
        .expect("table");
        let text = std::fs::read_to_string(&profile).expect("profile text");
        let talents = text
            .lines()
            .find_map(|l| l.strip_prefix("talents="))
            .expect("talents");
        let spec = &b.data.specs[&tpl.spec];
        for p in table.decode(talents).expect("decode").picks {
            if p.tree != portunus_ingest::talent_string::TraitTree::Class
                && std::env::var("CLASS").as_deref() != Ok("all")
            {
                continue;
            }
            let id = p.spell.0;
            let baseline = spec.baseline_auras.iter().any(|a| a.0 == id)
                || spec.baseline_spells.iter().any(|s| s.0 == id);
            let aura = b.data.auras.contains_key(&portunus_core::AuraId(id));
            let spell = b.data.spells.contains_key(&portunus_core::SpellId(id));
            println!(
                "class {id:8} {:32} rank={} baseline={baseline} aura={aura} spell={spell}",
                p.name, p.rank
            );
        }
    }
    b.adopt_simc_talents(&profile, &simc.join("engine/dbc/generated/trait_data.inc"))
        .expect("adopt");
    b.ingest_simc_profile(&profile).expect("ingest");
    b.configure_patchwerk_dummy();
    if let Ok(ids) = std::env::var("AURA") {
        for id in ids.split(',').filter_map(|s| s.parse::<u32>().ok()) {
            println!("aura {:#?}", b.data.auras.get(&portunus_core::AuraId(id)));
            println!(
                "spell {:#?}",
                b.data.spells.get(&portunus_core::SpellId(id))
            );
        }
    }
    let t = &b.templates[0];
    println!("stats {:?}", t.stats);
    println!("derived {:?}", t.derived);
    println!("mh {:?}\noh {:?}", t.main_hand, t.off_hand);
    if std::env::var("MODS").is_ok() {
        use portunus_gamedata::effect::{Effect, ModKind, ModScope};
        let mut auras: Vec<(portunus_core::AuraId, &str)> =
            t.passive_auras.iter().map(|&a| (a, "passive")).collect();
        for s in &t.abilities {
            if let Some(def) = b.data.spells.get(s) {
                for e in def.effects.iter().chain(&def.on_cast) {
                    if let Effect::ApplyAura { aura, .. } = e {
                        auras.push((*aura, "applied"));
                    }
                }
            }
        }
        for (a, how) in auras {
            let Some(def) = b.data.auras.get(&a) else {
                continue;
            };
            for m in &def.modifiers {
                let broad = matches!(m.scope, ModScope::All | ModScope::School(_));
                if broad && matches!(m.kind, ModKind::DamageDonePct) {
                    println!(
                        "{how:8} {:8} {:40} {:?} {} {:?}",
                        a.0, def.name, m.scope, m.value, m.condition
                    );
                }
            }
        }
    }
    if let Ok(ids) = std::env::var("SPELLMODS") {
        use portunus_gamedata::effect::{Effect, ModScope};
        let mut auras: Vec<(portunus_core::AuraId, &str)> =
            t.passive_auras.iter().map(|&a| (a, "passive")).collect();
        for def in b.data.spells.values() {
            for e in def.effects.iter().chain(&def.on_cast) {
                if let Effect::ApplyAura { aura, .. } = e {
                    auras.push((*aura, "applied"));
                }
            }
        }
        auras.sort();
        auras.dedup_by_key(|a| a.0);
        for id in ids.split(',').filter_map(|s| s.parse::<u32>().ok()) {
            let sid = portunus_core::SpellId(id);
            let school = b.data.spells.get(&sid).map_or(0, |d| d.school.0);
            println!("== spell {id} school {school}");
            for &(a, how) in &auras {
                let Some(def) = b.data.auras.get(&a) else {
                    continue;
                };
                for m in &def.modifiers {
                    let hits = match m.scope {
                        ModScope::Spell(s) => s == sid,
                        ModScope::School(mask) => mask.0 & school != 0,
                        ModScope::All | ModScope::Abilities => true,
                        ModScope::AutoAttack => false,
                    };
                    if hits {
                        println!(
                            "  {how:8} {:8} {:36} {:?} {:?} {} {:?}",
                            a.0, def.name, m.scope, m.kind, m.value, m.condition
                        );
                    }
                }
            }
        }
    }
    if std::env::var("SWINGS").is_ok() {
        let arm = b.arm(true);
        let mut env = arm.env.clone();
        let mut turn = env.reset(portunus_core::Seed(1)).expect("reset");
        while let portunus_env::Turn::Decide(d) = turn {
            let choice = arm.policies[usize::from(d.seat.0)].act(&d);
            turn = env.step(choice).expect("step").next;
        }
        let mut shown = 0;
        let want = std::env::var("SPELL")
            .ok()
            .map(|s| portunus_core::SpellId(s.parse().expect("SPELL")));
        let trace = env.drain_trace();
        if let Ok(ids) = std::env::var("UPTIME") {
            use portunus_engine::trace::TraceEvent;
            use std::collections::BTreeMap;
            let ids: Vec<u32> = ids.split(',').filter_map(|s| s.parse().ok()).collect();
            let end = trace.last().map_or(0, |r| u64::from(r.time.millis()));
            let mut open: BTreeMap<(u32, u32), u64> = BTreeMap::new();
            let mut total: BTreeMap<u32, (u64, u32)> = BTreeMap::new();
            for r in &trace {
                match r.event {
                    TraceEvent::AuraApplied { holder, aura, .. } if ids.contains(&aura.0) => {
                        let key = (u32::from(holder.0), aura.0);
                        if let std::collections::btree_map::Entry::Vacant(e) = open.entry(key) {
                            e.insert(u64::from(r.time.millis()));
                            total.entry(aura.0).or_default().1 += 1;
                        }
                    }
                    TraceEvent::AuraRemoved { holder, aura } if ids.contains(&aura.0) => {
                        if let Some(start) = open.remove(&(u32::from(holder.0), aura.0)) {
                            total.entry(aura.0).or_default().0 +=
                                u64::from(r.time.millis()) - start;
                        }
                    }
                    _ => {}
                }
            }
            for ((_, aura), start) in open {
                total.entry(aura).or_default().0 += end - start;
            }
            for (aura, (ms, starts)) in total {
                println!(
                    "uptime {aura:8} {:5.1}% starts={starts}",
                    100.0 * ms as f64 / end.max(1) as f64
                );
            }
        }
        if let Some(sid) = want.filter(|_| std::env::var("ACTIVE").is_ok()) {
            use portunus_engine::trace::TraceEvent;
            use portunus_gamedata::effect::ModScope;
            use std::collections::BTreeSet;
            let school = b.data.spells.get(&sid).map_or(0, |d| d.school.0);
            let mut open: BTreeSet<(u32, u32)> = BTreeSet::new();
            for r in &trace {
                match &r.event {
                    TraceEvent::AuraApplied { holder, aura, .. } => {
                        open.insert((u32::from(holder.0), aura.0));
                    }
                    TraceEvent::AuraRemoved { holder, aura } => {
                        open.remove(&(u32::from(holder.0), aura.0));
                    }
                    TraceEvent::Damage(d) if d.spell == Some(sid) => {
                        let mut names = Vec::new();
                        for &(h, a) in &open {
                            let Some(def) = b.data.auras.get(&portunus_core::AuraId(a)) else {
                                continue;
                            };
                            for m in &def.modifiers {
                                let hits = match m.scope {
                                    ModScope::Spell(s) => s == sid,
                                    ModScope::School(mask) => mask.0 & school != 0,
                                    ModScope::All => true,
                                    ModScope::Abilities | ModScope::AutoAttack => false,
                                };
                                if hits {
                                    names
                                        .push(format!("{h}:{}:{:?}={}", def.name, m.kind, m.value));
                                }
                            }
                        }
                        println!(
                            "{:8.3} {:>8} {}",
                            r.time.millis() as f64 / 1000.0,
                            d.amount,
                            names.join(" | ")
                        );
                    }
                    _ => {}
                }
            }
        }
        for r in trace {
            if let portunus_engine::trace::TraceEvent::Damage(d) = r.event {
                if d.spell == want && shown < 30 {
                    println!(
                        "{:8.3} {:>8} crit={}",
                        r.time.millis() as f64 / 1000.0,
                        d.amount,
                        d.crit
                    );
                    shown += 1;
                }
            }
        }
    }
    let bd = b
        .breakdown(SeedSet {
            first: 1,
            count: 10,
        })
        .expect("breakdown");
    println!("combat {:.1}s", bd.combat_secs);
    let filter = std::env::var("ROWS").unwrap_or_default();
    for r in &bd.rows {
        if filter.is_empty() || filter.split(',').any(|f| r.name.contains(f)) {
            println!(
                "{:32} casts={:7.1} hits={:7.1} crit={:5.1}% avg={:8.0} dps={:8.0}",
                r.name,
                r.casts,
                r.hits,
                100.0 * r.crit_rate,
                r.damage / r.hits.max(1.0),
                r.dps
            );
        }
    }
}
