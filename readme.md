example fd over uds
===

Basic example of metadata + file-descriptor send/receive over a sock_stream uds.

### run

1. start rx `cargo run --bin rx -- /tmp/fdsock`
2. run tx `cargo run --bin tx -- /tmp/fdsock -D src`

<details>
<summary>Expected Output ...</summary>

```text
starting on socket: /tmp/fdsock
uds @ /tmp/fdsock
listening...
connected...
Received RegularFile metadata:
	Path: src/lib.rs
	Type: RegularFile
	Size: 3419 bytes
	Permissions: 100644
	MIME: text/x-rust
	Executable: false
	fd: 7
	size: 3419
	preview:
use nix::sys::stat::{Mode, SFlag};
use serde::{Deserialize, Serialize};
use std::fs::Metadata;
use std::os::unix::fs::{MetadataE
==============================
Received RegularFile metadata:
	Path: src/receiver.rs
	Type: RegularFile
	Size: 6125 bytes
	Permissions: 100644
	MIME: text/x-rust
	Executable: false
	fd: 7
	size: 6125
	preview:
use bincode;
use clap::Parser;
use example_tokio_uds_fd::FileMetadata;
use nix::sys::socket::{recvmsg, ControlMessageOwned, MsgF
==============================
Received RegularFile metadata:
	Path: src/sender.rs
	Type: RegularFile
	Size: 4051 bytes
	Permissions: 100644
	MIME: text/x-rust
	Executable: false
	fd: 7
	size: 4051
	preview:
use anyhow::{bail, Context};
use bincode;
use clap::Parser;
use example_tokio_uds_fd::FileMetadata;
use nix::sys::socket::{sendm
==============================

total bytes received 13595
done...
```

</details>

### notes

- sender - async - tokio
  - requires blocking task for the system calls
    - channel to feed the blocking task
    - minimize work done in it
  - bincode serialization
  - into_raw_fd to forget the fd
- receiver - non-aync
  - bincode deserialization
  - from_raw_fd to take ownership of fd

### todo

there are a few more things that might be interesting
- MultiHeaders sendmsg - for batch fds
- test against a receiver implemented in C
- i forget the other one...
