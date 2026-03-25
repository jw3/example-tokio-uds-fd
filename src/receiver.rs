extern crate core;

use clap::Parser;
use example_tokio_uds_fd::FileMetadata;
use nix::sys::socket::{recvmsg, ControlMessageOwned, MsgFlags};
use std::fs;
use std::io::{ IoSliceMut, Read};
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use anyhow::bail;
use nix::cmsg_space;
use nix::errno::Errno;
use nix::libc::pathconf;
use tokio::net::{UnixListener, UnixStream};

#[derive(Debug, Parser)]
pub struct Opts {
    /// path to create socket at
    socket_path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts: Opts = Opts::parse();
    if opts.socket_path.exists() {
        fs::remove_file(&opts.socket_path)?;
    }

    let mut rx = SocketRx::new(&opts.socket_path);
    ctrlc::set_handler({
        let sock = opts.socket_path.clone();
        let total_bytes = rx.total_received.clone();
        move || {
            println!(
                "\ntotal bytes received {}\ndone...",
                total_bytes.load(Ordering::Relaxed)
            );
            let _ = fs::remove_file(&sock);
            exit(0);
        }
    })
    .expect("ctrl+c");

    println!("starting on socket: {}", opts.socket_path.display());
    rx.listen().await
}

struct SocketRx {
    socket_path: String,
    total_received: Arc<AtomicUsize>,
}

impl SocketRx {
    pub fn new<P: AsRef<Path>>(socket_path: P) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_string_lossy().to_string(),
            total_received: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub async fn listen(&mut self) -> anyhow::Result<()> {
        println!("uds @ {}", self.socket_path);
        let listener = UnixListener::bind(&self.socket_path)?;

        let permissions = fs::Permissions::from_mode(0o666);
        fs::set_permissions(&self.socket_path, permissions)?;

        println!("listening...");

        while let Ok((stream, _)) = listener.accept().await {
            println!("connected...");
            if let Err(e) = self.handle(stream).await {
                eprintln!("error handling connection: {e}");
            }
        }
        Ok(())
    }

    async fn handle(&mut self, stream: UnixStream) -> anyhow::Result<()> {
        let mut i = 0;
        loop {
            stream.readable().await?;
            let mut cmsg_buf = cmsg_space!(RawFd);
            let mut header = HeaderData::default();

            let cmsgs: Vec<_> = match recvmsg::<()>(stream.as_raw_fd(), &mut header.vectors(), Some(&mut cmsg_buf), MsgFlags::empty()) {
                Ok(res) => {
                    if res.bytes == 0 {
                        println!(">> done <<");
                        break;
                    }
                    res.cmsgs()?.collect()
                },
                Err(Errno::EAGAIN) => continue,
                Err(e) => bail!("recvmsg 1 failed: {e}"),
            };

            stream.readable().await?;
            let mut payload: PayloadData = header.into();
            let sz = match  recvmsg::<()>(stream.as_raw_fd(), &mut payload.vectors(), None, MsgFlags::empty()) {
                Ok(res) => res.bytes,
                Err(e) => bail!("recvmsg 2 failed: {e}"),
            };

            println!("payload 1: {}", payload.d1.len());
            println!("payload2- {}", String::from_utf8_lossy(payload.d2.as_slice()));

            match (sz, cmsgs) {
                (0, _) => break,
                (_, cmsgs) => match bincode::deserialize::<FileMetadata>(payload.d1.as_slice()) {
                    Ok(metadata) => {
                        i += 1;
                        println!("=========={i} From iov==========");
                        println!("Received {:?} metadata (payload1):", metadata.file_type);
                        println!("\tPath: {}", metadata.path);
                        println!("\tType: {:?}", metadata.file_type);
                        println!("\tSize: {} bytes", metadata.size);
                        println!("\tPermissions: {:o}", metadata.permissions);
                        println!("\tMIME: {}", metadata.mime_type);
                        println!("\tExecutable: {}", metadata.is_executable);

                        let mut received_fd: Option<OwnedFd> = None;
                        for cmsg in cmsgs {
                            match cmsg {
                                ControlMessageOwned::ScmRights(fds) => {
                                    if !fds.is_empty() {
                                        // take ownership of the fd
                                        received_fd = Some(unsafe { OwnedFd::from_raw_fd(fds[0]) });
                                        println!("\tfd: {}", fds[0]);
                                    }
                                }
                                other => {
                                    println!("\tother ctrl-msg: {other:?}");
                                }
                            }
                        }
                        if let Some(fd) = received_fd {
                            let mut file = fs::File::from(fd);

                            let mut contents = String::new();
                            match file.read_to_string(&mut contents) {
                                Ok(bytes_read) => {
                                    // keep a running count of total bytes received
                                    self.total_received.fetch_add(bytes_read, Ordering::Relaxed);

                                    // dump small files or first 128 of large ones
                                    println!("\tsize: {}", bytes_read);
                                    if contents.len() > 128 {
                                        println!("\tpreview:\n{}", &contents[..128]);
                                    } else {
                                        println!("\tcontent:\n{}", contents);
                                    }
                                }
                                Err(e) => {
                                    println!("error: could not read from file descriptor: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("failed to deserialize metadata: {}", e);
                    }
                }
            }
        }

        Ok(())
    }
}

impl Drop for SocketRx {
    fn drop(&mut self) {
        if fs::remove_file(&self.socket_path).is_err() {
            println!("rx: error rm socket file {}", self.socket_path);
        }
    }
}

#[derive(Default)]
struct HeaderData {
    t: [u8; 2],
    s1: [u8; 2],
    s2: [u8; 2],
}
impl HeaderData {
    fn t(&self) -> u16 {
        u16::from_ne_bytes(self.t)
    }
    fn s1(&self) -> u16 {
        u16::from_ne_bytes(self.s1)
    }
    fn s2(&self) -> u16 {
        u16::from_ne_bytes(self.s2)
    }
    fn vectors(&mut self) -> Vec<IoSliceMut<'_>> {
        vec![IoSliceMut::new(&mut self.t), IoSliceMut::new(&mut self.s1), IoSliceMut::new(&mut self.s2)]
    }
}

struct PayloadData {
    s1: u16,
    s2: u16,
    d1: Vec<u8>,
    d2: Vec<u8>,
}

impl PayloadData {
    fn new(s1: u16, s2: u16) -> Self {
        PayloadData {
            s1, s2,
            d1: vec![],
            d2: vec![],
        }
    }
    fn vectors(&mut self) -> Vec<IoSliceMut<'_>> {
        self.d1.resize(self.s1 as usize, 0);
        self.d2.resize(self.s2 as usize, 0);
        vec![IoSliceMut::new(&mut self.d1), IoSliceMut::new(&mut self.d2)]
    }
}

impl From<HeaderData> for PayloadData {
    fn from(hd: HeaderData) -> Self {
        PayloadData::new(hd.s1(), hd.s2())
    }
}
