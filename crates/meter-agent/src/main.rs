use meter_agent::sources;

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::Result;
use clap::{Parser, Subcommand};
use meter_capture::locator::GameLocator;
use meter_capture::pcap_io;
use meter_core::combat::Meter;
use meter_core::opcodes;
use meter_core::parser::{self, GameEvent, Hit};
use meter_core::pipeline::GamePacket;
use meter_core::wire::hex;
use meter_proto::{ActorRow, EncounterView};

const DEFAULT_DUMP_BYTES: usize = 64;
const HEX_PREFIX: &str = "0x";
const HEX_RADIX: u32 = 16;
const UNKNOWN_OPCODE_NAME: &str = "?";
const AS_FAST_AS_POSSIBLE: f64 = 0.0;
const MS_PER_SECOND: i64 = 1_000;
const PERCENT: f64 = 100.0;

#[derive(Parser)]
#[command(
    name = "a2m",
    version,
    about = "Passive AION 2 DPS meter (aion2-metric)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Devices,
    Conns,
    Live {
        #[arg(long)]
        record: Option<PathBuf>,
        #[arg(long)]
        seconds: Option<u64>,
        #[arg(long, value_parser = parse_hex_u16)]
        opcode: Option<u16>,
        #[arg(long)]
        quiet: bool,
    },
    Dump {
        file: PathBuf,
        #[arg(long, value_parser = parse_hex_u16)]
        opcode: Option<u16>,
        #[arg(long)]
        server_port: Option<u16>,
        #[arg(long, default_value_t = DEFAULT_DUMP_BYTES)]
        max_bytes: usize,
    },
    Stats {
        file: PathBuf,
    },
    Events {
        file: PathBuf,
    },
    Fights {
        file: PathBuf,
    },
    Kills {
        file: PathBuf,
    },
}

fn parse_hex_u16(text: &str) -> Result<u16, String> {
    u16::from_str_radix(text.trim_start_matches(HEX_PREFIX), HEX_RADIX)
        .map_err(|error| error.to_string())
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Devices => devices(),
        Cmd::Conns => conns(),
        Cmd::Live {
            record,
            seconds,
            opcode,
            quiet,
        } => live(record.as_deref(), seconds, opcode, quiet),
        Cmd::Dump {
            file,
            opcode,
            server_port,
            max_bytes,
        } => dump(&file, opcode, server_port, max_bytes),
        Cmd::Stats { file } => stats(&file),
        Cmd::Events { file } => events(&file),
        Cmd::Fights { file } => fights(&file),
        Cmd::Kills { file } => kills(&file),
    }
}

fn ms_to_secs(ms: i64) -> f64 {
    ms as f64 / MS_PER_SECOND as f64
}

fn opcode_name(opcode: u16) -> &'static str {
    opcodes::name(opcode).unwrap_or(UNKNOWN_OPCODE_NAME)
}

fn flag(enabled: bool, label: &'static str) -> &'static str {
    if enabled { label } else { "" }
}

fn devices() -> Result<()> {
    for device in pcap_io::list_devices()? {
        let addresses: Vec<String> = device
            .addresses
            .iter()
            .map(|address| address.addr.to_string())
            .collect();
        println!(
            "{}  {}  [{}]",
            device.name,
            device.desc.as_deref().unwrap_or(""),
            addresses.join(", ")
        );
    }
    Ok(())
}

fn conns() -> Result<()> {
    let mut locator = GameLocator::new();
    let pids = locator.pids();
    if pids.is_empty() {
        println!("AION 2 is not running");
        return Ok(());
    }
    println!("pids: {pids:?}");
    for conn in locator.connections()? {
        println!("pid {}  {} -> {}", conn.pid, conn.local, conn.remote);
    }
    Ok(())
}

fn print_packet(packet: &GamePacket, max_bytes: usize) {
    println!(
        "{:>16} {} -> {} {:04x} {:<11} {:>5}B {}",
        packet.ts_us,
        packet.flow.src,
        packet.flow.dst,
        packet.opcode,
        opcode_name(packet.opcode),
        packet.body.len(),
        hex(&packet.body, max_bytes)
    );
}

fn replay_all(file: &Path, on_packet: impl FnMut(&GamePacket)) -> Result<()> {
    sources::replay(
        file,
        AS_FAST_AS_POSSIBLE,
        &AtomicBool::new(false),
        on_packet,
    )
}

fn replay_through_meter(
    file: &Path,
    meter: &mut Meter,
    mut after_packet: impl FnMut(&mut Meter, i64, i64),
) -> Result<Option<i64>> {
    let mut events = Vec::new();
    let mut first_ts_ms: Option<i64> = None;
    replay_all(file, |packet| {
        let ts_ms = packet.ts_ms();
        let first = *first_ts_ms.get_or_insert(ts_ms);
        events.clear();
        parser::parse(packet.opcode, &packet.body, &mut events);
        meter.tick(ts_ms);
        for event in &events {
            meter.on_event(ts_ms, event);
        }
        after_packet(meter, ts_ms, first);
    })?;
    Ok(first_ts_ms)
}

fn dump(
    file: &Path,
    opcode: Option<u16>,
    server_port: Option<u16>,
    max_bytes: usize,
) -> Result<()> {
    replay_all(file, |packet| {
        if opcode.is_some_and(|wanted| wanted != packet.opcode)
            || server_port.is_some_and(|port| port != packet.flow.src.port())
        {
            return;
        }
        print_packet(packet, max_bytes);
    })
}

#[derive(Default)]
struct OpcodeTotals {
    count: u64,
    bytes: u64,
}

fn stats(file: &Path) -> Result<()> {
    let mut totals: HashMap<u16, OpcodeTotals> = HashMap::new();
    replay_all(file, |packet| {
        let entry = totals.entry(packet.opcode).or_default();
        entry.count += 1;
        entry.bytes += packet.body.len() as u64;
    })?;
    let mut rows: Vec<_> = totals.into_iter().collect();
    rows.sort_by_key(|(opcode, totals)| (Reverse(totals.count), *opcode));
    println!("opcode name          count    bytes");
    for (opcode, OpcodeTotals { count, bytes }) in rows {
        println!(
            "{opcode:04x}   {:<11} {count:>8} {bytes:>8}",
            opcode_name(opcode)
        );
    }
    Ok(())
}

fn kills(file: &Path) -> Result<()> {
    let mut meter = Meter::new();
    let mut kills_by_template: BTreeMap<u32, u32> = BTreeMap::new();
    replay_through_meter(file, &mut meter, |meter, ts_ms, first_ts_ms| {
        for template in meter.take_kills() {
            *kills_by_template.entry(template).or_default() += 1;
            println!("{:>7.1}s kill {template}", ms_to_secs(ts_ms - first_ts_ms));
        }
    })?;
    for (template, count) in kills_by_template {
        println!("{template}: {count}");
    }
    Ok(())
}

fn row_label(row: &ActorRow) -> String {
    match (&row.name, row.is_self) {
        (Some(_), true) => format!("{} (ty)", row.id),
        (Some(_), false) => format!("{} (nick)", row.id),
        (None, _) => format!("#{}", row.id),
    }
}

fn print_fight(fight: &EncounterView, first_ts_ms: i64) {
    println!(
        "@{:>4}s  {:>5.1}s  {:>9} dmg  {} players",
        (fight.start_ms - first_ts_ms) / MS_PER_SECOND,
        ms_to_secs(fight.duration_ms),
        fight.total_damage,
        fight.rows.len()
    );
    for row in &fight.rows {
        let summons = if row.summon_damage > 0 {
            format!("  pety {}", row.summon_damage)
        } else {
            String::new()
        };
        println!(
            "        {:<14} {:>9} dmg {:>7.0}/s {:>4.0}%{}",
            row_label(row),
            row.damage,
            row.dps,
            row.share * PERCENT,
            summons
        );
    }
}

fn fights(file: &Path) -> Result<()> {
    let mut meter = Meter::new();
    let mut last_fight: Option<EncounterView> = None;
    let first_ts_ms = replay_through_meter(file, &mut meter, |meter, _, first_ts_ms| {
        let current = meter.view();
        if let (Some(previous), Some(current)) = (&last_fight, &current)
            && current.start_ms != previous.start_ms
        {
            print_fight(previous, first_ts_ms);
        }
        if current.is_some() {
            last_fight = current;
        }
    })?;
    if let (Some(fight), Some(first_ts_ms)) = (&last_fight, first_ts_ms) {
        print_fight(fight, first_ts_ms);
    }
    println!("dropped DoT ticks: {}", meter.dropped_dots());
    Ok(())
}

fn events(file: &Path) -> Result<()> {
    let mut events = Vec::new();
    replay_all(file, |packet| {
        events.clear();
        parser::parse(packet.opcode, &packet.body, &mut events);
        for event in &events {
            println!("{:>16} {}", packet.ts_us, describe_event(event));
        }
    })
}

fn describe_hit(hit: &Hit) -> String {
    format!(
        "hit  {:>8} -> {:<8} skill {:>9} {:>8}{}{}{} x{}",
        hit.actor,
        hit.target,
        hit.skill,
        hit.amount,
        flag(hit.crit, " crit"),
        flag(hit.back, " back"),
        flag(hit.dot, " dot"),
        hit.hits
    )
}

fn describe_event(event: &GameEvent) -> String {
    match event {
        GameEvent::Hit(hit) => describe_hit(hit),
        GameEvent::Identity { id, name, is_self } => {
            format!("name {id} = {name}{}", flag(*is_self, " (self)"))
        }
        GameEvent::Cast {
            actor,
            target,
            skill,
        } => format!("cast {actor:>8} -> {target:<8} skill {skill:>9}"),
        GameEvent::Summon {
            id,
            owner,
            owner_name,
        } => format!(
            "summon {id} owner {} {}",
            owner.map_or_else(|| "?".to_string(), |owner| owner.to_string()),
            owner_name.as_deref().unwrap_or("")
        ),
        GameEvent::MapLoad { map } => format!("map  {map}"),
        GameEvent::SelfId { id } => format!("self {id}"),
        GameEvent::Caster { id, name } => format!("name {id} = {name} (caster)"),
        GameEvent::Party { members } => {
            let list: Vec<String> = members
                .iter()
                .map(|member| format!("{}#{}", member.name, member.char_id))
                .collect();
            format!("party {}", list.join(", "))
        }
        GameEvent::PartyJoin(member) => format!("party+ {}#{}", member.name, member.char_id),
        GameEvent::PartyMemberLeft { char_id } => format!("party- #{char_id}"),
        GameEvent::PartyLeft => "party left".to_string(),
        GameEvent::NpcSpawn { id, template } => format!("npc  {id} type {template}"),
        GameEvent::Death { id } => format!("died {id}"),
        GameEvent::PetCollection { levels, souls } => {
            format!("pets {} owned, {} with souls", levels.len(), souls.len())
        }
        GameEvent::PetSouls(picked) => {
            let list: Vec<String> = picked
                .iter()
                .map(|(id, amount)| format!("{id} +{amount}"))
                .collect();
            format!("souls {}", list.join(", "))
        }
        GameEvent::KinahBalance(balance) => format!("kinah balance {balance}"),
        GameEvent::DailyKinah(current) => format!("kinah today {current}"),
        GameEvent::DailyGathering(current) => format!("gathered today {current}"),
        GameEvent::AbyssPoints { current, limit } => format!("abyss {current}/{limit}"),
    }
}

fn live(
    record: Option<&Path>,
    seconds: Option<u64>,
    opcode: Option<u16>,
    quiet: bool,
) -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = stop.clone();
        ctrlc::set_handler(move || stop.store(true, Ordering::Relaxed))?;
    }
    let mut counts: HashMap<u16, u64> = HashMap::new();
    let started = Instant::now();
    sources::live(&stop, record, seconds, |packet| {
        *counts.entry(packet.opcode).or_default() += 1;
        if !quiet && opcode.is_none_or(|wanted| wanted == packet.opcode) {
            print_packet(packet, DEFAULT_DUMP_BYTES);
        }
    })?;
    let total: u64 = counts.values().sum();
    let heartbeats = counts.get(&opcodes::HEARTBEAT).copied().unwrap_or(0);
    eprintln!(
        "done: {total} packets, {heartbeats} heartbeats in {:.1}s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}
