extern crate core;

use clap::Parser;
use example_tokio_uds_fd::FileMetadata;
use nix::sys::socket::{recvmsg, ControlMessageOwned, MsgFlags};
use std::fs;
use std::io::{IoSliceMut, Read};
use std::os::fd::FromRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Parser)]
pub struct Opts {
    /// path to create socket at
    socket_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
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
    rx.listen()
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

    pub fn listen(&mut self) -> anyhow::Result<()> {
        println!("uds @ {}", self.socket_path);
        let listener = UnixListener::bind(&self.socket_path)?;

        let permissions = fs::Permissions::from_mode(0o666);
        fs::set_permissions(&self.socket_path, permissions)?;

        println!("listening...");

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    println!("connected...");
                    if let Err(e) = self.handle(stream) {
                        eprintln!("error handling connection: {e}");
                    }
                }
                Err(e) => {
                    eprintln!("error connecting: {e}");
                }
            }
        }
        Ok(())
    }

    fn handle(&mut self, stream: UnixStream) -> anyhow::Result<()> {
        loop {
            let mut cmsg_buf = vec![0u8; 1024];

            let mut t = [0u8; 2];
            let mut size = [0u8; 2];
            let mut iov = [IoSliceMut::new(&mut t), IoSliceMut::new(&mut size)];

            let (payload_sz, cmsg) = match recvmsg::<()>(stream.as_raw_fd(), &mut iov, Some(&mut cmsg_buf), MsgFlags::empty()) {
                Ok(res) => {
                    let mut iter = res.iovs();
                    if let Some(iov) =  iter.next() {
                        println!("type: ------------- {:#x}", u16::from_be_bytes(iov.try_into()?));
                    }
                    let iov = iter.next().ok_or(anyhow::anyhow!("no iov"))?;
                    let sz = u16::from_be_bytes(iov.try_into()?);
                    println!("size: ------------- {sz}");
                    (sz, res.cmsgs()?.collect())
                },
                Err(_) => (0, vec![]),
            };

            let mut recv_buf = vec![0u8; payload_sz as usize];
            let mut payload = [IoSliceMut::new(&mut recv_buf)];

            let (sz, payload) =  match recvmsg::<()>(stream.as_raw_fd(), &mut payload, None, MsgFlags::empty()) {
                Ok(res) => {
                    let payload_1 = res.iovs().next().unwrap();
                    (res.bytes, Some(payload_1))
                }
                Err(e) => (0, None),
            };

            match (payload_sz, payload, cmsg) {
                (0, _, _) => break,
                (bytes, Some(iov ), cmsgs) => match bincode::deserialize::<FileMetadata>(iov) {
                    Ok(metadata) => {
                        println!("==========From iov==========");
                        println!("Received {:?} metadata:", metadata.file_type);
                        println!("\tPath: {}", metadata.path);
                        println!("\tType: {:?}", metadata.file_type);
                        println!("\tSize: {} bytes", metadata.size);
                        println!("\tPermissions: {:o}", metadata.permissions);
                        println!("\tMIME: {}", metadata.mime_type);
                        println!("\tExecutable: {}", metadata.is_executable);

                        let mut received_fd: Option<RawFd> = None;
                        for cmsg in cmsgs {
                            match cmsg {
                                ControlMessageOwned::ScmRights(fds) => {
                                    if !fds.is_empty() {
                                        received_fd = Some(fds[0]);
                                        println!("\tfd: {}", fds[0]);
                                    }
                                }
                                other => {
                                    println!("\tother ctrl-msg: {other:?}");
                                }
                            }
                        }
                        if let Some(fd) = received_fd {
                            // take ownership of the fd
                            let mut file = unsafe { fs::File::from_raw_fd(fd) };

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
                },
                (bytes, None, _) => {
                    println!("no iov - received bytes: {:?}", bytes);
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
