use std::collections::HashSet;
use std::net::{IpAddr, SocketAddrV4};

use anyhow::Result;
use meter_core::net::FlowKey;
use netstat2::{
    AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpSocketInfo, TcpState,
    get_sockets_info,
};
use sysinfo::{ProcessesToUpdate, System};

const PROCESS_STEMS: &[&str] = &["aion2", "aion2-win64-shipping"];
const EXE_SUFFIX: &str = ".exe";
const WEB_PORTS: [u16; 2] = [80, 443];

pub fn is_game_process(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(EXE_SUFFIX).unwrap_or(&lower);
    PROCESS_STEMS.contains(&stem)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameConn {
    pub local: SocketAddrV4,
    pub remote: SocketAddrV4,
    pub pid: u32,
}

impl GameConn {
    pub fn inbound(&self) -> FlowKey {
        FlowKey {
            src: self.remote,
            dst: self.local,
        }
    }
}

pub struct GameLocator {
    system: System,
}

impl Default for GameLocator {
    fn default() -> Self {
        Self::new()
    }
}

impl GameLocator {
    pub fn new() -> Self {
        Self {
            system: System::new(),
        }
    }

    pub fn pids(&mut self) -> HashSet<u32> {
        self.system.refresh_processes(ProcessesToUpdate::All, true);
        self.system
            .processes()
            .iter()
            .filter(|(_, process)| is_game_process(&process.name().to_string_lossy()))
            .map(|(pid, _)| pid.as_u32())
            .collect()
    }

    pub fn connections(&mut self) -> Result<Vec<GameConn>> {
        let pids = self.pids();
        if pids.is_empty() {
            return Ok(Vec::new());
        }
        let mut conns = Vec::new();
        for socket in get_sockets_info(AddressFamilyFlags::IPV4, ProtocolFlags::TCP)? {
            let ProtocolSocketInfo::Tcp(tcp) = socket.protocol_socket_info else {
                continue;
            };
            if !is_game_traffic_candidate(&tcp) {
                continue;
            }
            let Some(&pid) = socket.associated_pids.iter().find(|pid| pids.contains(pid)) else {
                continue;
            };
            let (IpAddr::V4(local_ip), IpAddr::V4(remote_ip)) = (tcp.local_addr, tcp.remote_addr)
            else {
                continue;
            };
            conns.push(GameConn {
                local: SocketAddrV4::new(local_ip, tcp.local_port),
                remote: SocketAddrV4::new(remote_ip, tcp.remote_port),
                pid,
            });
        }
        Ok(drop_internal_pairs(conns))
    }
}

fn is_game_traffic_candidate(tcp: &TcpSocketInfo) -> bool {
    tcp.state == TcpState::Established && !WEB_PORTS.contains(&tcp.remote_port)
}

pub fn drop_internal_pairs(conns: Vec<GameConn>) -> Vec<GameConn> {
    let locals: HashSet<SocketAddrV4> = conns.iter().map(|conn| conn.local).collect();
    conns
        .into_iter()
        .filter(|conn| !locals.contains(&conn.remote))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_game_process_names() {
        assert!(is_game_process("AION2.exe"));
        assert!(is_game_process("Aion2-Win64-Shipping.exe"));
        assert!(is_game_process("aion2"));
        assert!(!is_game_process("chrome.exe"));
        assert!(!is_game_process("aion2launcher.exe"));
    }

    fn conn(local: &str, remote: &str) -> GameConn {
        GameConn {
            local: local.parse().unwrap(),
            remote: remote.parse().unwrap(),
            pid: 1,
        }
    }

    #[test]
    fn internal_loopback_pairs_are_dropped() {
        let conns = vec![
            conn("127.0.0.1:19649", "127.0.0.1:19650"),
            conn("127.0.0.1:19650", "127.0.0.1:19649"),
            conn("192.168.1.39:19690", "193.202.112.124:13328"),
        ];
        assert_eq!(
            drop_internal_pairs(conns),
            vec![conn("192.168.1.39:19690", "193.202.112.124:13328")]
        );
    }

    #[test]
    fn proxy_loopback_conn_is_kept() {
        let conns = vec![conn("127.0.0.1:50000", "127.0.0.1:7000")];
        assert_eq!(drop_internal_pairs(conns.clone()), conns);
    }

    #[test]
    fn inbound_is_server_to_client() {
        let game = conn("192.168.1.5:50000", "10.0.0.1:7777");
        assert_eq!(game.inbound().src, game.remote);
        assert_eq!(game.inbound().dst, game.local);
    }
}
