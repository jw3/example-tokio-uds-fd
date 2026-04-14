use crate::Msg;
use futures_util::TryStreamExt;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
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

        let hasher = Arc::new(Mutex::new(Sha256::new()));
        let hasher_stream = Arc::clone(&hasher);

        let f = tokio::fs::File::from_std(msg.file);
        let mut stream = FramedRead::new(f, BytesCodec::new())
        .map_ok(|chunk| chunk.freeze())
        .inspect_ok(move |bytes| {
            // assuming the mutex cannot be poisoned here because there is no concurrent access
            let mut guard = hasher_stream.lock().unwrap_or_else(|e| e.into_inner());
            guard.update(bytes);
        });

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

        // drain the stream to ensure hash is calculated completely
        while let Ok(Some(_)) = stream.try_next().await {}

        let digest = hasher.lock().unwrap().clone().finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        println!("<hash>{hex}</hash>");

        println!("</consumer>");
    }
}
