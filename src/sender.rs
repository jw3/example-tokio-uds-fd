use anyhow::{bail, Context};
use clap::Parser;
use example_tokio_uds_fd::FileMetadata;
use nix::sys::socket::{sendmsg, ControlMessage, MsgFlags, UnixAddr};
use std::fs::File;
use std::io::IoSlice;
use std::os::fd::IntoRawFd;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tokio::task;

#[derive(Debug, Parser)]
pub struct Opts {
    /// source dir of files to xfer, defaults to pwd
    #[clap(short = 'D', long, default_value = ".")]
    source_dir: PathBuf,
    /// existing socket to connect to (created by rx)
    socket_path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    if opts.socket_path.is_dir() || opts.socket_path.is_file() || !opts.socket_path.exists() {
        bail!("{} is not a socket", opts.socket_path.display());
    }

    let tx = SocketTx::new(&opts.socket_path);

    println!(
        "tx metadata for all files in {}/* to {}",
        opts.source_dir.display(),
        opts.socket_path.display()
    );

    match tx.send_dir(opts.source_dir).await {
        Ok(()) => println!("done"),
        Err(e) => println!("error: {}", e),
    }

    Ok(())
}

#[derive(Debug)]
struct Msg {
    pub meta: FileMetadata,
    pub file: Option<File>,
}

struct SocketTx {
    socket: PathBuf,
}

impl SocketTx {
    pub fn new<P: AsRef<Path>>(socket_path: P) -> Self {
        Self {
            socket: socket_path.as_ref().to_path_buf(),
        }
    }

    pub async fn send_dir<P: AsRef<Path>>(&self, src_dir: P) -> anyhow::Result<()> {
        let (tx, rx) = mpsc::channel::<Msg>(100);

        // spawn blocking
        let syscall = task::spawn_blocking({
            let sock = self.socket.clone();
            move || worker(sock, rx)
        });

        let scan = task::spawn({
            let src_dir = src_dir.as_ref().to_path_buf();
            async move { scan_dir(src_dir, tx).await }
        });

        let (scan_res, send_res) = tokio::try_join!(scan, syscall)?;
        scan_res?;
        send_res?;

        Ok(())
    }
}

// recv messages, send over socket
// syscalls must be made in blocking context
fn worker(socket_path: PathBuf, mut rx: mpsc::Receiver<Msg>) -> anyhow::Result<()> {
    let mut stream = UnixStream::connect(socket_path)?;
    while let Some(message) = rx.blocking_recv() {
        send_msg(&mut stream, message)?;
    }
    Ok(())
}

// send Msg to the socket, requires blocking context
// only split out to make error handling more concise
fn send_msg(stream: &mut UnixStream, mut message: Msg) -> anyhow::Result<()> {
    let serialized = bincode::serialize(&message.meta)?;
    let second_payload = "!*-*-*-*-*-*-*-*-*-*-*!";
    let t = 1u16.to_be_bytes();
    let size1  = (serialized.len() as u16).to_be_bytes();
    let size2  = (second_payload.len() as u16).to_be_bytes();
    let io_slice1 = IoSlice::new(&t);
    let io_slice2 = IoSlice::new(&size1);
    let io_slice3 = IoSlice::new(&size2);
    let io_slice4 = IoSlice::new(&serialized);
    let io_slice5 = IoSlice::new(second_payload.as_bytes());

    let fd_array;
    let control_messages = if let Some(file) = message.file.take() {
        // give up ownership of the fd
        fd_array = [file.into_raw_fd()];
        vec![ControlMessage::ScmRights(&fd_array)]
    } else {
        vec![]
    };

    sendmsg(
        stream.as_raw_fd(),
        &[io_slice1, io_slice2, io_slice3, io_slice4, io_slice5],
        &control_messages,
        MsgFlags::empty(),
        None::<&UnixAddr>,
    )
        .context("tx: failed to send message")?;

    println!("tx: sent {}", message.meta.path);

    Ok(())
}

// read all files from src and send on tx
async fn scan_dir<P: AsRef<Path>>(src: P, tx: mpsc::Sender<Msg>) -> anyhow::Result<()> {
    let mut entries = tokio::fs::read_dir(src).await?;
    while let Some(e) = entries.next_entry().await? {
        let path = e.path();
        let metadata = e.metadata().await?;

        let meta = FileMetadata::new(&path, &metadata)?;
        let file = if metadata.is_file() {
            match File::open(&path) {
                Ok(file) => Some(file),
                Err(_) => {
                    println!("tx: failed to open {}", path.display());
                    None
                }
            }
        } else {
            None
        };

        // the worker will take it from here
        tx.send(Msg { meta, file }).await?;
    }
    Ok(())
}
