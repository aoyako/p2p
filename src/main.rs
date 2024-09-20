use clap::Parser;
use env_logger::Env;
use log::info;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::hash::Hash;
use std::io::{BufReader, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Period for sending messages (in seconds)
    #[arg(long, default_value_t = 1)]
    period: u32,

    /// Port to bind this peer to
    #[arg(long)]
    port: u16,

    /// Address of a peer to connect to (optional)
    #[arg(long)]
    connect: Option<String>,
}

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
struct Peer {
    address: SocketAddr,
    connections: HashSet<Node>,
    timeout: Duration,
}

impl Peer {
    fn list_addresses(&self) -> Vec<SocketAddr> {
        self.connections.iter().map(|node| node.address).collect()
    }

    fn send_message(&self, node: &Node, msg: &MessageBlock) -> Result<(), std::io::Error> {
        let mut stream = self.get_connection(&node.address)?;
        let marshalled = serde_json::to_string(msg)?;
        stream.write(marshalled.as_bytes())?;
        Ok(())
    }

    fn get_connection(&self, to: &SocketAddr) -> Result<TcpStream, std::io::Error> {
        let stream = TcpStream::connect_timeout(to, Duration::from_secs(1))?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;

        Ok(stream)
    }

    fn new(port: u16, timeout: Duration) -> Peer {
        let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        Peer {
            connections: HashSet::from([Node::new(address)]),
            address: address,
            timeout: timeout,
        }
    }

    fn broadcast(&mut self, msg: &MessageBlock) -> Result<(), std::io::Error> {
        let mut broken_connections = Vec::new();
        for node in &self.connections {
            if node.address != self.address {
                match self.send_message(node, msg) {
                    Ok(_) => (),
                    Err(_) => {
                        broken_connections.push(node.clone());
                    }
                }
            }
        }
        for item in broken_connections {
            self.connections.remove(&item);
        }

        Ok(())
    }

    fn update_connections(&mut self, connections: &HashSet<Node>) {
        self.connections = connections.clone();
    }

    fn ask_connections(&self, addr: &SocketAddr) -> Result<(), std::io::Error> {
        let mut stream = self.get_connection(addr)?;
        let message = MessageBlock::RequestPeerList(self.address.clone());
        let marshalled = serde_json::to_string(&message)?;
        stream.write(marshalled.as_bytes())?;

        Ok(())
    }

    fn send_connections(&self, addr: &SocketAddr) -> Result<(), std::io::Error> {
        let mut stream = self.get_connection(addr)?;
        let message = MessageBlock::UpdatePeerList(self.address.clone(), self.connections.clone());
        let marshalled = serde_json::to_string(&message)?;
        stream.write(marshalled.as_bytes())?;

        Ok(())
    }

    fn joined(&mut self, addr: &SocketAddr) -> Result<(), std::io::Error> {
        self.notify_join(addr);
        self.connections.replace(Node::new(addr.clone()));
        self.send_connections(addr)?;

        self.broadcast(&MessageBlock::PeerJoined(addr.clone()))?;

        Ok(())
    }

    fn notify_join(&mut self, addr: &SocketAddr) {
        self.connections.replace(Node::new(addr.clone()));
    }

    fn generate_message(&self) -> String {
        let mut rng = rand::thread_rng();
        let rnum: i32 = rng.gen_range(0..=100);
        rnum.to_string()
    }
}

fn listen(peer: Arc<Mutex<Peer>>) -> Result<(), std::io::Error> {
    let addr = peer.lock().unwrap().address;
    let listener = TcpListener::bind(&peer.lock().unwrap().address)?;
    info!("My address is {addr}");

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

fn talk(peer: Arc<Mutex<Peer>>, period: u64) -> Result<(), std::io::Error> {
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

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();
    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(1);
    let args = Args::parse();

    let peer = Arc::new(Mutex::new(Peer::new(args.port, DEFAULT_TIMEOUT)));
    let mut handles = Vec::new();

    let peer_sender = Arc::clone(&peer);
    let handle = thread::spawn(move || talk(peer_sender, u64::from(args.period)));
    handles.push(handle);

    let peer_listener = Arc::clone(&peer);
    let handle = thread::spawn(move || listen(peer_listener));
    handles.push(handle);

    let peer_init = Arc::clone(&peer);
    let handle = thread::spawn(move || -> Result<(), std::io::Error> {
        let peer = peer_init.lock().unwrap();
        if let Some(conn) = &args.connect {
            let addr = conn.parse().expect("parse connection string");
            peer.ask_connections(&addr)?;
            info!("Connected to {conn}");
        }
        Ok(())
    });
    handles.push(handle);

    for handle in handles {
        let _ = handle.join().expect("Thread panicked");
    }
}
