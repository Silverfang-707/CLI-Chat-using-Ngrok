# AIRAA Decentralized Chat

AIRAA is a decentralized, encrypted terminal chat platform built in Rust.

It supports:

* Local LAN communication
* Ngrok TCP tunneling
* Tor Hidden Services (.onion)
* End-to-end encrypted rooms
* Private encrypted whispers
* Multi-room chat
* User discovery
* Anonymous darknet communication

No central server is required. Any user can host a node and create a communication grid.

---

# Features

## Secure Rooms

Every room is protected using a shared password.

Messages are encrypted before transmission and decrypted only by clients that possess the correct room password.

---

## Whisper Messaging

Private messages can be sent directly to another user:

```text
/w Alice Hello there
```

Whispers use recipient-specific encryption keys and are not readable by other room members.

---

## Tor Hidden Services

AIRAA can expose a room through the Tor network.

Hosts can generate a unique `.onion` address and share it with others.

Benefits:

* IP address hidden
* Anonymous communication
* No port forwarding required
* Works behind NAT

---

## Ngrok Tunnels

AIRAA supports Ngrok TCP tunnels for users who prefer a simpler public uplink.

If Ngrok is not installed:

* AIRAA can install it using Winget
* Users provide their Ngrok authtoken
* AIRAA automatically establishes a tunnel

---

## Live User Discovery

Display active users in the room:

```text
/u
```

---

## Multi-Room Support

Users may:

* Create rooms
* Join existing rooms
* View active rooms on the grid

Each room has independent encryption.

---

# Commands

| Command                | Description            |
| ---------------------- | ---------------------- |
| `/h`                   | Show help              |
| `/u`                   | List active users      |
| `/w <alias> <message>` | Send encrypted whisper |
| `/!exit!`              | Exit chat              |
| `ESC`                  | Exit chat              |

---

# Installation

## Requirements

Windows 10/11

Rust 1.80+

Cargo

---

# Build

```bash
cargo build --release
```

Compiled binaries:

```text
target/release/
├── airaa-chat.exe
├── server.exe
└── client.exe
```

---

# Tor Setup

AIRAA expects the Tor Browser folder to exist beside the executable.

Directory structure:

```text
AIRAA/
├── airaa-chat.exe
├── server.exe
├── client.exe
└── tor/
    └── tor/
        ├── tor.exe
        ├── geoip
        ├── geoip6
        └── ...
```

Download Tor Browser:

https://www.torproject.org/download/

---

# Hosting a Node

Launch:

```bash
airaa-chat.exe
```

Choose:

```text
1) Host a Node & Chat
```

Select:

```text
1) Local Network
2) Ngrok Tunnel
3) Tor Hidden Service
```

Choose a port:

```text
3000
```

AIRAA launches:

* Local server
* Tunnel/hidden service
* Client UI

---

# Joining a Node

Launch:

```bash
airaa-chat.exe
```

Choose:

```text
2) Join a Node
```

Examples:

Local:

```text
192.168.1.50:3000
```

Ngrok:

```text
0.tcp.ngrok.io:12345
```

Tor:

```text
examplehiddenservice.onion
```

---

# Encryption Model

Room Encryption:

```text
Room Password
        +
Room Name
        ↓
 AES Key
```

Whisper Encryption:

```text
Room Password
        +
Room Name
        +
Recipient Alias
        ↓
 Whisper Key
```

Only the intended recipient can decrypt whisper messages.

---

# Security Notes

AIRAA is designed for privacy-oriented communication.

Recommendations:

* Use strong room passwords
* Use Tor mode for anonymous communication
* Avoid sharing room passwords publicly
* Verify recipient aliases before sending whispers

---

# Troubleshooting

## Tor Not Found

Ensure the Tor Browser folder is placed beside AIRAA.

Expected:

```text
AIRAA/
└── Tor Browser/
```

---

## Ngrok Not Found

Install manually:

https://ngrok.com/download

or allow AIRAA to install Ngrok through Winget.

---

## Cannot Connect To Onion Address

Ensure Tor is running locally.

AIRAA requires a local Tor SOCKS5 proxy:

```text
127.0.0.1:9050
```

---

## Connection Timeout

Check:

* Firewall rules
* Room password
* Host availability
* Tor connectivity

---

# Project Structure

```text
airaa-chat/
├── src/
│   ├── main.rs
│   ├── client.rs
│   ├── server.rs
│   └── crypto.rs
│
├── Tor Browser/
│
├── Cargo.toml
└── README.md
```

---

Copyright (c) AIRAA
