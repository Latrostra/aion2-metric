use std::ops::{Range, RangeInclusive};

use crate::frame::{BUNDLE_HEADER_LEN, BUNDLE_MARKER, MAX_BUNDLE, frame_size};
use crate::opcodes;
use crate::wire::{Cursor, read_varint, u16_le, u32_le};

const ENTITY_IDS: RangeInclusive<u32> = 1..=9_999_999;
const PET_IDS: RangeInclusive<u32> = 1000..=9999;
const CLASS_SKILLS: RangeInclusive<u32> = 10_000_000..=19_999_999;
const SKILL_IDS: RangeInclusive<u32> = 1..=299_999_999;
const SKILL_FAMILY_SIZE: u32 = 10_000;
const LEGACY_SKILL_IDS: RangeInclusive<u32> = 3_000_000..=3_099_999;
const LEGACY_SKILL_SCALE: u32 = 10;
const LEGACY_SKILL_SUFFIX: u32 = 1;
const NPC_TEMPLATES: RangeInclusive<u32> = 1_000_000..=9_999_999;

const MIN_SCANNED_BODY_LEN: usize = 16;
const MAP_ID_OFFSET: usize = 4;

const NAME_PRESENT_FLAG: u8 = 0x01;
const NAME_FIELD_LENS: RangeInclusive<usize> = 1..=36;
const NAME_CHARS: RangeInclusive<usize> = 1..=16;
const HIDDEN_NAME_PREFIX: u8 = b'$';
const IDENTITY_SKIPPED_LEN: usize = 4;
const MIN_EMBEDDED_IDENTITY_LEN: usize = 10;
const EFFECT_SOURCE_SKIPPED_LEN: usize = 2;

const DAMAGE_RECORD_SEPARATOR: [u8; 2] = [0x01, 0x00];
const DAMAGE_RECORD_TRAILER: [u8; 2] = [0x03, 0x00];
const LAYOUT_MASK: u32 = 0x0F;
const ANNOUNCEMENT_LAYOUT: u32 = 0;
const MULTI_HIT_FLAG: u32 = 0x20;
const DAMAGE_LAYOUTS: RangeInclusive<usize> = 4..=7;
const LAYOUT_WITH_HIT_FIELD: usize = 4;
const MIN_BACK_ATTACK_LAYOUT: usize = 5;
const COMPACT_CRIT_LAYOUT: usize = 6;
const BACK_ATTACK_FLAG_OFFSET: usize = 2;
const BACK_ATTACK_FLAG: u8 = 0x01;
const DAMAGE_TYPE_CRIT: u32 = 3;
const VALUE_BLOCK_OFFSETS_BY_LAYOUT: [&[usize]; 8] =
    [&[], &[], &[], &[], &[8], &[11, 12], &[11, 10], &[11, 14]];
const DAMAGE_AMOUNTS: RangeInclusive<u32> = 1..=99_999_999;
const PLAUSIBLE_SCALARS: RangeInclusive<u32> = 1_000..=200_000;
const COMPACT_CRIT_DAMAGE: RangeInclusive<u32> = 1_000..=5_000_000;
const HIT_COUNTS: RangeInclusive<u32> = 1..=25;
const NEXT_RECORD_TAGS: RangeInclusive<u8> = 1..=7;

const CAST_SKIPPED_LEN: usize = 2;
const DOT_DAMAGE_EFFECTS: [u8; 2] = [0x02, 0x0A];
const DOT_SKILL_SCALE: u32 = 100;

const NPC_TEMPLATE_GAP: usize = 3;
const DEATH_KIND_GAP: usize = 1;
const KILLED_DEATH_KINDS: [u8; 2] = [3, 7];

const PET_COLLECTION_HEADER_LEN: usize = 4;
const MAX_PREALLOCATED_ENTRIES: usize = 1024;
const MAX_PET_LEVEL: u32 = 10;
const PET_SOULS_RECORD_LEN: usize = 8;
const U64_LEN: usize = 8;
const DAILY_COUNTER_LEN: usize = 2 * U64_LEN;
const ABYSS_POINTS_LEN: usize = 2 * U64_LEN;
const KINAH_ITEM_TEMPLATE: [u8; 4] = 0x3770_333F_u32.to_le_bytes();
const INVENTORY_ITEM_TEMPLATE_OFFSET: usize = 11;
const PET_SOUL_AMOUNTS: RangeInclusive<u32> = 1..=1000;

const LARGE_SPAWN_ID_THRESHOLD: u32 = 1_000_000;
const FOLDED_SPAWN_ID_MASK: u32 = 0x3FFF;
const FOLDED_SPAWN_ID_BIT: u32 = 0x4000;
const SPAWN_KIND_WITH_PARENT: u8 = 0x5F;
const SUMMON_KINDS: [u8; 5] = [0x5F, 0x1C, 0x1F, 0x1D, 0x5D];
const SPIRIT_SUMMON_KINDS: [u8; 4] = [0x5F, 0x1F, 0x1D, 0x5D];
const PARENT_BLOCK_FLAG: u32 = 0x10;
const OWNER_NAME_FIELD_OFFSETS: [usize; 2] = [4, 2];
const OWNER_SEARCH_DEFAULT_OFFSET: usize = 5;
const SPIRIT_ANCHOR: [u8; 8] = [0x80, 0x75, 0xD5, 0x2A, 0xBB, 0x03, 0x00, 0x00];
const SPIRIT_ANCHOR_SEARCH_LEN: usize = 128;

const PARENT_BLOCK_TAG: u8 = 0x06;
const PARENT_ZERO_OFFSET: usize = 8;
const PARENT_MARKER_OFFSET: usize = 10;
const PARENT_NAME_LEN_OFFSET: usize = 12;
const PARENT_NAME_OFFSET: usize = 13;
const PARENT_MARKERS: Range<u16> = 1000..3000;
const PARENT_NAME_LENS: RangeInclusive<usize> = 1..=48;

const MAX_EMBEDDED_LENGTH_VARINT_BYTES: usize = 3;
const MIN_EMBEDDED_BUNDLE_DECLARED_LEN: u32 = 12;
const MIN_EMBEDDED_BUNDLE_TAIL: usize = 8;

const PARTY_CHAR_ID_BEFORE_NAME: usize = 8;
const PARTY_MEMBER_MARKER_GAP: usize = 4;
const PARTY_MEMBER_MARKER: [u8; 4] = [0x2D, 0x00, 0x00, 0x00];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub actor: u32,
    pub target: u32,
    pub skill: u32,
    pub amount: u32,
    pub crit: bool,
    pub back: bool,
    pub dot: bool,
    pub hits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameEvent {
    Hit(Hit),
    Identity {
        id: u32,
        name: String,
        is_self: bool,
    },
    Cast {
        actor: u32,
        target: u32,
        skill: u32,
    },
    Summon {
        id: u32,
        owner: Option<u32>,
        owner_name: Option<String>,
    },
    MapLoad {
        map: u32,
    },
    SelfId {
        id: u32,
    },
    Caster {
        id: u32,
        name: String,
    },
    Party {
        members: Vec<PartyMember>,
    },
    PartyJoin(PartyMember),
    PartyMemberLeft {
        char_id: u32,
    },
    PartyLeft,
    NpcSpawn {
        id: u32,
        template: u32,
    },
    Death {
        id: u32,
    },
    PetCollection {
        levels: Vec<(u32, u8)>,
        souls: Vec<(u32, u32)>,
    },
    PetSouls(Vec<(u32, u32)>),
    KinahBalance(u64),
    DailyKinah(u64),
    DailyGathering(u64),
    AbyssPoints {
        current: u64,
        limit: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyMember {
    pub char_id: u32,
    pub name: String,
}

fn is_pet(id: u32) -> bool {
    PET_IDS.contains(&id)
}

pub fn is_entity(id: u32) -> bool {
    ENTITY_IDS.contains(&id)
}

pub fn is_class_skill(skill: u32) -> bool {
    CLASS_SKILLS.contains(&skill)
}

fn is_valid_skill(skill: u32) -> bool {
    SKILL_IDS.contains(&skill)
}

pub fn base_skill(skill: u32) -> u32 {
    if is_class_skill(skill) {
        skill - skill % SKILL_FAMILY_SIZE
    } else {
        skill
    }
}

fn read_entity(cursor: &mut Cursor) -> Option<u32> {
    cursor.varint().filter(|&id| is_entity(id))
}

fn read_skill(cursor: &mut Cursor) -> Option<u32> {
    cursor.u32().filter(|&skill| is_valid_skill(skill))
}

fn read_name_field<'a>(cursor: &mut Cursor<'a>) -> Option<&'a [u8]> {
    let len = usize::from(cursor.u8()?);
    if !NAME_FIELD_LENS.contains(&len) {
        return None;
    }
    cursor.take(len)
}

pub fn parse(opcode: u16, body: &[u8], out: &mut Vec<GameEvent>) {
    match opcode {
        opcodes::DAMAGE => parse_damage(body, out),
        opcodes::DOT => out.extend(parse_dot(body).map(GameEvent::Hit)),
        opcodes::CAST => out.extend(parse_cast(body)),
        opcodes::SPAWN => {
            out.extend(parse_spawn(body));
            out.extend(parse_npc_template(body));
        }
        opcodes::DEATH => out.extend(parse_death(body)),
        opcodes::PET_COLLECTION => out.extend(parse_pet_collection(body)),
        opcodes::PET_SOULS => out.extend(parse_pet_souls(body)),
        opcodes::DAILY_KINAH_STATE | opcodes::DAILY_KINAH => {
            out.extend(parse_daily_counter(body).map(GameEvent::DailyKinah))
        }
        opcodes::DAILY_GATHERING_STATE | opcodes::DAILY_GATHERING => {
            out.extend(parse_daily_counter(body).map(GameEvent::DailyGathering))
        }
        opcodes::ABYSS_POINTS => out.extend(parse_abyss_points(body)),
        opcodes::INVENTORY => {
            out.extend(parse_inventory_kinah(body));
            scan_unknown(body, out);
        }
        opcodes::INVENTORY_ITEM => {
            out.extend(parse_inventory_item_kinah(body));
            scan_unknown(body, out);
        }
        opcodes::MAP_LOAD => {
            out.extend(u32_le(body, MAP_ID_OFFSET).map(|map| GameEvent::MapLoad { map }))
        }
        opcodes::SELF_INFO => {
            out.extend(parse_identity(body, true));
            scan_embedded_bundles(body, out);
        }
        opcodes::PLAYER_INFO | opcodes::PLAYER_INFO_OLD => {
            out.extend(parse_identity(body, false));
            scan_embedded_bundles(body, out);
        }
        opcodes::SELF_STATE => {
            out.extend(read_entity(&mut Cursor::new(body)).map(|id| GameEvent::SelfId { id }))
        }
        opcodes::EFFECT_SOURCE => out.extend(parse_effect_source(body)),
        opcodes::PARTY_ROSTER => out.push(GameEvent::Party {
            members: party_members(body),
        }),
        opcodes::PARTY_JOIN | opcodes::PARTY_UPDATE => {
            out.extend(party_members(body).into_iter().map(GameEvent::PartyJoin));
        }
        opcodes::PARTY_MEMBER_LEFT => out.extend(
            Cursor::new(body)
                .u32()
                .map(|char_id| GameEvent::PartyMemberLeft { char_id }),
        ),
        opcodes::PARTY_LEFT => out.push(GameEvent::PartyLeft),
        _ => scan_unknown(body, out),
    }
}

fn scan_unknown(body: &[u8], out: &mut Vec<GameEvent>) {
    if body.len() >= MIN_SCANNED_BODY_LEN {
        scan_identities(body, out);
        scan_embedded_bundles(body, out);
    }
}

fn parse_damage(body: &[u8], out: &mut Vec<GameEvent>) {
    let mut pos = 0;
    let mut first = true;
    while pos < body.len() {
        if body[pos..].starts_with(&DAMAGE_RECORD_SEPARATOR) {
            pos += DAMAGE_RECORD_SEPARATOR.len();
        } else if !first {
            break;
        }
        if first && is_announcement(body, pos) {
            out.extend(parse_announcement(body, pos));
            return;
        }
        let Some((hit, end, clean)) = damage_record(body, pos) else {
            break;
        };
        out.push(GameEvent::Hit(hit));
        pos = end;
        first = false;
        if !clean {
            break;
        }
    }
}

fn is_announcement(body: &[u8], start: usize) -> bool {
    let mut cursor = Cursor::at(body, start);
    cursor.varint().is_some()
        && cursor
            .varint()
            .is_some_and(|record_flags| record_flags & LAYOUT_MASK == ANNOUNCEMENT_LAYOUT)
}

fn parse_announcement(body: &[u8], start: usize) -> Option<GameEvent> {
    let mut cursor = Cursor::at(body, start);
    let target = read_entity(&mut cursor)?;
    cursor.varint()?;
    cursor.varint()?;
    let actor = read_entity(&mut cursor)?;
    let skill = read_skill(&mut cursor)?;
    Some(GameEvent::Cast {
        actor,
        target,
        skill,
    })
}

fn parse_cast(body: &[u8]) -> Option<GameEvent> {
    let mut cursor = Cursor::new(body);
    let actor = read_entity(&mut cursor)?;
    cursor.varint()?;
    let skill = read_skill(&mut cursor)?;
    cursor.take(CAST_SKIPPED_LEN)?;
    let target = read_entity(&mut cursor)?;
    Some(GameEvent::Cast {
        actor,
        target,
        skill,
    })
}

fn parse_npc_template(body: &[u8]) -> Option<GameEvent> {
    let mut cursor = Cursor::new(body);
    let id = read_entity(&mut cursor)?;
    let template = u32_le(body, cursor.pos + NPC_TEMPLATE_GAP)
        .filter(|template| NPC_TEMPLATES.contains(template))?;
    Some(GameEvent::NpcSpawn { id, template })
}

fn parse_pet_collection(body: &[u8]) -> Option<GameEvent> {
    let mut cursor = Cursor::new(body);
    cursor.take(PET_COLLECTION_HEADER_LEN)?;
    let level_count = cursor.varint()? as usize;
    let mut levels = Vec::with_capacity(level_count.min(MAX_PREALLOCATED_ENTRIES));
    for _ in 0..level_count {
        let (id, repeated_id, level) = (cursor.u32()?, cursor.u32()?, cursor.u32()?);
        if id != repeated_id || !is_pet(id) || level > MAX_PET_LEVEL {
            return None;
        }
        levels.push((id, level as u8));
    }
    let soul_count = cursor.varint()? as usize;
    let mut souls = Vec::with_capacity(soul_count.min(MAX_PREALLOCATED_ENTRIES));
    for _ in 0..soul_count {
        let (id, amount) = (cursor.u32()?, cursor.u32()?);
        if !is_pet(id) {
            return None;
        }
        souls.push((id, amount));
    }
    (level_count + soul_count > 0).then_some(GameEvent::PetCollection { levels, souls })
}

fn u64_at(body: &[u8], offset: usize) -> Option<u64> {
    let bytes = body.get(offset..offset + U64_LEN)?;
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

fn parse_daily_counter(body: &[u8]) -> Option<u64> {
    (body.len() == DAILY_COUNTER_LEN).then(|| u64_at(body, 0))?
}

fn kinah_amount_after_template(body: &[u8], template_offset: usize) -> Option<GameEvent> {
    u64_at(body, template_offset + KINAH_ITEM_TEMPLATE.len()).map(GameEvent::KinahBalance)
}

fn parse_inventory_kinah(body: &[u8]) -> Option<GameEvent> {
    let template_offset = body
        .windows(KINAH_ITEM_TEMPLATE.len())
        .position(|window| window == KINAH_ITEM_TEMPLATE)?;
    kinah_amount_after_template(body, template_offset)
}

fn parse_inventory_item_kinah(body: &[u8]) -> Option<GameEvent> {
    let template = body.get(
        INVENTORY_ITEM_TEMPLATE_OFFSET..INVENTORY_ITEM_TEMPLATE_OFFSET + KINAH_ITEM_TEMPLATE.len(),
    )?;
    (template == KINAH_ITEM_TEMPLATE)
        .then(|| kinah_amount_after_template(body, INVENTORY_ITEM_TEMPLATE_OFFSET))?
}

fn parse_abyss_points(body: &[u8]) -> Option<GameEvent> {
    if body.len() != ABYSS_POINTS_LEN {
        return None;
    }
    let current = u64_at(body, 0)?;
    let limit = u64_at(body, U64_LEN)?;
    (limit > 0).then_some(GameEvent::AbyssPoints { current, limit })
}

fn parse_pet_souls(body: &[u8]) -> Option<GameEvent> {
    let count = usize::from(*body.first()?);
    if count == 0 || body.len() != 1 + PET_SOULS_RECORD_LEN * count {
        return None;
    }
    let picked: Vec<(u32, u32)> = body[1..]
        .chunks_exact(PET_SOULS_RECORD_LEN)
        .map(|record| {
            let mut cursor = Cursor::new(record);
            (
                cursor.u32().expect("record has an id"),
                cursor.u32().expect("record has an amount"),
            )
        })
        .collect();
    picked
        .iter()
        .all(|&(id, amount)| is_pet(id) && PET_SOUL_AMOUNTS.contains(&amount))
        .then_some(GameEvent::PetSouls(picked))
}

fn parse_death(body: &[u8]) -> Option<GameEvent> {
    let mut cursor = Cursor::new(body);
    let id = read_entity(&mut cursor)?;
    let kind = *body.get(cursor.pos + DEATH_KIND_GAP)?;
    KILLED_DEATH_KINDS
        .contains(&kind)
        .then_some(GameEvent::Death { id })
}

fn fold_spawn_id(raw_id: u32) -> u32 {
    if raw_id > LARGE_SPAWN_ID_THRESHOLD {
        (raw_id & FOLDED_SPAWN_ID_MASK) | FOLDED_SPAWN_ID_BIT
    } else {
        raw_id
    }
}

fn parse_spawn(body: &[u8]) -> Option<GameEvent> {
    let mut cursor = Cursor::new(body);
    let id = fold_spawn_id(cursor.varint().filter(|&raw_id| raw_id != 0)?);
    let mask_offset = cursor.pos;
    let mask_low = u16_le(body, mask_offset)?;
    let mask = u32_le(body, mask_offset).unwrap_or(u32::from(mask_low));
    let kind = body[mask_offset];
    let (owner_name, after_owner_name) = find_owner_name(body, mask_offset);

    if kind == SPAWN_KIND_WITH_PARENT
        && mask & PARENT_BLOCK_FLAG != 0
        && let Some(owner) = find_parent_key(body, after_owner_name, id)
    {
        return Some(GameEvent::Summon {
            id,
            owner: Some(owner),
            owner_name: None,
        });
    }
    if SUMMON_KINDS.contains(&kind) && owner_name.is_some() {
        return Some(GameEvent::Summon {
            id,
            owner: None,
            owner_name,
        });
    }
    if SPIRIT_SUMMON_KINDS.contains(&kind) {
        let owner = find_spirit_owner(body, mask_offset).filter(|&owner| owner != id);
        return Some(GameEvent::Summon {
            id,
            owner,
            owner_name: None,
        });
    }
    None
}

fn find_owner_name(body: &[u8], mask_offset: usize) -> (Option<String>, usize) {
    for field_offset in OWNER_NAME_FIELD_OFFSETS {
        let flags_at = mask_offset + field_offset;
        if flags_at + 2 >= body.len() || body[flags_at] & NAME_PRESENT_FLAG == 0 {
            continue;
        }
        let len = usize::from(body[flags_at + 1]);
        let name_start = flags_at + 2;
        let name_end = name_start + len;
        if !NAME_FIELD_LENS.contains(&len) || name_end > body.len() {
            continue;
        }
        if let Some(name) = valid_name(&body[name_start..name_end]) {
            return (Some(name), name_end);
        }
    }
    (None, mask_offset + OWNER_SEARCH_DEFAULT_OFFSET)
}

fn find_parent_key(body: &[u8], from: usize, summon_id: u32) -> Option<u32> {
    (from.max(1)..)
        .take_while(|&pos| pos + PARENT_NAME_OFFSET <= body.len())
        .find_map(|pos| parent_block_at(body, pos, summon_id))
}

fn parent_block_at(body: &[u8], pos: usize, summon_id: u32) -> Option<u32> {
    if body[pos - 1] != PARENT_BLOCK_TAG {
        return None;
    }
    let parent = u32_le(body, pos).filter(|&parent| is_entity(parent) && parent != summon_id)?;
    if u16_le(body, pos + PARENT_ZERO_OFFSET)? != 0 {
        return None;
    }
    if !PARENT_MARKERS.contains(&u16_le(body, pos + PARENT_MARKER_OFFSET)?) {
        return None;
    }
    let name_len = usize::from(body[pos + PARENT_NAME_LEN_OFFSET]);
    let name_start = pos + PARENT_NAME_OFFSET;
    let name = body
        .get(name_start..name_start + name_len)
        .filter(|_| PARENT_NAME_LENS.contains(&name_len))?;
    is_text(name).then_some(parent)
}

fn find_spirit_owner(body: &[u8], from: usize) -> Option<u32> {
    let search_end = body
        .len()
        .min(from + SPIRIT_ANCHOR_SEARCH_LEN + SPIRIT_ANCHOR.len());
    let window = body.get(from..search_end)?;
    let anchor_pos = window
        .windows(SPIRIT_ANCHOR.len())
        .position(|candidate| candidate == SPIRIT_ANCHOR)?;
    let (owner, _) = read_varint(&body[from + anchor_pos + SPIRIT_ANCHOR.len()..])?;
    is_entity(owner).then_some(owner)
}

fn is_text(field: &[u8]) -> bool {
    std::str::from_utf8(field).is_ok_and(|text| !text.chars().any(char::is_control))
}

fn scan_identities(data: &[u8], out: &mut Vec<GameEvent>) {
    for start in 0..data.len().saturating_sub(MIN_EMBEDDED_IDENTITY_LEN) {
        let opcode = u16::from_be_bytes([data[start], data[start + 1]]);
        if !matches!(
            opcode,
            opcodes::SELF_INFO | opcodes::PLAYER_INFO | opcodes::PLAYER_INFO_OLD
        ) {
            continue;
        }
        let record = &data[start + opcodes::OPCODE_LEN..];
        if let Some(event) = parse_identity(record, opcode == opcodes::SELF_INFO)
            && !out.contains(&event)
        {
            out.push(event);
        }
    }
}

fn scan_embedded_bundles(body: &[u8], out: &mut Vec<GameEvent>) {
    let mut pos = 1;
    while pos + MIN_EMBEDDED_BUNDLE_TAIL < body.len() {
        if body[pos..].starts_with(&BUNDLE_MARKER)
            && let Some(end) = (1..=MAX_EMBEDDED_LENGTH_VARINT_BYTES)
                .rev()
                .find_map(|varint_len| decode_embedded_bundle(body, pos, varint_len, out))
        {
            pos = end;
            continue;
        }
        pos += 1;
    }
}

fn decode_embedded_bundle(
    body: &[u8],
    marker_pos: usize,
    varint_len: usize,
    out: &mut Vec<GameEvent>,
) -> Option<usize> {
    let frame_start = marker_pos.checked_sub(varint_len)?;
    let (declared_len, actual_varint_len) = read_varint(&body[frame_start..])?;
    if actual_varint_len != varint_len || declared_len < MIN_EMBEDDED_BUNDLE_DECLARED_LEN {
        return None;
    }
    let frame_end = frame_start + frame_size(declared_len as usize, varint_len);
    if frame_end > body.len() {
        return None;
    }
    let decompressed_len = u32_le(body, marker_pos + BUNDLE_MARKER.len())
        .filter(|&len| len > 0 && len as usize <= MAX_BUNDLE)?;
    let mut decompressed = vec![0u8; decompressed_len as usize];
    let compressed = &body[marker_pos + BUNDLE_HEADER_LEN..frame_end];
    let written = lz4_flex::block::decompress_into(compressed, &mut decompressed).ok()?;
    scan_identities(&decompressed[..written], out);
    Some(frame_end)
}

fn normalize_skill(skill: u32) -> u32 {
    if LEGACY_SKILL_IDS.contains(&skill) {
        skill * LEGACY_SKILL_SCALE + LEGACY_SKILL_SUFFIX
    } else {
        skill
    }
}

fn has_multi_hit(record_flags: u32) -> bool {
    record_flags & MULTI_HIT_FLAG != 0
}

fn damage_record(body: &[u8], start: usize) -> Option<(Hit, usize, bool)> {
    let mut cursor = Cursor::at(body, start);
    let target = read_entity(&mut cursor)?;
    let record_flags = cursor.varint()?;
    let layout = (record_flags & LAYOUT_MASK) as usize;
    if !DAMAGE_LAYOUTS.contains(&layout) {
        return None;
    }
    cursor.varint()?;
    let actor = read_entity(&mut cursor)?;
    let skill = normalize_skill(cursor.u32()?);
    if !is_valid_skill(skill) {
        return None;
    }
    cursor.u8()?;
    let damage_type = cursor.varint()?;
    let back = layout >= MIN_BACK_ATTACK_LAYOUT
        && body.get(cursor.pos + BACK_ATTACK_FLAG_OFFSET) == Some(&BACK_ATTACK_FLAG);
    let (values_end, amount) = choose_damage_value(body, cursor.pos, layout, damage_type)?;
    let (end, hits, clean) = finish_record(body, values_end, layout, record_flags, amount);
    let hit = Hit {
        actor,
        target,
        skill,
        amount,
        crit: damage_type == DAMAGE_TYPE_CRIT,
        back,
        dot: false,
        hits,
    };
    Some((hit, end, clean))
}

fn choose_damage_value(
    body: &[u8],
    block_start: usize,
    layout: usize,
    damage_type: u32,
) -> Option<(usize, u32)> {
    let mut fallback = None;
    for &block_offset in VALUE_BLOCK_OFFSETS_BY_LAYOUT[layout] {
        let Some((end, scalar, damage)) =
            read_damage_values(body, block_start + block_offset, layout, damage_type)
        else {
            continue;
        };
        if !DAMAGE_AMOUNTS.contains(&damage) {
            continue;
        }
        if scalar == 0 || PLAUSIBLE_SCALARS.contains(&scalar) {
            return Some((end, damage));
        }
        fallback.get_or_insert((end, damage));
    }
    fallback
}

fn read_damage_values(
    body: &[u8],
    offset: usize,
    layout: usize,
    damage_type: u32,
) -> Option<(usize, u32, u32)> {
    if offset >= body.len() {
        return None;
    }
    let mut cursor = Cursor::at(body, offset);
    let mut scalar = cursor.varint()?;
    let mut after_scalar = cursor.pos;
    let mut damage = cursor.varint()?;
    if scalar == 0 {
        scalar = damage;
        after_scalar = cursor.pos;
        damage = cursor.varint()?;
    }
    if layout == COMPACT_CRIT_LAYOUT
        && damage_type == DAMAGE_TYPE_CRIT
        && COMPACT_CRIT_DAMAGE.contains(&scalar)
        && damage <= *HIT_COUNTS.end()
    {
        return Some((after_scalar, 0, scalar));
    }
    Some((cursor.pos, scalar, damage))
}

fn finish_record(
    body: &[u8],
    values_end: usize,
    layout: usize,
    record_flags: u32,
    amount: u32,
) -> (usize, u32, bool) {
    let mut end = values_end;
    let mut hits = 1;
    let tail = parse_tail(body, end, layout, record_flags, amount).or_else(|| {
        if layout != COMPACT_CRIT_LAYOUT {
            parse_tail(body, end, COMPACT_CRIT_LAYOUT, record_flags, amount)
        } else {
            None
        }
    });
    let clean = tail.is_some();
    if let Some((tail_end, extra_hits)) = tail {
        end = tail_end;
        hits += extra_hits;
    } else if has_multi_hit(record_flags)
        && let Some((count, _)) = read_varint(&body[end..])
        && HIT_COUNTS.contains(&count)
    {
        hits += count;
    }
    if clean && body[end..].starts_with(&DAMAGE_RECORD_TRAILER) {
        end += DAMAGE_RECORD_TRAILER.len();
        if let Some((_, len)) = read_varint(&body[end..]) {
            end += len;
        }
    }
    (end, hits, clean)
}

fn parse_tail(
    body: &[u8],
    offset: usize,
    layout: usize,
    record_flags: u32,
    damage: u32,
) -> Option<(usize, u32)> {
    let mut cursor = Cursor::at(body, offset);
    if layout == LAYOUT_WITH_HIT_FIELD {
        let hit_field = cursor.varint()?;
        if !HIT_COUNTS.contains(&hit_field) {
            return None;
        }
    }
    let mut extra_hits = 0;
    if has_multi_hit(record_flags) {
        let count = cursor.varint()?;
        if !HIT_COUNTS.contains(&count) {
            return None;
        }
        let mut extra_damage = 0u64;
        for _ in 0..count {
            extra_damage += u64::from(cursor.varint()?);
        }
        if extra_damage >= u64::from(damage) {
            return None;
        }
        extra_hits = count;
    }
    is_record_boundary(cursor.rest()).then_some((cursor.pos, extra_hits))
}

fn is_record_boundary(rest: &[u8]) -> bool {
    rest.is_empty()
        || (rest.len() >= 2 && rest[1] == 0x00 && NEXT_RECORD_TAGS.contains(&rest[0]))
        || rest.starts_with(&opcodes::DAMAGE.to_be_bytes())
}

fn parse_dot(body: &[u8]) -> Option<Hit> {
    let mut cursor = Cursor::new(body);
    let target = read_entity(&mut cursor)?;
    let effect = cursor.u8()?;
    if !DOT_DAMAGE_EFFECTS.contains(&effect) {
        return None;
    }
    let actor = read_entity(&mut cursor)?;
    cursor.varint()?;
    let skill = cursor.u32()? / DOT_SKILL_SCALE;
    if !is_valid_skill(skill) {
        return None;
    }
    let amount = cursor
        .varint()
        .filter(|amount| DAMAGE_AMOUNTS.contains(amount))?;
    Some(Hit {
        actor,
        target,
        skill,
        amount,
        crit: false,
        back: false,
        dot: true,
        hits: 1,
    })
}

fn parse_identity(record: &[u8], is_self: bool) -> Option<GameEvent> {
    let mut cursor = Cursor::new(record);
    let id = read_entity(&mut cursor)?;
    cursor.take(IDENTITY_SKIPPED_LEN)?;
    if cursor.u8()? & NAME_PRESENT_FLAG == 0 {
        return None;
    }
    let field = read_name_field(&mut cursor)?;
    if field[0] == HIDDEN_NAME_PREFIX {
        return None;
    }
    let name = valid_name(field)?;
    Some(GameEvent::Identity { id, name, is_self })
}

fn parse_effect_source(body: &[u8]) -> Option<GameEvent> {
    let mut cursor = Cursor::new(body);
    cursor.varint()?;
    if !is_class_skill(cursor.u32()?) {
        return None;
    }
    let id = read_entity(&mut cursor)?;
    cursor.take(EFFECT_SOURCE_SKIPPED_LEN)?;
    let name = valid_name(read_name_field(&mut cursor)?)?;
    Some(GameEvent::Caster { id, name })
}

fn party_members(body: &[u8]) -> Vec<PartyMember> {
    let mut members: Vec<PartyMember> = Vec::new();
    for len_pos in PARTY_CHAR_ID_BEFORE_NAME..body.len() {
        let len = usize::from(body[len_pos]);
        let name_end = len_pos + 1 + len;
        let marker_start = name_end + PARTY_MEMBER_MARKER_GAP;
        let marker = body.get(marker_start..marker_start + PARTY_MEMBER_MARKER.len());
        if !NAME_FIELD_LENS.contains(&len) || marker != Some(&PARTY_MEMBER_MARKER[..]) {
            continue;
        }
        let (Some(name), Some(char_id)) = (
            valid_name(&body[len_pos + 1..name_end]),
            u32_le(body, len_pos - PARTY_CHAR_ID_BEFORE_NAME),
        ) else {
            continue;
        };
        if !members.iter().any(|member| member.char_id == char_id) {
            members.push(PartyMember { char_id, name });
        }
    }
    members
}

fn valid_name(field: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(field).ok()?;
    let is_name = NAME_CHARS.contains(&text.chars().count())
        && text.chars().all(char::is_alphanumeric)
        && text.chars().any(char::is_alphabetic);
    is_name.then(|| text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: u32 = 12874;
    const MOB: u32 = 55697;
    const UNRELATED_OPCODE: u16 = 0x2F37;
    const SUMMON_ID_VARINT: [u8; 2] = [0x88, 0x0f];
    const SUMMON_ID: u32 = 1928;

    fn bytes(hex: &str) -> Vec<u8> {
        hex.split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).unwrap())
            .collect()
    }

    fn events(opcode: u16, body: &[u8]) -> Vec<GameEvent> {
        let mut out = Vec::new();
        parse(opcode, body, &mut out);
        out
    }

    fn hits(opcode: u16, body: &str) -> Vec<Hit> {
        events(opcode, &bytes(body))
            .into_iter()
            .filter_map(|event| match event {
                GameEvent::Hit(hit) => Some(hit),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn multi_hit_crit_layout6() {
        let got = hits(
            opcodes::DAMAGE,
            "91 b3 03 36 00 ca 64 c4 d3 c6 00 bc 03 80 00 01 97 3f aa 4d 03 00 00 00 b6 58 f1 11 03 e4 01 e4 01 e4 01 03 00",
        );
        assert_eq!(
            got,
            vec![Hit {
                actor: ME,
                target: MOB,
                skill: 13030340,
                amount: 2289,
                crit: true,
                back: true,
                dot: false,
                hits: 4
            }]
        );
    }

    #[test]
    fn single_crit_layout6() {
        let got = hits(
            opcodes::DAMAGE,
            "91 b3 03 16 00 ca 64 28 6f c7 00 8f 03 80 00 01 b5 6b e7 4d 02 00 00 00 b6 58 f4 36 02 00",
        );
        assert_eq!(got.len(), 1);
        assert_eq!(
            (got[0].skill, got[0].amount, got[0].crit, got[0].hits),
            (13070120, 7028, true, 1)
        );
    }

    #[test]
    fn multi_hit_with_chained_marker() {
        let got = hits(
            opcodes::DAMAGE,
            "91 b3 03 36 04 ca 64 fc b4 cb 00 99 02 84 00 02 7b b2 92 4f 01 00 00 00 b6 58 c4 2d 04 c6 04 c6 04 c6 04 c6 04 01 00 c3 01",
        );
        assert_eq!(got.len(), 1);
        assert_eq!(
            (got[0].skill, got[0].amount, got[0].crit, got[0].hits),
            (13350140, 5828, false, 5)
        );
    }

    #[test]
    fn mob_hitting_player_layout4() {
        let got = hits(
            opcodes::DAMAGE,
            "ca 64 04 00 91 b3 03 92 87 18 00 02 02 13 f5 94 09 01 00 00 00 90 4e 84 11 01 00",
        );
        assert_eq!(got.len(), 1);
        assert_eq!(
            (got[0].actor, got[0].target, got[0].skill, got[0].amount),
            (MOB, ME, 1607570, 2180)
        );
        assert!(!is_class_skill(got[0].skill));
    }

    #[test]
    fn layout0_announcement_has_no_damage() {
        assert!(
            hits(
                opcodes::DAMAGE,
                "91 b3 03 00 00 ca 64 04 7c ca 00 a0 02 9f 71 18 4f 01 00 00 00 b6 58 01 00"
            )
            .is_empty()
        );
    }

    #[test]
    fn dot_damage() {
        let got = hits(
            opcodes::DOT,
            "91 b3 03 0a ca 64 06 08 54 d6 51 cb 03 d7 80 d1 00",
        );
        assert_eq!(
            got,
            vec![Hit {
                actor: ME,
                target: MOB,
                skill: 13730007,
                amount: 459,
                crit: false,
                back: false,
                dot: true,
                hits: 1
            }]
        );
    }

    #[test]
    fn effect_source_names_the_caster() {
        let mut body = bytes("af 9b 01 fc b4 cb 00 cb 66 09 09 08");
        body.extend_from_slice(b"Testname");
        body.extend(bytes("00 00 00 00 00 00 00 00 00 01 00"));
        assert_eq!(
            events(opcodes::EFFECT_SOURCE, &body),
            vec![GameEvent::Caster {
                id: 13131,
                name: "Testname".into()
            }]
        );
        let mut npc = bytes("af 9b 01 18 b9 12 00 cb 66 09 09 08");
        npc.extend_from_slice(b"Testname");
        assert!(events(opcodes::EFFECT_SOURCE, &npc).is_empty());
    }

    #[test]
    fn self_state_tells_the_local_player_id() {
        let body = bytes("cb 66 01 a9 01 f6 04 00 00 00 00 00 00 00 00 00 00 00");
        assert_eq!(
            events(opcodes::SELF_STATE, &body),
            vec![GameEvent::SelfId { id: 13131 }]
        );
    }

    #[test]
    fn npc_spawn_carries_monster_type_and_death_kind() {
        let spawn =
            bytes("dc b8 01 0c 20 00 8c ac 27 00 40 02 0c 89 07 46 9c 69 98 c3 00 1f 1f 47");
        assert!(
            events(opcodes::SPAWN, &spawn).contains(&GameEvent::NpcSpawn {
                id: 23644,
                template: 2600076
            })
        );
        assert_eq!(
            events(opcodes::DEATH, &bytes("dc b8 01 00 03")),
            vec![GameEvent::Death { id: 23644 }]
        );
        assert!(
            events(opcodes::DEATH, &bytes("dc b8 01 00 01")).is_empty(),
            "corpse removed, not a kill"
        );
    }

    #[test]
    fn pet_collection_and_picked_up_souls() {
        let body = bytes(
            "28 04 00 00 02 bd 04 00 00 bd 04 00 00 01 00 00 00 be 04 00 00 be 04 00 00 \
             02 00 00 00 02 bd 04 00 00 04 00 00 00 be 04 00 00 20 00 00 00 06 01 54 00 00 00",
        );
        assert_eq!(
            events(opcodes::PET_COLLECTION, &body),
            vec![GameEvent::PetCollection {
                levels: vec![(1213, 1), (1214, 2)],
                souls: vec![(1213, 4), (1214, 32)]
            }]
        );
        assert_eq!(
            events(opcodes::PET_SOULS, &bytes("01 be 04 00 00 01 00 00 00")),
            vec![GameEvent::PetSouls(vec![(1214, 1)])]
        );
        assert_eq!(
            events(
                opcodes::PET_SOULS,
                &bytes("02 ed 03 00 00 01 00 00 00 26 04 00 00 02 00 00 00")
            ),
            vec![GameEvent::PetSouls(vec![(1005, 1), (1062, 2)])]
        );
        assert!(events(opcodes::PET_SOULS, &bytes("0c 06 73 ae 67 3e 05 fe")).is_empty());
    }

    #[test]
    fn kinah_balance_from_inventory() {
        let item = "01 01 00 a1 56 15 00 00 13 00 00 3f 33 70 37 fc 49 db 00 00 00 00 00 01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 0b 00 00";
        assert_eq!(
            events(opcodes::INVENTORY_ITEM, &bytes(item)),
            vec![GameEvent::KinahBalance(14_371_324)]
        );
        let other_item = "01 01 00 39 12 14 00 00 0c 00 00 5d 2b 65 24 43 00 00 00 00 00 00 00 01 52 00 00 00 00 00 00 00 00 00 00 00 00 00 29 00 00";
        assert!(events(opcodes::INVENTORY_ITEM, &bytes(other_item)).is_empty());
        let inventory = "00 00 00 00 00 00 00 00 00 00 00 00 a1 56 15 00 00 13 00 00 3f 33 70 37 5d 48 db 00 00 00 00 00 01 00 00 00";
        assert_eq!(
            events(opcodes::INVENTORY, &bytes(inventory)),
            vec![GameEvent::KinahBalance(14_370_909)]
        );
    }

    #[test]
    fn daily_limits_from_login_and_updates() {
        assert_eq!(
            events(
                opcodes::DAILY_KINAH_STATE,
                &bytes("0d e7 00 00 00 00 00 00 9d d6 dc 83 a1 01 00 00")
            ),
            vec![GameEvent::DailyKinah(59_149)]
        );
        assert_eq!(
            events(
                opcodes::DAILY_KINAH,
                &bytes("a2 f5 00 00 00 00 00 00 17 ac dd 83 a1 01 00 00")
            ),
            vec![GameEvent::DailyKinah(62_882)]
        );
        assert_eq!(
            events(
                opcodes::DAILY_GATHERING,
                &bytes("bb 02 00 00 00 00 00 00 cb c0 dd 83 a1 01 00 00")
            ),
            vec![GameEvent::DailyGathering(699)]
        );
        assert_eq!(
            events(
                opcodes::ABYSS_POINTS,
                &bytes("f2 9a 00 00 00 00 00 00 a0 25 26 00 00 00 00 00")
            ),
            vec![GameEvent::AbyssPoints {
                current: 39_666,
                limit: 2_500_000
            }]
        );
        assert!(events(opcodes::DAILY_KINAH, &bytes("0d e7 00 00")).is_empty());
        assert!(
            events(
                opcodes::ABYSS_POINTS,
                &bytes("f2 9a 00 00 00 00 00 00 00 00 00 00 00 00 00 00")
            )
            .is_empty()
        );
    }

    #[test]
    fn party_roster_join_and_leave() {
        let roster = bytes(
            "5c 5c 19 00 04 31 31 30 4b 05 d6 27 09 00 00 03 1a 06 04 00 00 00 00 09 0f 01 \
             00 05 0c 01 1a 06 04 00 00 00 00 09 06 41 6c 70 68 61 61 07 00 00 00 2d 00 00 \
             00 6c 09 00 00 00 09 00 09 04 3a a8 01 00 00 00 00 00 00 01 01 0c 02 2c 8e 03 \
             00 00 00 16 05 09 42 72 61 76 6f 6f 6f 6f 6f 16 00 00 00 2d 00 00 00 e9 08 00 \
             00 1b 05 1b 05 04 66 ae 01 00 00 00 00 00 00 01 01 0c 03 3b aa 02 00 00 00 09 \
             09 09 43 68 61 72 6c 69 65 31 32 14 00 00 00 2d 00 00 00 bc 08 00 00 09 09 09 \
             09 04 86 8f 01 00 00 00 00 00 00 01 01 00 04 00 00 00 00 00 00 00 00 00 00 00 \
             00 00 00 00 00 00 00 00 00 00 01 00 00 00 00 00 00 00 00 00 00 00 00 05 00 00 \
             00 00 00 00 00",
        );
        let member = |char_id, name: &str| PartyMember {
            char_id,
            name: name.into(),
        };
        assert_eq!(
            events(opcodes::PARTY_ROSTER, &roster),
            vec![GameEvent::Party {
                members: vec![
                    member(263706, "Alphaa"),
                    member(233004, "Bravooooo"),
                    member(174651, "Charlie12")
                ]
            }]
        );
        let join = bytes(
            "0c 04 b6 53 03 00 00 00 18 05 09 44 65 6c 74 61 61 61 61 31 05 00 00 00 2d 00 \
             00 00 66 08 00 00 01 18 05 18 05 01 fa 84 01 00 00 00 00 00 00 01 01",
        );
        assert_eq!(
            events(opcodes::PARTY_JOIN, &join),
            vec![GameEvent::PartyJoin(member(218038, "Deltaaaa1"))]
        );
        assert_eq!(
            events(
                opcodes::PARTY_MEMBER_LEFT,
                &bytes("2c 8e 03 00 00 00 16 05 01")
            ),
            vec![GameEvent::PartyMemberLeft { char_id: 233004 }]
        );
        assert_eq!(
            events(opcodes::PARTY_LEFT, &bytes("00 00")),
            vec![GameEvent::PartyLeft]
        );
    }

    #[test]
    fn map_load_carries_map_id() {
        let dungeon = bytes("02 00 00 00 d6 27 09 00 45 54 bc 00 00 00 00 00");
        let world = bytes("01 00 00 00 56 04 00 00 a3 1d c3 00 00 00 00 00");
        assert_eq!(
            events(opcodes::MAP_LOAD, &dungeon),
            vec![GameEvent::MapLoad { map: 600022 }]
        );
        assert_eq!(
            events(opcodes::MAP_LOAD, &world),
            vec![GameEvent::MapLoad { map: 1110 }]
        );
    }

    #[test]
    fn dot_non_damage_effect_ignored() {
        assert!(hits(opcodes::DOT, "91 b3 03 00 91 b3 03 01 0a e1 f5 05").is_empty());
    }

    #[test]
    fn layout0_announcement_is_a_cast_without_damage() {
        let body =
            bytes("91 b3 03 00 00 ca 64 04 7c ca 00 a0 02 9f 71 18 4f 01 00 00 00 b6 58 01 00");
        assert_eq!(
            events(opcodes::DAMAGE, &body),
            vec![GameEvent::Cast {
                actor: ME,
                target: MOB,
                skill: 13270020
            }]
        );
    }

    #[test]
    fn cast_packet() {
        let body = bytes("ca 64 00 08 54 d6 00 01 02 91 b3 03 00 00 00 00");
        assert_eq!(
            events(opcodes::CAST, &body),
            vec![GameEvent::Cast {
                actor: ME,
                target: MOB,
                skill: 0x00d65408
            }]
        );
    }

    fn spawn(id: &[u8], mask: [u8; 4], rest: &[u8]) -> Vec<u8> {
        let mut body = id.to_vec();
        body.extend_from_slice(&mask);
        body.extend_from_slice(rest);
        body
    }

    #[test]
    fn spawn_summon_with_parent_block() {
        let mut rest = vec![0u8; 6];
        rest.push(PARENT_BLOCK_TAG);
        rest.extend_from_slice(&ME.to_le_bytes());
        rest.extend_from_slice(&7u32.to_le_bytes());
        rest.extend_from_slice(&[0, 0]);
        rest.extend_from_slice(&2001u16.to_le_bytes());
        rest.push(5);
        rest.extend_from_slice(b"Wolfs");
        let body = spawn(&SUMMON_ID_VARINT, [0x5F, 0x10, 0, 0], &rest);
        assert_eq!(
            events(opcodes::SPAWN, &body),
            vec![GameEvent::Summon {
                id: SUMMON_ID,
                owner: Some(ME),
                owner_name: None
            }]
        );
    }

    #[test]
    fn spawn_summon_with_owner_name() {
        let mut rest = vec![NAME_PRESENT_FLAG, 6];
        rest.extend_from_slice(b"Faelis");
        rest.extend_from_slice(&[0; 8]);
        let body = spawn(&SUMMON_ID_VARINT, [0x1C, 0, 0, 0], &rest);
        assert_eq!(
            events(opcodes::SPAWN, &body),
            vec![GameEvent::Summon {
                id: SUMMON_ID,
                owner: None,
                owner_name: Some("Faelis".into())
            }]
        );
    }

    #[test]
    fn spawn_spirit_owner_after_anchor() {
        let mut rest = vec![0u8; 10];
        rest.extend_from_slice(&SPIRIT_ANCHOR);
        rest.extend_from_slice(&[0xca, 0x64]);
        let body = spawn(&SUMMON_ID_VARINT, [0x1F, 0, 0, 0], &rest);
        assert_eq!(
            events(opcodes::SPAWN, &body),
            vec![GameEvent::Summon {
                id: SUMMON_ID,
                owner: Some(ME),
                owner_name: None
            }]
        );
    }

    #[test]
    fn spawn_large_ids_are_folded() {
        let body = spawn(&[0x80, 0x89, 0x7a], [0x1F, 0, 0, 0], &[0; 4]);
        let expected = (2_000_000 & 0x3FFF) | 0x4000;
        assert_eq!(
            events(opcodes::SPAWN, &body),
            vec![GameEvent::Summon {
                id: expected,
                owner: None,
                owner_name: None
            }]
        );
    }

    #[test]
    fn spawn_plain_npc_is_ignored() {
        assert!(
            events(
                opcodes::SPAWN,
                &spawn(&SUMMON_ID_VARINT, [0x0C, 0, 0, 0], &[0; 20])
            )
            .is_empty()
        );
    }

    fn identity_record(opcode: u16, name: &[u8]) -> Vec<u8> {
        let mut record = opcode.to_be_bytes().to_vec();
        record.extend_from_slice(&[0xca, 0x64, 0, 0, 0, 0, NAME_PRESENT_FLAG, name.len() as u8]);
        record.extend_from_slice(name);
        record.extend_from_slice(&[0; 6]);
        record
    }

    #[test]
    fn identity_embedded_in_other_packet() {
        let mut body = vec![0x11; 9];
        body.extend(identity_record(opcodes::SELF_INFO, b"Faelis"));
        assert_eq!(
            events(UNRELATED_OPCODE, &body),
            vec![GameEvent::Identity {
                id: ME,
                name: "Faelis".into(),
                is_self: true
            }]
        );
    }

    #[test]
    fn identity_inside_embedded_lz4_bundle() {
        let inner = identity_record(opcodes::PLAYER_INFO, b"Kyra");
        let mut payload = BUNDLE_MARKER.to_vec();
        payload.extend_from_slice(&(inner.len() as u32).to_le_bytes());
        payload.extend(lz4_flex::block::compress(&inner));
        let mut body = vec![0x22; 5];
        body.push((payload.len() + 4) as u8);
        body.extend(payload);
        body.extend_from_slice(&[0x22; 4]);
        assert_eq!(
            events(UNRELATED_OPCODE, &body),
            vec![GameEvent::Identity {
                id: ME,
                name: "Kyra".into(),
                is_self: false
            }]
        );
    }

    #[test]
    fn self_identity() {
        let mut body = vec![0xca, 0x64, 0, 0, 0, 0, NAME_PRESENT_FLAG, 6];
        body.extend_from_slice(b"Faelis");
        body.extend_from_slice(&[0x18, 0, 0, 0]);
        assert_eq!(
            events(opcodes::SELF_INFO, &body),
            vec![GameEvent::Identity {
                id: ME,
                name: "Faelis".into(),
                is_self: true
            }]
        );
    }

    #[test]
    fn identity_rejects_non_name_bytes() {
        let mut body = vec![0xca, 0x64, 0, 0, 0, 0, NAME_PRESENT_FLAG, 4];
        body.extend_from_slice(b"a b!");
        assert!(events(opcodes::PLAYER_INFO, &body).is_empty());
    }

    #[test]
    fn base_skill_collapses_levels() {
        assert_eq!(base_skill(13030340), 13030000);
        assert_eq!(base_skill(1607570), 1607570);
    }
}
