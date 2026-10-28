pub const OPCODE_LEN: usize = 2;

pub const HEARTBEAT: u16 = 0x0036;
pub const SELF_INFO: u16 = 0x3336;
pub const PLAYER_INFO: u16 = 0x4536;
pub const PLAYER_INFO_OLD: u16 = 0x4436;
pub const SPAWN: u16 = 0x4136;
pub const DEATH: u16 = 0x4236;
pub const MAP_LOAD: u16 = 0x2136;
pub const CAST: u16 = 0x0238;
pub const HIT_NOTICE: u16 = 0x0338;
pub const DAMAGE: u16 = 0x0438;
pub const DOT: u16 = 0x0538;
pub const REMAIN_HP: u16 = 0x008D;
pub const PARTY_ROSTER: u16 = 0x0297;
pub const PARTY_JOIN: u16 = 0x0B97;
pub const PARTY_MEMBER_LEFT: u16 = 0x1697;
pub const PARTY_LEFT: u16 = 0x1D97;
pub const PARTY_UPDATE: u16 = 0x1F97;
pub const PET_COLLECTION: u16 = 0x0090;
pub const PET_SOULS: u16 = 0x0D90;
pub const SELF_STATE: u16 = 0x4A36;
pub const EFFECT_SOURCE: u16 = 0x048D;
pub const DAILY_KINAH_STATE: u16 = 0xA5FF;
pub const DAILY_KINAH: u16 = 0xA6FF;
pub const DAILY_GATHERING_STATE: u16 = 0xA7FF;
pub const DAILY_GATHERING: u16 = 0xA8FF;
pub const ABYSS_POINTS: u16 = 0xABFF;
pub const INVENTORY: u16 = 0x1156;
pub const INVENTORY_ITEM: u16 = 0x1B56;

pub fn name(opcode: u16) -> Option<&'static str> {
    Some(match opcode {
        HEARTBEAT => "Heartbeat",
        SELF_INFO => "SelfInfo",
        PLAYER_INFO => "PlayerInfo",
        SPAWN => "Spawn",
        DEATH => "Death",
        MAP_LOAD => "MapLoad",
        CAST => "Cast",
        HIT_NOTICE => "HitNotice",
        DAMAGE => "Damage",
        DOT => "Dot",
        REMAIN_HP => "RemainHp",
        PARTY_ROSTER => "PartyRoster",
        PARTY_JOIN => "PartyJoin",
        PARTY_MEMBER_LEFT => "PartyMemberLeft",
        PARTY_LEFT => "PartyLeft",
        PARTY_UPDATE => "PartyUpdate",
        PET_COLLECTION => "PetCollection",
        PET_SOULS => "PetSouls",
        DAILY_KINAH_STATE | DAILY_KINAH => "DailyKinah",
        DAILY_GATHERING_STATE | DAILY_GATHERING => "DailyGathering",
        ABYSS_POINTS => "AbyssPoints",
        INVENTORY => "Inventory",
        INVENTORY_ITEM => "InventoryItem",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn known_and_unknown() {
        assert_eq!(super::name(super::DAMAGE), Some("Damage"));
        assert_eq!(super::name(0xBEEF), None);
    }
}
