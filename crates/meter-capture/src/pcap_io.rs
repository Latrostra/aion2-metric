use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use meter_core::net::RawFrame;
use pcap::{Active, Capture, Device, Packet, PacketHeader};

pub const BPF: &str = "tcp and not port 80 and not port 443";
const LOOPBACK_DEVICE_MARKER: &str = "loopback";
const SNAPLEN: i32 = 65_535;
const KERNEL_BUFFER_BYTES: i32 = 16 * 1024 * 1024;
const READ_TIMEOUT_MS: i32 = 250;
const SAVEFILE_FLUSH_INTERVAL: Duration = Duration::from_secs(1);
const MICROS_PER_SECOND: i64 = 1_000_000;

pub fn list_devices() -> Result<Vec<Device>> {
    Device::list().context("listing Npcap devices (is Npcap installed?)")
}

pub fn device_for_ip(local_ip: IpAddr) -> Result<Device> {
    let devices = list_devices()?;
    if local_ip.is_loopback() {
        return devices
            .into_iter()
            .find(|device| {
                device
                    .name
                    .to_ascii_lowercase()
                    .contains(LOOPBACK_DEVICE_MARKER)
            })
            .ok_or_else(|| {
                anyhow!("Npcap loopback adapter not found; reinstall Npcap with loopback support")
            });
    }
    devices
        .into_iter()
        .find(|device| {
            device
                .addresses
                .iter()
                .any(|address| address.addr == local_ip)
        })
        .ok_or_else(|| anyhow!("no capture device has address {local_ip}"))
}

fn open_live(device: Device) -> Result<Capture<Active>> {
    let mut capture = Capture::from_device(device)?
        .immediate_mode(true)
        .snaplen(SNAPLEN)
        .buffer_size(KERNEL_BUFFER_BYTES)
        .timeout(READ_TIMEOUT_MS)
        .open()
        .context("opening capture (Npcap admin-only mode? try running as administrator)")?;
    capture.filter(BPF, true)?;
    Ok(capture)
}

pub fn capture_live(
    device: Device,
    record: Option<&Path>,
    stop: Arc<AtomicBool>,
    frames: Sender<RawFrame>,
) -> Result<()> {
    let mut capture = open_live(device)?;
    let linktype = capture.get_datalink().0;
    let mut savefile = record
        .map(|path| {
            capture
                .savefile(path)
                .with_context(|| format!("creating {}", path.display()))
        })
        .transpose()?;
    let mut last_flush = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        if let Some(savefile) = savefile.as_mut()
            && last_flush.elapsed() >= SAVEFILE_FLUSH_INTERVAL
        {
            savefile.flush()?;
            last_flush = Instant::now();
        }
        match capture.next_packet() {
            Ok(packet) => {
                if let Some(savefile) = savefile.as_mut() {
                    savefile.write(&packet);
                }
                if frames.send(raw_frame(&packet, linktype)).is_err() {
                    break;
                }
            }
            Err(pcap::Error::TimeoutExpired) => continue,
            Err(error) => return Err(error.into()),
        }
    }
    if let Some(mut savefile) = savefile {
        savefile.flush()?;
    }
    Ok(())
}

pub fn read_pcap(path: &Path, mut on_frame: impl FnMut(RawFrame)) -> Result<()> {
    let mut capture =
        Capture::from_file(path).with_context(|| format!("opening {}", path.display()))?;
    let linktype = capture.get_datalink().0;
    loop {
        match capture.next_packet() {
            Ok(packet) => on_frame(raw_frame(&packet, linktype)),
            Err(pcap::Error::NoMorePackets) => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
}

fn raw_frame(packet: &Packet, linktype: i32) -> RawFrame {
    RawFrame {
        ts_us: ts_us(packet.header),
        linktype,
        data: packet.data.to_vec(),
    }
}

fn ts_us(header: &PacketHeader) -> i64 {
    i64::from(header.ts.tv_sec) * MICROS_PER_SECOND + i64::from(header.ts.tv_usec)
}
