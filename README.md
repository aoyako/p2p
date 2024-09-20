# P2P client

Table of contents
* [About](#about)
* [Building](#building)
* [Running](#running)
* [Examples](#examples)

## About
Full mesh simple peer-to-peer client implementation.


## Building
```
cargo build --release
```

## Running
```
./target/release/p2p <args>
```

### Availiable parameters:
1. `--help` shows help information
2. `--period=<seconds>` [default: 1] Period for sending messages (in seconds)
2. `--port=<port>` [required] Binds peer to the selected port
4. `--connect=<address>` Address of a peer to connect to

## Examples

### Three peers
#### Initialization
Peer A:
```
p2p --port=8080 --period=5
```

Peer B:
```
p2p --port=8081 --connect=127.0.0.1:8080 --period=5
```

Peer C:
```
p2p --port=8082 --connect=127.0.0.1:8080 --period=5
```

#### Sample output
Peer A:
```
[1991-01-01T00:00:00Z INFO  p2p::peer] My address is 127.0.0.1:8080
[1991-01-01T00:00:05Z INFO  p2p::peer] Sending message [x] to [127.0.0.1:8081]
[1991-01-01T00:00:06Z INFO  p2p::peer] Received message [x] from 127.0.0.1:8081
[1991-01-01T00:00:10Z INFO  p2p::peer] Sending message [x] to [127.0.0.1:8082, 127.0.0.1:8081]
...
```

Peer B:
```
[1991-01-01T00:00:01Z INFO  p2p::peer] My address is 127.0.0.1:8081
[1991-01-01T00:00:05Z INFO  p2p::peer] Received message [x] from 127.0.0.1:8080
[1991-01-01T00:00:06Z INFO  p2p::peer] Sending message [x] to [127.0.0.1:8080]
[1991-01-01T00:00:10Z INFO  p2p::peer] Received message [x] from 127.0.0.1:8080
[1991-01-01T00:00:11Z INFO  p2p::peer] Sending message [x] to [127.0.0.1:8082, 127.0.0.1:8081]
...
```

Peer C:
```
[1991-01-01T00:00:06Z INFO  p2p::peer] My address is 127.0.0.1:8082
[1991-01-01T00:00:10Z INFO  p2p::peer] Received message [x] from 127.0.0.1:8080
[1991-01-01T00:00:11Z INFO  p2p::peer] Sending message [x] to [127.0.0.1:8080, 127.0.0.1:8081]
[1991-01-01T00:00:11Z INFO  p2p::peer] Received message [x] from 127.0.0.1:8080
[1991-01-01T00:00:11Z INFO  p2p::peer] Received message [x] from 127.0.0.1:8081
...
```