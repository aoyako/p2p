use log::info;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::hash::Hash;
use std::io::{BufReader, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug, Clone)]
enum MessageBlock {
    UpdatePeerList(SocketAddr, HashSet<Node>),
    RequestPeerList(SocketAddr),
    Info(SocketAddr, String),
    PeerJoined(SocketAddr),
}

#[derive(Eq, Hash, PartialEq, Serialize, Deserialize, Debug, Clone)]
struct Node {
    address: SocketAddr,
}

impl Node {
    fn new(addr: SocketAddr) -> Node {
        Node { address: addr }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Peer {
    address: SocketAddr,
    connections: HashSet<Node>,
    timeout: Duration,
}

impl Peer {
    pub fn list_addresses(&self) -> Vec<SocketAddr> {
        self.connections.iter().map(|node| node.address).collect()
    }

    fn send_message(&self, node: &Node, msg: &MessageBlock) -> Result<(), std::io::Error> {
        let mut stream = self.get_connection(&node.address)?;
        let marshalled = serde_json::to_string(msg)?;
        stream.write(marshalled.as_bytes())?;
        Ok(())
    }

    pub fn get_connection(&self, to: &SocketAddr) -> Result<TcpStream, std::io::Error> {
        let stream = TcpStream::connect_timeout(to, Duration::from_secs(1))?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;

        Ok(stream)
    }

    pub fn new(port: u16, timeout: Duration) -> Peer {
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        Peer {
            connections: HashSet::from([Node::new(address)]),
            address: address,
            timeout: timeout,
        }
    }

    fn broadcast(&mut self, msg: &MessageBlock) -> Result<(), Box<dyn std::error::Error>> {
        let broken_connections = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();

        for node in self.connections.clone() {
            if node.address != self.address {
                let broken_connections = Arc::clone(&broken_connections);
                let msg = msg.clone();
                let node_clone = node.clone();
                let peer = self.clone();
                let handle = thread::spawn(move || {
                    if let Err(_) = peer.send_message(&node_clone, &msg) {
                        let mut broken = broken_connections.lock().unwrap();
                        broken.push(node_clone);
                    }
                });

                handles.push(handle);
            }
        }
        for handle in handles {
            handle.join().unwrap();
        }
        let broken_connections = Arc::try_unwrap(broken_connections).unwrap().into_inner()?;
        for item in broken_connections {
            self.connections.remove(&item);
        }

        Ok(())
    }

    fn update_connections(&mut self, connections: &HashSet<Node>) {
        self.connections = connections.clone();
    }

    pub fn ask_connections(&self, addr: &SocketAddr) -> Result<(), std::io::Error> {
        let mut stream = self.get_connection(addr)?;
        let message = MessageBlock::RequestPeerList(self.address.clone());
        let marshalled = serde_json::to_string(&message)?;
        stream.write(marshalled.as_bytes())?;

        Ok(())
    }

    pub fn send_connections(&self, addr: &SocketAddr) -> Result<(), std::io::Error> {
        let mut stream = self.get_connection(addr)?;
        let message = MessageBlock::UpdatePeerList(self.address.clone(), self.connections.clone());
        let marshalled = serde_json::to_string(&message)?;
        stream.write(marshalled.as_bytes())?;

        Ok(())
    }

    pub fn joined(&mut self, addr: &SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        self.notify_join(addr);
        self.connections.replace(Node::new(addr.clone()));
        self.send_connections(addr)?;

        self.broadcast(&MessageBlock::PeerJoined(addr.clone()))?;

        Ok(())
    }

    pub fn notify_join(&mut self, addr: &SocketAddr) {
        self.connections.replace(Node::new(addr.clone()));
    }

    pub fn generate_message(&self) -> String {
        let mut rng = rand::thread_rng();
        let rnum: i32 = rng.gen_range(0..=100);
        rnum.to_string()
    }
}

pub fn talk(peer: Arc<Mutex<Peer>>, period: u64) -> Result<(), Box<dyn std::error::Error>> {
    let interval_duration = Duration::from_secs(period);
    loop {
        thread::sleep(interval_duration);
        let mut peer = peer.lock().unwrap();
        let msg = peer.generate_message();

        let peer_list: Vec<_> = peer
            .list_addresses()
            .iter()
            .cloned()
            .filter(|addr| addr != &peer.address)
            .collect();

        info!("Sending message [{msg}] to {peer_list:?}");

        let adress = peer.address.clone();
        peer.broadcast(&MessageBlock::Info(adress, msg))?;
    }
}

pub fn listen(
    peer: Arc<Mutex<Peer>>,
    tx: mpsc::Sender<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr = peer.lock().unwrap().address.clone();
    let listener = TcpListener::bind(&peer.lock().unwrap().address)?;
    info!("My address is {addr}");
    tx.send(())?;

    for stream in listener.incoming() {
        let mut stream = stream?;

        let buf_reader = BufReader::new(&mut stream);
        let mut de = serde_json::Deserializer::from_reader(buf_reader);
        let req = MessageBlock::deserialize(&mut de)?;

        let mut peer = peer.lock().unwrap();
        match req {
            MessageBlock::Info(from, data) => info!("Received message [{data}] from {from}"),
            MessageBlock::RequestPeerList(from) => peer.joined(&from)?,
            MessageBlock::UpdatePeerList(_, peers_data) => peer.update_connections(&peers_data),
            MessageBlock::PeerJoined(from) => peer.notify_join(&from),
        }
    }

    Ok(())
}
