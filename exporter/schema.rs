use std::convert::TryInto;

#[derive(Debug, Clone, Copy, Hash)]
struct Cooldown {
    spell_id: u32
    time_until_next_charge_ms: u32
    charge_info_flags: u8 // [0000XXYY] X -> current (integral) charges, Y -> max charges (usually Y == 1)
}

#[derive(Debug, Clone, Copy, Hash)]
struct Aura {
    spell_id: u32
    source_guid_hash: u32
    expiration_time_ms: u32
    flags: u8 
    // [UUVVVVVV]
       // U => premultiplier effect active (snapshot relevance)
       // V => stacks of the aura, up to 
}

#[derive(Debug, Clone, Copy, Hash)]
struct Resource {
    current_value: u16
    maximum_value: u16
    resource_type: u8 // See the docs here for enum values: https://wowpedia.fandom.com/wiki/API_UnitPowerMax
}

#[derive(Debug, Clone, Copy, Hash)]
struct UnitBase {
    health_current: u32
    health_maximum: u32
    speed: f32
    spell_cast_spell_id: u32 // if 
    spell_cast_start_time_ms: u32
    spell_cast_end_time_ms: u32
    spell_cast_flags: u8 // [000000XY] w/ X => unit_is_channeling and Y => cast_is_interruptible
    auras: Vec<Aura>
}

#[derive(Debug, Clone, Copy, Hash)]
struct EnemyUnit {
    guid_hash: u32
    range_lb: u8
    range_ub: u8 // distance estimate from player in yards
    flags: u8
    // [0000WXYZ] 
       // W => nameplate visible on screen (usually true unless behind player but not dead)
       // X => unit attackable; 
       // Y => unit in combat with something; 
       // Z => player on unit's threat table
    base: UnitBase
} 

#[derive(Debug, Clone, Copy, Hash)]
struct PlayerUnit {
    resources: Vec<Resource>
    cooldowns: Vec<Cooldown>
    base: UnitBase
}


#[derive(Debug, Clone, Copy, Hash)]
struct Snapshot  {
    snapshot_time_ms: u32
    combat_time_ms: u32
    target_guid_hash: u32
    player: PlayerUnit
    enemies: Vec<EnemyUnit>
}




// DESERIALIZATION CODE
trait FromBytes: Sized {
    fn from_bytes(bytes: &[u8], offset: &mut usize) -> Self;
}

impl FromBytes for u32 {
    fn from_bytes(bytes: &[u8], offset: &mut usize) -> Self {
        let value = Self::from_le_bytes(bytes[*offset .. *offset + 4].try_into().unwrap());
        *offset += 4;
        value
    }
}

impl FromBytes for f32 {
    fn from_bytes(bytes: &[u8], offset: &mut usize) -> Self {
        let value = Self::from_le_bytes(bytes[*offset .. *offset + 4].try_into().unwrap());
        *offset += 4;
        value
    }
}

impl FromBytes for u16 {
    fn from_bytes(bytes: &[u8], offset: &mut usize) -> Self {
        let value = Self::from_le_bytes(bytes[*offset .. *offset + 2].try_into().unwrap());
        *offset += 2;
        value
    }
}

impl FromBytes for u8 {
    fn from_bytes(bytes: &[u8], offset: &mut usize) -> Self {
        let value = data[*offset];
        *offset += 1;
        value;
    }
} 

fn read<T: FromBytes>(data: &[u8], offset: &mut usize) -> T {
    T::from_bytes(data, ofset)
}

// Assume that offset current points to the `size` or `count` parameter encoding the number of items in the vector.
fn read_vec<T>(data: &[u8], offset: &mut usize) -> Vec<T> {
    let count = read(data, offset); 
    let mut vec = Vec::with_capacity(count as usize);
    for _ in 0 .. count {
        vec.push(read(data, offset)); 
    }
    vec
}

// should probably codegen these deserializers but for now it's manual
// CODEGEN_START
impl FromBytes for Cooldown {
    fn from_bytes(data: &[u8], offset: &mut usize) -> Self {
        Self {
            spell_id: read(data, offset),
            ready_time_ms: read(data, offset),
            charge_info_flags: read(data, offset),
        }
    }
}

impl FromBytes for Aura {
    fn from_bytes(data: &[u8], offset: &mut usize) -> Self {
        Self {
            spell_id: read(data, offset),
            source_guid_hash: read(data, offset),
            expiration_time_ms: read(data, offset),
            flags: read(data, offset),
        }
    }
}

impl FromBytes for Resource {
    fn from_bytes(data: &[u8], offset: &mut usize) -> Self {
        Self {
            current_value: read(data, offset),
            maximum_value: read(data, offset),
            resource_type: read(data, offset),
        }
    }
}


impl FromBytes for UnitBase {
    fn from_bytes(data: &[u8], offset: &mut usize) -> Self {
        Self {
            health_current: read(data, offset),
            health_maximum: read(data, offset),
            speed: read(data, offset),
            spell_cast_spell_id: read(data, offset),
            spell_cast_start_time_ms: read(data, offset),
            spell_cast_end_time_ms: read(data, offset),
            spell_cast_flags: read(data, offset),
            auras: read_vec(data, offset),
        }
    }
}

impl FromBytes for EnemyUnit {
    fn from_bytes(data: &[u8], offset: &mut usize) -> Self {
        Self {
            guid_hash: read(data, offset),
            range: read(data, offset),
            flags: read(data, offset),
            base: read(data, offset),
        }
    }
}

impl FromBytes for PlayerUnit {
    fn from_bytes(data: &[u8], offset: &mut usize) -> Self {
        Self {
            resources: read_vec(data, offset),
            cooldowns: read_vec(data, offset),
            base: read(data, offset),
        }
    }
}

impl FromBytes for Snapshot {
    fn from_bytes(data: &[u8], offset: &mut usize) -> Self {
        Self {
            snapshot_time_ms: read(data, offset),
            combat_time_ms: read(data, offset),
            target_guid_hash: read(data, offset),
            player: read(data, offset),
            enemies: read_vec(data, offset),
        }
    }
}
// CODEGEN_END

fn deserialize(data: &[u8]) -> Snapshot {
    let mut offset = 0;
    Snapshot::from_bytes(data, offset);
}