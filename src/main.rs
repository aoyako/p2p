mod peer;

use crate::peer::*;
use clap::Parser;
use env_logger::Env;
use log::info;
use std::sync::mpsc;
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

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();
    let args = Args::parse();
    let (tx, rx) = mpsc::channel::<()>();
    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(1);

    let peer = Arc::new(Mutex::new(Peer::new(args.port, DEFAULT_TIMEOUT)));
    let mut handles = Vec::new();

    let peer_sender = Arc::clone(&peer);
    let handle = thread::spawn(move || talk(peer_sender, u64::from(args.period)).expect("talk"));
    handles.push(handle);

    let peer_listener = Arc::clone(&peer);
    let handle = thread::spawn(move || listen(peer_listener, tx).expect("listen"));
    handles.push(handle);

    let peer_init = Arc::clone(&peer);
    let handle = thread::spawn(move || {
        rx.recv().unwrap();
        let peer = peer_init.lock().unwrap();
        if let Some(conn) = &args.connect {
            let addr = conn.parse().expect("parse connection string");
            peer.ask_connections(&addr).expect("connect");
            info!("Connected to {conn}");
        }
    });
    handles.push(handle);

    for handle in handles {
        let _ = handle.join().expect("Thread panicked");
    }
}
