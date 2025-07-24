use nix::sys::stat::{Mode, SFlag};
use serde::{Deserialize, Serialize};
use std::fs::Metadata;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

// metadata tracking

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub path: String,
    pub file_type: FileType,
    pub size: u64,
    pub permissions: u32,
    pub modified_time: u64,
    pub created_time: u64,
    pub is_executable: bool,
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileType {
    RegularFile,
    Directory,
    SymbolicLink,
    BlockDevice,
    CharacterDevice,
    Fifo,
    Socket,
    Unknown,
}

impl FileMetadata {
    pub fn new(path: &Path, metadata: &Metadata) -> anyhow::Result<Self> {
        let file_type = if metadata.is_file() {
            FileType::RegularFile
        } else if metadata.is_dir() {
            FileType::Directory
        } else if metadata.is_symlink() {
            FileType::SymbolicLink
        } else {
            let _mode = Mode::from_bits_truncate(metadata.mode());
            let file_type_bits = SFlag::from_bits_truncate(metadata.mode());
            if file_type_bits.contains(SFlag::S_IFBLK) {
                FileType::BlockDevice
            } else if file_type_bits.contains(SFlag::S_IFCHR) {
                FileType::CharacterDevice
            } else if file_type_bits.contains(SFlag::S_IFIFO) {
                FileType::Fifo
            } else if file_type_bits.contains(SFlag::S_IFSOCK) {
                FileType::Socket
            } else {
                FileType::Unknown
            }
        };

        let modified_time = metadata.modified()?.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

        let created_time = metadata
            .created()
            .unwrap_or(SystemTime::UNIX_EPOCH)
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let permissions = metadata.permissions().mode();
        let is_executable =
            Mode::from_bits_truncate(permissions).intersects(Mode::S_IXUSR | Mode::S_IXGRP | Mode::S_IXOTH);
        let mime_type = detect_mime_type(path);

        Ok(Self {
            path: path.to_string_lossy().to_string(),
            file_type,
            size: metadata.len(),
            permissions,
            modified_time,
            created_time,
            is_executable,
            mime_type,
        })
    }
}


fn detect_mime_type(path: &Path) -> String {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("txt") => "text/plain".to_string(),
        Some("html") | Some("htm") => "text/html".to_string(),
        Some("css") => "text/css".to_string(),
        Some("js") => "application/javascript".to_string(),
        Some("json") => "application/json".to_string(),
        Some("png") => "image/png".to_string(),
        Some("jpg") | Some("jpeg") => "image/jpeg".to_string(),
        Some("gif") => "image/gif".to_string(),
        Some("pdf") => "application/pdf".to_string(),
        Some("zip") => "application/zip".to_string(),
        Some("rs") => "text/x-rust".to_string(),
        Some("py") => "text/x-python".to_string(),
        Some("c") => "text/x-c".to_string(),
        Some("cpp") | Some("cc") => "text/x-c++".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}
