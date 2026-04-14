use crate::Msg;
use futures_util::TryStreamExt;
use tokio::sync::mpsc::Receiver;
use tokio_util::codec::{BytesCodec, FramedRead};

pub async fn consume(mut rx: Receiver<Msg>) {
    while let Some(msg) = rx.recv().await {
        println!("<consumer id={}>", msg.id);
        println!("Received {:?} metadata (payload1):", msg.metadata.file_type);
        println!("\tPath: {}", msg.metadata.path);
        println!("\tType: {:?}", msg.metadata.file_type);
        println!("\tSize: {} bytes", msg.metadata.size);
        println!("\tPermissions: {:o}", msg.metadata.permissions);
        println!("\tMIME: {}", msg.metadata.mime_type);
        println!("\tExecutable: {}", msg.metadata.is_executable);
        println!("\tFile Size: {}", msg.metadata.size);

        let f = tokio::fs::File::from_std(msg.file);
        let mut stream = FramedRead::new(f, BytesCodec::new())
            .map_ok(|chunk| chunk.freeze());

        if let Some(chunk) = stream.try_next().await.unwrap() {
            if let Ok(contents) = std::str::from_utf8(&chunk) {
                if msg.metadata.size > 128 {
                    println!("\tpreview:\n{}", &contents[..128]);
                } else {
                    println!("\tcontent:\n{contents}");
                }
            }
        } else {
            println!("stream error {}", msg.id);
        }

        println!("</consumer>");
    }
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut s, "{b:02x}");
    }
    s
}
