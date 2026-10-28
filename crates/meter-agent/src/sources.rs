use std::collections::HashSet;
use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use meter_capture::locator::{GameConn, GameLocator};
use meter_capture::pcap_io;
use meter_core::net::{FlowKey, RawFrame};
use meter_core::pipeline::{GamePacket, Pipeline};

const INTERRUPTED: &str = "interrupted";
const GAME_POLL_INTERVAL: Duration = Duration::from_secs(2);
const CONNECTION_REFRESH_INTERVAL: Duration = Duration::from_secs(2);
const FRAME_WAIT: Duration = Duration::from_millis(250);
const MICROS_PER_SECOND: f64 = 1e6;

pub fn is_interrupted(error: &anyhow::Error) -> bool {
    error.to_string() == INTERRUPTED
}

fn wait_for_game(locator: &mut GameLocator, stop: &AtomicBool) -> Result<Vec<GameConn>> {
    loop {
        let conns = locator.connections()?;
        if !conns.is_empty() {
            return Ok(conns);
        }
        if stop.load(Ordering::Relaxed) {
            bail!(INTERRUPTED);
        }
        eprintln!("waiting for AION 2 connections...");
        std::thread::sleep(GAME_POLL_INTERVAL);
    }
}

fn inbound_flows(conns: &[GameConn]) -> HashSet<FlowKey> {
    conns.iter().map(GameConn::inbound).collect()
}

fn emit_packets(
    pipeline: &mut Pipeline,
    frame: &RawFrame,
    packets: &mut Vec<GamePacket>,
    on_packet: &mut impl FnMut(&GamePacket),
) {
    pipeline.push(frame, packets);
    for packet in packets.drain(..) {
        on_packet(&packet);
    }
}

pub fn live(
    stop: &Arc<AtomicBool>,
    record: Option<&Path>,
    seconds: Option<u64>,
    mut on_packet: impl FnMut(&GamePacket),
) -> Result<()> {
    let mut locator = GameLocator::new();
    let conns = wait_for_game(&mut locator, stop)?;
    for conn in &conns {
        eprintln!(
            "game conn: pid {} {} -> {}",
            conn.pid, conn.local, conn.remote
        );
    }
    let device = pcap_io::device_for_ip(IpAddr::V4(*conns[0].local.ip()))?;
    eprintln!("capturing on {}", device.name);

    let (frame_sender, frames) = mpsc::channel();
    let record_path = record.map(Path::to_path_buf);
    let capture_stop = stop.clone();
    let capture = std::thread::spawn(move || {
        pcap_io::capture_live(device, record_path.as_deref(), capture_stop, frame_sender)
    });

    let mut pipeline = Pipeline::new();
    pipeline.set_allowed(Some(inbound_flows(&conns)));
    let started = Instant::now();
    let mut last_refresh = Instant::now();
    let mut packets = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        if seconds.is_some_and(|secs| started.elapsed() >= Duration::from_secs(secs)) {
            break;
        }
        match frames.recv_timeout(FRAME_WAIT) {
            Ok(frame) => emit_packets(&mut pipeline, &frame, &mut packets, &mut on_packet),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if last_refresh.elapsed() >= CONNECTION_REFRESH_INTERVAL {
            last_refresh = Instant::now();
            let conns = locator.connections()?;
            if !conns.is_empty() {
                pipeline.set_allowed(Some(inbound_flows(&conns)));
            }
        }
    }
    stop.store(true, Ordering::Relaxed);
    drop(frames);
    capture
        .join()
        .map_err(|_| anyhow!("capture thread panicked"))?
}

pub fn replay(
    file: &Path,
    speed: f64,
    stop: &AtomicBool,
    mut on_packet: impl FnMut(&GamePacket),
) -> Result<()> {
    let mut pipeline = Pipeline::new();
    let mut packets = Vec::new();
    let mut origin: Option<(i64, Instant)> = None;
    pcap_io::read_pcap(file, |frame| {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        if speed > 0.0 {
            let (first_ts_us, started) = *origin.get_or_insert((frame.ts_us, Instant::now()));
            let capture_elapsed_secs =
                (frame.ts_us - first_ts_us).max(0) as f64 / MICROS_PER_SECOND;
            let due = Duration::from_secs_f64(capture_elapsed_secs / speed);
            if let Some(wait) = due.checked_sub(started.elapsed()) {
                std::thread::sleep(wait);
            }
        }
        emit_packets(&mut pipeline, &frame, &mut packets, &mut on_packet);
    })
}
