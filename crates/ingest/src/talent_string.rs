//! In-game talent loadout strings (`talents=` in a `.simc` profile), read
//! against SimC's generated `trait_data.inc`.
//!
//! The format follows Blizzard's `ClassTalentImportExport.lua` as SimC's
//! `parse_traits_hash` reads it: base64 digits of 6 bits, least significant
//! bit first; an 8-bit version, a 16-bit spec, a 128-bit tree hash, then one
//! record per node of the class, in ascending node id.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use portunus_core::{SpecId, SpellId};

use crate::IngestError;

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const SERIALIZATION_VERSION: u32 = 2;
const VERSION_BITS: u32 = 8;
const SPEC_BITS: u32 = 16;
const TREE_HASH_BITS: u32 = 128;
const RANK_BITS: u32 = 6;
const CHOICE_BITS: u32 = 2;
const DIGIT_BITS: usize = 6;

/// Which tree a trait belongs to (`talent_tree` in SimC).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TraitTree {
    Class,
    Spec,
    Hero,
    /// The node that picks a hero tree.
    Selection,
}

/// `trait_node_type_e`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraitNodeType {
    Normal,
    /// Ranks spread over the node's entries in order.
    Tiered,
    Choice,
    /// A choice between hero trees.
    SubTreeSelection,
}

/// One entry of `trait_data.inc`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitRow {
    pub tree: TraitTree,
    /// SimC's class index (1 Warrior … 13 Evoker).
    pub class_index: u32,
    pub entry: u32,
    pub node: u32,
    pub max_ranks: u32,
    pub spell: SpellId,
    pub name: String,
    /// Specs that may take it; empty when every spec of the class may.
    pub specs: Vec<SpecId>,
    /// SimC's hero tree id, 0 outside hero trees.
    pub sub_tree: u32,
    pub node_type: TraitNodeType,
}

/// Every row of `trait_data.inc`, in file order.
#[derive(Debug, Clone, Default)]
pub struct TraitTable {
    pub rows: Vec<TraitRow>,
}

/// A talent taken in a decoded loadout string. A tiered node is one pick:
/// its first entry's spell, with the ranks of all its entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TalentPick {
    pub tree: TraitTree,
    pub node: u32,
    pub spell: SpellId,
    pub name: String,
    pub rank: u32,
    /// SimC's hero tree id, 0 outside hero trees.
    pub sub_tree: u32,
}

/// A decoded loadout string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedTalents {
    pub spec: SpecId,
    /// The SimC hero tree id the string selects, when it selects one.
    pub hero_sub_tree: Option<u32>,
    /// Class, spec, and selected hero tree picks. Picks in the hero tree the
    /// string doesn't select are dropped, as the game ignores them.
    pub picks: Vec<TalentPick>,
}

impl TraitTable {
    pub fn from_file(path: &Path) -> Result<Self, IngestError> {
        let text = fs::read_to_string(path)
            .map_err(|e| IngestError::Io(format!("{}: {e}", path.display())))?;
        Self::parse_str(&text)
    }

    /// Reads the rows of the `__trait_data_data` array.
    pub fn parse_str(text: &str) -> Result<Self, IngestError> {
        let mut rows = Vec::new();
        let mut in_live = false;
        for line in text.lines() {
            if line.contains("__trait_data_data") {
                in_live = true;
                continue;
            }
            if !in_live {
                continue;
            }
            let line = line.trim();
            if line.starts_with("} }") {
                break;
            }
            if line.starts_with('{') {
                if let Some(row) = parse_row(line)? {
                    rows.push(row);
                }
            }
        }
        if rows.is_empty() {
            return Err(IngestError::Parse("trait_data: no rows".into()));
        }
        Ok(Self { rows })
    }

    /// Decodes an in-game loadout string.
    pub fn decode(&self, talents: &str) -> Result<DecodedTalents, IngestError> {
        let mut bits = BitReader::new(talents)?;
        let version = bits.take(VERSION_BITS);
        if version != SERIALIZATION_VERSION {
            return Err(IngestError::Parse(format!(
                "talents: serialization version {version}, expected {SERIALIZATION_VERSION}"
            )));
        }
        let spec_raw = bits.take(SPEC_BITS);
        let spec = SpecId(
            u16::try_from(spec_raw)
                .map_err(|_| IngestError::Parse(format!("talents: spec {spec_raw}")))?,
        );
        for _ in 0..TREE_HASH_BITS / 32 {
            bits.take(32);
        }
        let class_index = self
            .rows
            .iter()
            .find(|r| r.tree == TraitTree::Spec && r.specs.contains(&spec))
            .map(|r| r.class_index)
            .ok_or_else(|| IngestError::Parse(format!("talents: no class has spec {}", spec.0)))?;

        let nodes = self.class_nodes(class_index);
        let mut picks = Vec::new();
        let mut hero_sub_tree = None;
        for (node_id, entries) in &nodes {
            if bits.take(1) == 0 {
                continue;
            }
            let first = entries[0];
            let max_rank = if first.node_type == TraitNodeType::Tiered {
                entries.iter().map(|e| e.max_ranks).sum()
            } else {
                first.max_ranks
            };
            let mut rank = max_rank;
            let mut chosen = first;
            if bits.take(1) == 0 {
                rank = 1;
            } else {
                if bits.take(1) == 1 {
                    rank = bits.take(RANK_BITS);
                    if rank >= max_rank {
                        return Err(IngestError::Parse(format!(
                            "talents: node {node_id} partial rank {rank} of {max_rank}"
                        )));
                    }
                }
                if bits.take(1) == 1 {
                    let index = bits.take(CHOICE_BITS) as usize;
                    chosen = entries.get(index).copied().ok_or_else(|| {
                        IngestError::Parse(format!("talents: node {node_id} choice {index}"))
                    })?;
                }
            }
            let applies = first.tree == TraitTree::Hero
                || first.specs.is_empty()
                || first.specs.contains(&spec);
            if !applies {
                if first.tree == TraitTree::Selection {
                    continue;
                }
                return Err(IngestError::Parse(format!(
                    "talents: node {node_id} isn't available to spec {}",
                    spec.0
                )));
            }
            if chosen.tree == TraitTree::Selection {
                hero_sub_tree = Some(chosen.sub_tree);
                continue;
            }
            picks.push(pick(chosen, rank));
        }
        picks.retain(|p| p.tree != TraitTree::Hero || Some(p.sub_tree) == hero_sub_tree);
        Ok(DecodedTalents {
            spec,
            hero_sub_tree,
            picks,
        })
    }
}

impl TraitTable {
    /// Encodes a loadout as an in-game string, with a zero tree hash as
    /// SimC writes it. Every pick is written as purchased.
    pub fn encode(&self, talents: &DecodedTalents) -> Result<String, IngestError> {
        let spec = talents.spec;
        let class_index = self
            .rows
            .iter()
            .find(|r| r.tree == TraitTree::Spec && r.specs.contains(&spec))
            .map(|r| r.class_index)
            .ok_or_else(|| IngestError::Parse(format!("talents: no class has spec {}", spec.0)))?;
        let nodes = self.class_nodes(class_index);
        let mut bits = BitWriter::default();
        bits.put(VERSION_BITS, SERIALIZATION_VERSION);
        bits.put(SPEC_BITS, u32::from(spec.0));
        for _ in 0..TREE_HASH_BITS / 32 {
            bits.put(32, 0);
        }
        let mut placed = 0;
        for entries in nodes.values() {
            let first = entries[0];
            let chosen = if first.tree == TraitTree::Selection {
                talents.hero_sub_tree.and_then(|sub| {
                    entries
                        .iter()
                        .position(|e| e.sub_tree == sub && e.specs.contains(&spec))
                        .map(|i| (i, 1))
                })
            } else {
                talents
                    .picks
                    .iter()
                    .filter(|p| p.node == first.node)
                    .find_map(|p| {
                        entries
                            .iter()
                            .position(|e| e.spell == p.spell)
                            .map(|i| (i, p.rank))
                    })
            };
            let Some((index, rank)) = chosen.filter(|&(_, rank)| rank > 0) else {
                bits.put(1, 0);
                continue;
            };
            if first.tree != TraitTree::Selection {
                placed += 1;
            }
            let max_rank = if first.node_type == TraitNodeType::Tiered {
                entries.iter().map(|e| e.max_ranks).sum()
            } else {
                entries[index].max_ranks
            };
            bits.put(1, 1);
            bits.put(1, 1);
            if rank < max_rank {
                bits.put(1, 1);
                bits.put(RANK_BITS, rank);
            } else {
                bits.put(1, 0);
            }
            let is_choice = matches!(
                first.node_type,
                TraitNodeType::Choice | TraitNodeType::SubTreeSelection
            );
            if is_choice {
                bits.put(1, 1);
                bits.put(CHOICE_BITS, index as u32);
            } else {
                bits.put(1, 0);
            }
        }
        if placed != talents.picks.len() {
            return Err(IngestError::Parse(format!(
                "talents: {} of {} picks have no node in spec {}'s class",
                talents.picks.len() - placed,
                talents.picks.len(),
                spec.0
            )));
        }
        Ok(bits.finish())
    }

    /// The class's rows grouped by node, in the order the string walks them.
    fn class_nodes(&self, class_index: u32) -> BTreeMap<u32, Vec<&TraitRow>> {
        let mut rows: Vec<&TraitRow> = self
            .rows
            .iter()
            .filter(|r| r.class_index == class_index)
            .collect();
        rows.sort_by_key(|r| r.tree);
        let mut nodes: BTreeMap<u32, Vec<&TraitRow>> = BTreeMap::new();
        for row in rows {
            nodes.entry(row.node).or_default().push(row);
        }
        nodes
    }

    /// SimC's hero tree id for the sub tree holding `spell`.
    pub fn hero_sub_tree_of(&self, spell: SpellId) -> Option<u32> {
        self.rows
            .iter()
            .find(|r| r.tree == TraitTree::Hero && r.spell == spell)
            .map(|r| r.sub_tree)
    }
}

fn pick(row: &TraitRow, rank: u32) -> TalentPick {
    TalentPick {
        tree: row.tree,
        node: row.node,
        spell: row.spell,
        name: row.name.clone(),
        rank,
        sub_tree: row.sub_tree,
    }
}

/// `None` for rows outside the player's loadout (SimC's `EXPANSION` tree
/// and later).
fn parse_row(line: &str) -> Result<Option<TraitRow>, IngestError> {
    let bad = || IngestError::Parse(format!("trait_data row: {line}"));
    let open = line.find('"').ok_or_else(bad)?;
    let close = open + 1 + line[open + 1..].find('"').ok_or_else(bad)?;
    let numbers: Vec<u32> = line[1..open]
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<i64>().map(|v| v.max(0) as u32))
        .collect::<Result<_, _>>()
        .map_err(|_| bad())?;
    let [tree, class_index, entry, node, max_ranks, _req, _def, spell, ..] = numbers[..] else {
        return Err(bad());
    };
    let tree = match tree {
        1 => TraitTree::Class,
        2 => TraitTree::Spec,
        3 => TraitTree::Hero,
        4 => TraitTree::Selection,
        _ => return Ok(None),
    };
    let rest = &line[close + 1..];
    let lists: Vec<&str> = rest.split('{').skip(1).collect();
    let spec_list = lists
        .first()
        .and_then(|s| s.split('}').next())
        .ok_or_else(bad)?;
    let specs = spec_list
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "0")
        .map(|s| s.parse::<u16>().map(SpecId))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| bad())?;
    let tail: Vec<u32> = rest
        .rsplit('}')
        .nth(1)
        .ok_or_else(bad)?
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()
        .map_err(|_| bad())?;
    let [sub_tree, node_type] = tail[..] else {
        return Err(bad());
    };
    Ok(Some(TraitRow {
        tree,
        class_index,
        entry,
        node,
        max_ranks,
        spell: SpellId(spell),
        name: line[open + 1..close].to_owned(),
        specs,
        sub_tree,
        node_type: match node_type {
            0 => TraitNodeType::Normal,
            1 => TraitNodeType::Tiered,
            2 => TraitNodeType::Choice,
            3 => TraitNodeType::SubTreeSelection,
            _ => return Err(bad()),
        },
    }))
}

#[derive(Default)]
struct BitWriter {
    out: String,
    head: usize,
    digit: u32,
}

impl BitWriter {
    fn put(&mut self, n: u32, value: u32) {
        for i in 0..n {
            let bit = self.head % DIGIT_BITS;
            self.digit |= ((value >> i) & 1) << bit;
            self.head += 1;
            if bit == DIGIT_BITS - 1 {
                self.out.push(char::from(BASE64[self.digit as usize]));
                self.digit = 0;
            }
        }
    }

    fn finish(mut self) -> String {
        if !self.head.is_multiple_of(DIGIT_BITS) {
            self.out.push(char::from(BASE64[self.digit as usize]));
        }
        self.out
    }
}

struct BitReader {
    digits: Vec<u32>,
    head: usize,
}

impl BitReader {
    fn new(s: &str) -> Result<Self, IngestError> {
        let digits = s
            .bytes()
            .map(|b| {
                BASE64
                    .iter()
                    .position(|&c| c == b)
                    .map(|p| p as u32)
                    .ok_or_else(|| {
                        IngestError::Parse(format!("talents: bad digit {:?}", b as char))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { digits, head: 0 })
    }

    /// The next `n` bits (at most 32), least significant first; zeros past
    /// the end.
    fn take(&mut self, n: u32) -> u32 {
        let mut value = 0;
        for i in 0..n {
            let digit = self
                .digits
                .get(self.head / DIGIT_BITS)
                .copied()
                .unwrap_or(0);
            value |= ((digit >> (self.head % DIGIT_BITS)) & 1) << i;
            self.head += 1;
        }
        value
    }
}
