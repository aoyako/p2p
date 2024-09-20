use log::info;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::io::{BufReader, Write};
use std::net::SocketAddr;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MessageBlock<T: NodeSender> {
    UpdatePeerList(T, HashSet<T>),
    RequestPeerList(T),
    Info(T, String),
    PeerJoined(T),
}

#[derive(Eq, Hash, PartialEq, Serialize, Deserialize, Debug, Clone)]
pub struct Node {
    address: SocketAddr,
    timeout: Duration,
}

impl Node {
    pub fn new(address: SocketAddr, timeout: Duration) -> Node {
        Node { address, timeout }
    }

    pub fn get_connection(&self, to: &SocketAddr) -> Result<TcpStream, std::io::Error> {
        let stream = TcpStream::connect_timeout(to, self.timeout)?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;

        Ok(stream)
    }
}

impl NodeSender for Node {
    fn get_addr(&self) -> SocketAddr {
        self.address
    }

    fn send_message(
        &self,
        node: &impl NodeSender,
        msg: &MessageBlock<impl NodeSender>,
    ) -> Result<(), std::io::Error> {
        let mut stream = self.get_connection(&node.get_addr())?;
        let marshalled = serde_json::to_string(msg)?;
        stream.write(marshalled.as_bytes())?;
        Ok(())
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct Peer<T: NodeSender> {
    body: T,
    connections: HashSet<T>,
}

pub trait NodeSender: Eq + Hash + Clone + Debug + std::marker::Send + Serialize {
    fn get_addr(&self) -> SocketAddr;
    fn send_message(
        &self,
        node: &impl NodeSender,
        msg: &MessageBlock<impl NodeSender>,
    ) -> Result<(), std::io::Error>;
}

impl<T: NodeSender + 'static> Peer<T> {
    pub fn list_addresses(&self) -> Vec<SocketAddr> {
        self.connections
            .iter()
            .map(|node| node.get_addr())
            .collect()
    }

    pub fn new(node: T) -> Peer<T> {
        Peer {
            connections: HashSet::from([node.clone()]),
            body: node,
        }
    }

    fn broadcast(&mut self, msg: &MessageBlock<T>) -> Result<(), Box<dyn std::error::Error>> {
        let broken_connections = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();

        for node in self.connections.clone() {
            if node != self.body {
                let broken_connections = Arc::clone(&broken_connections);
                let msg = msg.clone();
                let node_clone = node.clone();
                let peer = self.clone();
                let handle = thread::spawn(move || {
                    if let Err(_) = peer.body.send_message(&node_clone, &msg) {
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

    fn update_connections(&mut self, connections: &HashSet<T>) {
        self.connections = connections.clone();
    }

    pub fn ask_connections(&self, node: &T) -> Result<(), std::io::Error> {
        let message = MessageBlock::<T>::RequestPeerList(self.body.clone());
        self.body.send_message(node, &message)?;

        Ok(())
    }

    pub fn send_connections(&self, node: &T) -> Result<(), std::io::Error> {
        let message = MessageBlock::UpdatePeerList(self.body.clone(), self.connections.clone());
        self.body.send_message(node, &message)?;

        Ok(())
    }

    pub fn joined(&mut self, node: &T) -> Result<(), Box<dyn std::error::Error>> {
        self.notify_join(node);
        self.connections.replace(node.clone());
        self.send_connections(node)?;

        self.broadcast(&MessageBlock::PeerJoined(node.clone()))?;

        Ok(())
    }

    pub fn notify_join(&mut self, node: &T) {
        self.connections.replace(node.clone());
    }

    pub fn generate_message(&self) -> String {
        let mut rng = rand::thread_rng();
        let rnum: i32 = rng.gen_range(0..=100);
        rnum.to_string()
    }

    fn proc_message(&mut self, msg: &MessageBlock<T>) -> Result<(), Box<dyn std::error::Error>> {
        match msg {
            MessageBlock::Info(from, data) => {
                info!("Received message [{data}] from {}", from.get_addr())
            }
            MessageBlock::RequestPeerList(from) => self.joined(from)?,
            MessageBlock::UpdatePeerList(_, peers_data) => self.update_connections(&peers_data),
            MessageBlock::PeerJoined(from) => self.notify_join(&from),
        }

        Ok(())
    }
}

pub fn talk<N: NodeSender + 'static>(
    peer: Arc<Mutex<Peer<N>>>,
    period: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let interval_duration = Duration::from_secs(period);
    loop {
        thread::sleep(interval_duration);
        let mut peer = peer.lock().unwrap();
        let msg = peer.generate_message();

        let peer_list: Vec<_> = peer
            .list_addresses()
            .iter()
            .cloned()
            .filter(|addr| addr != &peer.body.get_addr())
            .collect();

        info!("Sending message [{msg}] to {peer_list:?}");

        let body = peer.body.clone();
        peer.broadcast(&MessageBlock::Info(body, msg))?;
    }
}

pub fn listen<N: NodeSender + 'static>(
    peer: Arc<Mutex<Peer<N>>>,
    tx: mpsc::Sender<()>,
) -> Result<(), Box<dyn std::error::Error>>
where
    N: for<'a> Deserialize<'a>,
{
    let addr = peer.lock().unwrap().body.get_addr().clone();
    let listener = TcpListener::bind(addr)?;
    info!("My address is {addr}");
    tx.send(())?;

    for stream in listener.incoming() {
        let mut stream = stream?;

        let buf_reader = BufReader::new(&mut stream);
        let mut de = serde_json::Deserializer::from_reader(buf_reader);
        let req = MessageBlock::<N>::deserialize(&mut de)?;

        let mut peer = peer.lock().unwrap();
        peer.proc_message(&req)?;
    }

    Ok(())
}
