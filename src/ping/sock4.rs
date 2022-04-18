use super::probe::Probe;
use super::state::State;
use crate::icmp::icmp4::checksum;
use crate::icmp::IcmpV4Packet;
use crate::Bind;
use anyhow::Result;
use log::{debug, error};
use raw_socket::tokio::RawSocket;
use raw_socket::{Domain, Protocol, Type};
use std::convert::{TryFrom, TryInto};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

pub struct Sock4 {
    recv: JoinHandle<()>,
    sock: Mutex<Arc<RawSocket>>,
}

impl Sock4 {
    pub async fn new(bind: &Bind, state: Arc<State>) -> Result<Self> {
        let dgram = Type::dgram();
        let icmp4 = Protocol::icmpv4();

        let sock = Arc::new(RawSocket::new(Domain::ipv4(), dgram, Some(icmp4))?);
        sock.bind(bind.sa4()).await?;
        let rx = sock.clone();

        let recv = tokio::spawn(async move {
            match recv(rx, state).await {
                Ok(()) => debug!("recv finished"),
                Err(e) => error!("recv failed: {}", e),
            }
        });

        Ok(Self {
            recv,
            sock: Mutex::new(sock),
        })
    }

    pub async fn send(&self, probe: &Probe) -> Result<Instant> {
        let mut pkt = [0u8; 64];

        let pkt = probe.encode(&mut pkt)?;
        let cksum = checksum(pkt).to_be_bytes();
        pkt[2..4].copy_from_slice(&cksum);

        let addr = SocketAddr::new(probe.addr, 0);
        let sock = self.sock.lock().await;
        sock.send_to(pkt, &addr).await?;

        Ok(Instant::now())
    }
}

#[cfg(not(any(ios, mac)))]
async fn recv(sock: Arc<RawSocket>, state: Arc<State>) -> Result<()> {
    let mut pkt = [0u8; 128];
    loop {
        let (n, _) = sock.recv_from(&mut pkt).await?;
        let now = Instant::now();
        if let IcmpV4Packet::EchoReply(echo) = IcmpV4Packet::try_from(&pkt[0..n])? {
            if let Ok(token) = echo.data.try_into() {
                if let Some(tx) = state.remove(&token) {
                    let _ = tx.send(now);
                }
            }
        }
    }
}

#[cfg(any(ios, mac))]
async fn recv(sock: Arc<RawSocket>, state: Arc<State>) -> Result<()> {
    use etherparse::{IpNumber, Ipv4Header};
    const ICMP4: u8 = IpNumber::Icmp as u8;
    let mut pkt = [0u8; 128];
    loop {
        let (n, _) = sock.recv_from(&mut pkt).await?;
        let now = Instant::now();
        let pkt = Ipv4Header::from_slice(&pkt[..n])?;
        if let (
            Ipv4Header {
                protocol: ICMP4, ..
            },
            tail,
        ) = pkt
        {
            if let IcmpV4Packet::EchoReply(echo) = IcmpV4Packet::try_from(tail)? {
                if let Ok(token) = echo.data.try_into() {
                    if let Some(tx) = state.remove(&token) {
                        let _ = tx.send(now);
                    }
                }
            }
        }
    }
}

impl Drop for Sock4 {
    fn drop(&mut self) {
        self.recv.abort();
    }
}
