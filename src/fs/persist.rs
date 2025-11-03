extern crate alloc;

use alloc::{string::String, vec::Vec, boxed::Box};
use alloc::vec;
use crate::alloc::string::ToString;
use core::sync::atomic::{AtomicU64, Ordering};
use crate::sys::UPTIME_TICKS;
use crate::fs::{dir::Directory, file::File, storage::ROOT_DIR};
use crate::block::ata::{read_lba28, write_lba28};

// On-disk layout: [MAGIC u32][LEN u32][DATA bytes][zero padding to sector]
const MAGIC: u32 = 0x4B_46_53_31; // 'KFS1'
const START_LBA: u32 = 2048; // leave room before

pub static LAST_SNAPSHOT_TICKS: AtomicU64 = AtomicU64::new(0);
pub static LAST_SNAPSHOT_BYTES: AtomicU64 = AtomicU64::new(0);

#[derive(serde::Serialize, serde::Deserialize)]
struct SFile { name: String, content: Vec<u8> }

#[derive(serde::Serialize, serde::Deserialize)]
struct SDir { name: String, files: Vec<SFile>, subdirs: Vec<SDir> }

fn to_serializable(dir: &Directory) -> SDir {
    let mut files: Vec<SFile> = Vec::new();
    for (_k, f) in dir.files.iter() {
        files.push(SFile { name: f.name.clone(), content: f.content.clone() });
    }
    let mut subdirs_vec: Vec<SDir> = Vec::new();
    for (_k, sd) in dir.subdirs.iter() {
        subdirs_vec.push(to_serializable(sd));
    }
    SDir { name: dir.name.to_string(), files, subdirs: subdirs_vec }
}

fn from_serializable(s: &SDir) -> Directory {
    let static_name: &'static str = Box::leak(s.name.clone().into_boxed_str());
    let mut d = Directory::new(static_name);
    for sf in s.files.iter() {
        let mut f = File::new(&sf.name);
        f.write(&sf.content);
        d.add_file(f);
    }
    for sd in s.subdirs.iter() {
        d.add_subdir(from_serializable(sd));
    }
    d
}

pub fn save_to_disk() -> Result<(), ()> {
    let root = ROOT_DIR.lock();
    let snapshot = to_serializable(&root);
    let data: Vec<u8> = postcard::to_allocvec(&snapshot).map_err(|_| ())?;

    let mut header = [0u8; 8];
    header[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    header[4..8].copy_from_slice(&(data.len() as u32).to_le_bytes());

    // Build full buffer with header + data, aligned to 512
    let total_len = 8 + data.len();
    let sectors_total = ((total_len + 511) / 512) as usize;
    let mut buf: Vec<u8> = Vec::with_capacity(sectors_total * 512);
    buf.extend_from_slice(&header);
    buf.extend_from_slice(&data);
    while buf.len() % 512 != 0 { buf.push(0); }

    // Write in up to 255-sector chunks
    let mut written_sectors = 0usize;
    let mut lba = START_LBA;
    while written_sectors < sectors_total {
        let remaining = sectors_total - written_sectors;
        let chunk_sectors = core::cmp::min(255, remaining) as u8;
        let start = written_sectors * 512;
        let end = start + (chunk_sectors as usize) * 512;
        write_lba28(lba, chunk_sectors, &buf[start..end])?;
        written_sectors += chunk_sectors as usize;
        lba += chunk_sectors as u32;
    }
    LAST_SNAPSHOT_TICKS.store(UPTIME_TICKS.load(Ordering::Relaxed), Ordering::Relaxed);
    LAST_SNAPSHOT_BYTES.store(total_len as u64, Ordering::Relaxed);
    Ok(())
}

pub fn load_from_disk() -> Result<(), ()> {
    // Read first sector
    let mut first: [u8; 512] = [0; 512];
    read_lba28(START_LBA, 1, &mut first)?;
    let magic = u32::from_le_bytes([first[0], first[1], first[2], first[3]]);
    if magic != MAGIC { return Err(()); }
    let len = u32::from_le_bytes([first[4], first[5], first[6], first[7]]) as usize;
    let total = 8 + len;
    let sectors_total = ((total + 511) / 512) as usize;
    let mut buf: Vec<u8> = vec![0u8; sectors_total * 512];

    // Read in up to 255-sector chunks
    let mut read_so_far = 0usize;
    let mut lba = START_LBA;
    while read_so_far < sectors_total {
        let remaining = sectors_total - read_so_far;
        let chunk_sectors = core::cmp::min(255, remaining) as u8;
        let start = read_so_far * 512;
        let end = start + (chunk_sectors as usize) * 512;
        read_lba28(lba, chunk_sectors, &mut buf[start..end])?;
        read_so_far += chunk_sectors as usize;
        lba += chunk_sectors as u32;
    }
    let payload = &buf[8..8+len];
    let snapshot: SDir = postcard::from_bytes(payload).map_err(|_| ())?;
    let restored = from_serializable(&snapshot);
    let mut root = ROOT_DIR.lock();
    *root = restored;
    LAST_SNAPSHOT_BYTES.store(total as u64, Ordering::Relaxed);
    Ok(())
}


