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

        // clone the OwnedFd from the Arc for use in tokio stream
        let cloned_fd = msg.fd.try_clone().expect("can't clone file");
        let std_file = std::fs::File::from(cloned_fd);
        let tokio_file = tokio::fs::File::from_std(std_file);
        let mut stream = FramedRead::new(tokio_file, BytesCodec::new())
            .map_ok(|b| b.freeze());

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
