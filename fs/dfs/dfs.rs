#![allow(dead_code)]
use crate::block::{BlockDevice, BlockError, SECTOR_SIZE};
use core::cell::UnsafeCell;

const MAGIC: &[u8; 8] = b"DREAMDFS";
const VERSION: u32 = 1;
const JOURNAL_MAGIC: &[u8; 8] = b"DFSTXN01";
const JOURNAL_SECTORS: u64 = 17;
const INODE_COUNT: usize = 256;
const MAX_EXTENTS: usize = 14;
const MAX_PATH: usize = 255;
const TRANSACTION_SECTORS: usize = (JOURNAL_SECTORS - 1) as usize;
const NODE_SIZE: usize = SECTOR_SIZE;
const NODE_PATH_END: usize = MAX_PATH;
const NODE_KIND_OFFSET: usize = 256;
const NODE_EXTENTS_OFFSET: usize = 280;
const NODE_CHECKSUM_OFFSET: usize = 508;
const NODE_FILE: u8 = 1;
const NODE_DIRECTORY: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DfsError {
    Block(BlockError),
    InvalidFilesystem,
    CorruptMetadata,
    InvalidPath,
    NotFound,
    NotDirectory,
    IsDirectory,
    AlreadyExists,
    DirectoryNotEmpty,
    NoSpace,
    JournalFull,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metadata {
    pub size: u64,
    pub mode: u16,
    pub uid: u32,
    pub gid: u32,
    pub is_directory: bool,
}

#[derive(Clone, Copy)]
struct Extent {
    start_lba: u64,
    logical_start: u32,
    sector_count: u32,
}

impl Extent {
    const EMPTY: Self = Self {
        start_lba: 0,
        logical_start: 0,
        sector_count: 0,
    };
}

#[derive(Clone, Copy)]
struct Node {
    path: [u8; MAX_PATH + 1],
    path_length: usize,
    kind: u8,
    size: u64,
    mode: u16,
    uid: u32,
    gid: u32,
    extent_count: usize,
    extents: [Extent; MAX_EXTENTS],
}

impl Node {
    const EMPTY: Self = Self {
        path: [0; MAX_PATH + 1],
        path_length: 0,
        kind: 0,
        size: 0,
        mode: 0,
        uid: 0,
        gid: 0,
        extent_count: 0,
        extents: [Extent::EMPTY; MAX_EXTENTS],
    };

    fn metadata(self) -> Metadata {
        Metadata {
            size: self.size,
            mode: self.mode,
            uid: self.uid,
            gid: self.gid,
            is_directory: self.kind == NODE_DIRECTORY,
        }
    }

    fn path(&self) -> &str {
        core::str::from_utf8(&self.path[..self.path_length]).unwrap_or("")
    }
}

#[derive(Clone, Copy)]
struct Layout {
    start_lba: u64,
    sectors: u64,
    journal_lba: u64,
    inode_lba: u64,
    bitmap_lba: u64,
    bitmap_sectors: u64,
    data_lba: u64,
}

#[derive(Clone, Copy)]
pub struct Dfs {
    layout: Layout,
}

struct MountedDfs(UnsafeCell<Option<Dfs>>);
unsafe impl Sync for MountedDfs {}
static MOUNTED_DFS: MountedDfs = MountedDfs(UnsafeCell::new(None));

const MAX_EXECUTABLE_SIZE: usize = 1024 * 1024;
struct ExecutableBuffer(UnsafeCell<[u8; MAX_EXECUTABLE_SIZE]>);
unsafe impl Sync for ExecutableBuffer {}
static EXECUTABLE_BUFFER: ExecutableBuffer =
    ExecutableBuffer(UnsafeCell::new([0; MAX_EXECUTABLE_SIZE]));

struct Transaction {
    count: usize,
    targets: [u64; TRANSACTION_SECTORS],
    data: [[u8; SECTOR_SIZE]; TRANSACTION_SECTORS],
}

impl Transaction {
    fn new() -> Self {
        Self {
            count: 0,
            targets: [0; TRANSACTION_SECTORS],
            data: [[0; SECTOR_SIZE]; TRANSACTION_SECTORS],
        }
    }

    fn add(&mut self, lba: u64, sector: &[u8; SECTOR_SIZE]) -> Result<(), DfsError> {
        if let Some(index) = self.targets[..self.count]
            .iter()
            .position(|target| *target == lba)
        {
            self.data[index].copy_from_slice(sector);
            return Ok(());
        }
        if self.count == TRANSACTION_SECTORS {
            return Err(DfsError::JournalFull);
        }
        self.targets[self.count] = lba;
        self.data[self.count].copy_from_slice(sector);
        self.count += 1;
        Ok(())
    }
}

impl Dfs {
    pub fn format(
        device: &impl BlockDevice,
        start_lba: u64,
        sectors: u64,
    ) -> Result<Self, DfsError> {
        let layout = make_layout(start_lba, sectors)?;
        let zero = [0u8; SECTOR_SIZE];
        for lba in start_lba..layout.data_lba {
            device.write_sector(lba, &zero).map_err(DfsError::Block)?;
        }

        let reserved = layout.data_lba - start_lba;
        // Mark every sector before data_lba allocated, including all metadata.
        for bitmap_index in 0..layout.bitmap_sectors {
            let mut bitmap = [0u8; SECTOR_SIZE];
            let bitmap_bit_start = bitmap_index * (SECTOR_SIZE * 8) as u64;
            let bitmap_bit_end = (bitmap_bit_start + (SECTOR_SIZE * 8) as u64).min(reserved);
            for bit in bitmap_bit_start..bitmap_bit_end {
                set_bit(&mut bitmap, (bit - bitmap_bit_start) as usize, true);
            }
            device
                .write_sector(layout.bitmap_lba + bitmap_index, &bitmap)
                .map_err(DfsError::Block)?;
        }
        write_superblock(device, layout)?;
        device.flush().map_err(DfsError::Block)?;

        let fs = Self { layout };
        let root = Node {
            path: [0; MAX_PATH + 1],
            path_length: 0,
            kind: NODE_DIRECTORY,
            size: 0,
            mode: 0o755,
            uid: 0,
            gid: 0,
            extent_count: 0,
            extents: [Extent::EMPTY; MAX_EXTENTS],
        };
        fs.store_node(device, 0, &root)?;
        Ok(fs)
    }

    pub fn mount(
        device: &impl BlockDevice,
        start_lba: u64,
        sectors: u64,
    ) -> Result<Self, DfsError> {
        let layout = read_superblock(device, start_lba, sectors)?;
        let fs = Self { layout };
        fs.replay_journal(device)?;
        let root = fs.read_node(device, 0)?;
        if root.kind != NODE_DIRECTORY || root.path_length != 0 {
            return Err(DfsError::CorruptMetadata);
        }
        Ok(fs)
    }

    pub fn seed_from_ustar(
        &self,
        device: &impl BlockDevice,
        archive: &[u8],
    ) -> Result<(), DfsError> {
        let mut offset = 0usize;
        loop {
            let end = offset
                .checked_add(SECTOR_SIZE)
                .filter(|end| *end <= archive.len())
                .ok_or(DfsError::InvalidFilesystem)?;
            let header = &archive[offset..end];
            if header.iter().all(|byte| *byte == 0) {
                return Ok(());
            }
            let name = tar_field(&header[..100]);
            let prefix = tar_field(&header[345..500]);
            let size = tar_octal(&header[124..136]).ok_or(DfsError::InvalidFilesystem)?;
            let data_start = end;
            let data_end = data_start
                .checked_add(size)
                .filter(|end| *end <= archive.len())
                .ok_or(DfsError::InvalidFilesystem)?;
            let path_len = prefix
                .len()
                .checked_add(usize::from(!prefix.is_empty()))
                .and_then(|length| length.checked_add(name.len()))
                .ok_or(DfsError::InvalidPath)?;
            if path_len > MAX_PATH {
                return Err(DfsError::InvalidPath);
            }
            let mut path_bytes = [0u8; MAX_PATH + 1];
            let mut written = 0;
            if !prefix.is_empty() {
                path_bytes[..prefix.len()].copy_from_slice(prefix);
                written += prefix.len();
                path_bytes[written] = b'/';
                written += 1;
            }
            path_bytes[written..written + name.len()].copy_from_slice(name);
            written += name.len();
            while written != 0 && path_bytes[written - 1] == b'/' {
                written -= 1;
            }
            let path =
                core::str::from_utf8(&path_bytes[..written]).map_err(|_| DfsError::InvalidPath)?;
            if !path.is_empty() && path != "." {
                let mode = tar_octal(&header[100..108]).unwrap_or(0o644) as u16;
                let uid = tar_octal(&header[108..116]).unwrap_or(0) as u32;
                let gid = tar_octal(&header[116..124]).unwrap_or(0) as u32;
                match header[156] {
                    b'5' => match self.create_directory(device, path, mode, uid, gid) {
                        Ok(()) | Err(DfsError::AlreadyExists) => {}
                        Err(error) => return Err(error),
                    },
                    0 | b'0' => {
                        self.write_file(
                            device,
                            path,
                            0,
                            &archive[data_start..data_end],
                            true,
                            mode,
                            uid,
                            gid,
                        )?;
                    }
                    _ => {}
                }
            }
            let padded = size
                .checked_add(SECTOR_SIZE - 1)
                .ok_or(DfsError::InvalidFilesystem)?
                & !(SECTOR_SIZE - 1);
            offset = data_start
                .checked_add(padded)
                .ok_or(DfsError::InvalidFilesystem)?;
        }
    }

    pub fn read_file(
        &self,
        device: &impl BlockDevice,
        path: &str,
        offset: u64,
        output: &mut [u8],
    ) -> Result<usize, DfsError> {
        let path = canonical_path(path)?;
        let node = self.find_node(device, path)?.ok_or(DfsError::NotFound)?;
        if node.kind != NODE_FILE {
            return Err(DfsError::IsDirectory);
        }
        if offset >= node.size {
            return Ok(0);
        }
        let length = output
            .len()
            .min((node.size - offset).min(usize::MAX as u64) as usize);
        let mut copied = 0;
        let mut sector = [0u8; SECTOR_SIZE];
        while copied < length {
            let position = offset + copied as u64;
            let logical = (position / SECTOR_SIZE as u64) as u32;
            let within = (position % SECTOR_SIZE as u64) as usize;
            let extent = node.extents[..node.extent_count]
                .iter()
                .find(|extent| {
                    logical >= extent.logical_start
                        && logical < extent.logical_start + extent.sector_count
                })
                .ok_or(DfsError::CorruptMetadata)?;
            let physical = extent.start_lba + (logical - extent.logical_start) as u64;
            device
                .read_sector(physical, &mut sector)
                .map_err(DfsError::Block)?;
            let count = (SECTOR_SIZE - within).min(length - copied);
            output[copied..copied + count].copy_from_slice(&sector[within..within + count]);
            copied += count;
        }
        Ok(length)
    }

    pub fn write_file(
        &self,
        device: &impl BlockDevice,
        path: &str,
        offset: u64,
        input: &[u8],
        create: bool,
        mode: u16,
        uid: u32,
        gid: u32,
    ) -> Result<usize, DfsError> {
        let path = canonical_path(path)?;
        if path.is_empty() {
            return Err(DfsError::IsDirectory);
        }
        let index = match self.find_node(device, path)? {
            Some(node) if node.kind == NODE_DIRECTORY => return Err(DfsError::IsDirectory),
            Some(_) => self
                .find_index(device, path)?
                .ok_or(DfsError::CorruptMetadata)?,
            None if create => self.create_node(device, path, NODE_FILE, mode, uid, gid)?,
            None => return Err(DfsError::NotFound),
        };
        let mut node = self.read_node(device, index)?;
        if offset > node.size {
            return Err(DfsError::InvalidPath);
        }
        let end = offset
            .checked_add(input.len() as u64)
            .ok_or(DfsError::NoSpace)?;
        let required_sectors = end.div_ceil(SECTOR_SIZE as u64);
        let mut allocated_sectors: u64 = node.extents[..node.extent_count]
            .iter()
            .map(|extent| extent.sector_count as u64)
            .sum();
        while allocated_sectors < required_sectors {
            let request = required_sectors - allocated_sectors;
            let (start_lba, count) = self.find_free_run(device, request)?;
            self.add_extent(&mut node, start_lba, allocated_sectors as u32, count as u32)?;
            self.reserve_run(device, start_lba, count, index, &node)?;
            allocated_sectors += count;
        }

        let mut consumed = 0;
        let mut sector = [0u8; SECTOR_SIZE];
        while consumed < input.len() {
            let position = offset + consumed as u64;
            let logical = (position / SECTOR_SIZE as u64) as u32;
            let within = (position % SECTOR_SIZE as u64) as usize;
            let count = (SECTOR_SIZE - within).min(input.len() - consumed);
            let physical = self
                .extent_for(&node, logical)
                .ok_or(DfsError::CorruptMetadata)?;
            if within == 0 && count == SECTOR_SIZE {
                sector.copy_from_slice(&input[consumed..consumed + count]);
            } else {
                device
                    .read_sector(physical, &mut sector)
                    .map_err(DfsError::Block)?;
                sector[within..within + count].copy_from_slice(&input[consumed..consumed + count]);
            }
            device
                .write_sector(physical, &sector)
                .map_err(DfsError::Block)?;
            consumed += count;
        }
        node.size = node.size.max(end);
        device.flush().map_err(DfsError::Block)?;
        self.store_node(device, index, &node)?;
        Ok(consumed)
    }

    pub fn create_directory(
        &self,
        device: &impl BlockDevice,
        path: &str,
        mode: u16,
        uid: u32,
        gid: u32,
    ) -> Result<(), DfsError> {
        let path = canonical_path(path)?;
        if path.is_empty() {
            return Err(DfsError::AlreadyExists);
        }
        self.create_node(device, path, NODE_DIRECTORY, mode, uid, gid)?;
        Ok(())
    }

    pub fn remove(
        &self,
        device: &impl BlockDevice,
        path: &str,
        recursive: bool,
    ) -> Result<(), DfsError> {
        self.remove_at_depth(device, path, recursive, 0)
    }

    fn remove_at_depth(
        &self,
        device: &impl BlockDevice,
        path: &str,
        recursive: bool,
        depth: usize,
    ) -> Result<(), DfsError> {
        if depth > 32 {
            return Err(DfsError::InvalidPath);
        }
        let path = canonical_path(path)?;
        if path.is_empty() {
            return Err(DfsError::InvalidPath);
        }
        let index = self.find_index(device, path)?.ok_or(DfsError::NotFound)?;
        let node = self.read_node(device, index)?;
        if node.kind == NODE_DIRECTORY {
            loop {
                let mut child_path = [0u8; MAX_PATH + 1];
                let mut child_path_length = 0;
                let mut found_child = false;
                for child_index in 0..INODE_COUNT {
                    let child = self.read_node(device, child_index)?;
                    if child.kind == 0 {
                        continue;
                    }
                    if immediate_child(child.path(), path).is_some() {
                        if !recursive {
                            return Err(DfsError::DirectoryNotEmpty);
                        }
                        child_path[..child.path_length]
                            .copy_from_slice(&child.path[..child.path_length]);
                        child_path_length = child.path_length;
                        found_child = true;
                        break;
                    }
                }
                if !found_child {
                    break;
                }
                let child_path = core::str::from_utf8(&child_path[..child_path_length])
                    .map_err(|_| DfsError::CorruptMetadata)?;
                self.remove_at_depth(device, child_path, true, depth + 1)?;
            }
        }
        self.store_node(device, index, &Node::EMPTY)?;
        for extent in &node.extents[..node.extent_count] {
            self.release_run(device, extent.start_lba, extent.sector_count as u64)?;
        }
        Ok(())
    }

    pub fn truncate(&self, device: &impl BlockDevice, path: &str) -> Result<(), DfsError> {
        let path = canonical_path(path)?;
        let index = self.find_index(device, path)?.ok_or(DfsError::NotFound)?;
        let mut node = self.read_node(device, index)?;
        if node.kind != NODE_FILE {
            return Err(DfsError::IsDirectory);
        }
        let extents = node.extents;
        let extent_count = node.extent_count;
        node.size = 0;
        node.extents = [Extent::EMPTY; MAX_EXTENTS];
        node.extent_count = 0;
        self.store_node(device, index, &node)?;
        for extent in &extents[..extent_count] {
            self.release_run(device, extent.start_lba, extent.sector_count as u64)?;
        }
        Ok(())
    }

    pub fn stat(&self, device: &impl BlockDevice, path: &str) -> Result<Metadata, DfsError> {
        let path = canonical_path(path)?;
        Ok(self
            .find_node(device, path)?
            .ok_or(DfsError::NotFound)?
            .metadata())
    }

    pub fn read_directory(
        &self,
        device: &impl BlockDevice,
        path: &str,
        include_hidden: bool,
        output: &mut [u8],
    ) -> Result<usize, DfsError> {
        let path = canonical_path(path)?;
        let directory = self.find_node(device, path)?.ok_or(DfsError::NotFound)?;
        if directory.kind != NODE_DIRECTORY {
            return Err(DfsError::NotDirectory);
        }
        let mut count = 0usize;
        for index in 0..INODE_COUNT {
            let node = self.read_node(device, index)?;
            if node.kind == 0 || node.path_length == 0 {
                continue;
            }
            let Some(name) = immediate_child(node.path(), path) else {
                continue;
            };
            if !include_hidden && name.starts_with('.') {
                continue;
            }
            let required = count
                .checked_add(name.len())
                .and_then(|length| length.checked_add(1))
                .ok_or(DfsError::NoSpace)?;
            if required > output.len() {
                return Err(DfsError::NoSpace);
            }
            output[count..count + name.len()].copy_from_slice(name.as_bytes());
            output[count + name.len()] = b'\n';
            count = required;
        }
        Ok(count)
    }

    fn create_node(
        &self,
        device: &impl BlockDevice,
        path: &str,
        kind: u8,
        mode: u16,
        uid: u32,
        gid: u32,
    ) -> Result<usize, DfsError> {
        if self.find_node(device, path)?.is_some() {
            return Err(DfsError::AlreadyExists);
        }
        let parent = parent_path(path);
        if self
            .find_node(device, parent)?
            .is_none_or(|node| node.kind != NODE_DIRECTORY)
        {
            return Err(DfsError::NotDirectory);
        }
        let index = (1..INODE_COUNT)
            .find(|index| {
                self.read_node(device, *index)
                    .is_ok_and(|node| node.kind == 0)
            })
            .ok_or(DfsError::NoSpace)?;
        let mut node = Node::EMPTY;
        node.path[..path.len()].copy_from_slice(path.as_bytes());
        node.path_length = path.len();
        node.kind = kind;
        node.mode = mode;
        node.uid = uid;
        node.gid = gid;
        self.store_node(device, index, &node)?;
        Ok(index)
    }

    fn find_node(&self, device: &impl BlockDevice, path: &str) -> Result<Option<Node>, DfsError> {
        let index = self.find_index(device, path)?;
        index.map(|index| self.read_node(device, index)).transpose()
    }

    fn find_index(&self, device: &impl BlockDevice, path: &str) -> Result<Option<usize>, DfsError> {
        for index in 0..INODE_COUNT {
            let node = self.read_node(device, index)?;
            if node.kind != 0 && node.path() == path {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }

    fn read_node(&self, device: &impl BlockDevice, index: usize) -> Result<Node, DfsError> {
        if index >= INODE_COUNT {
            return Err(DfsError::CorruptMetadata);
        }
        let mut bytes = [0u8; NODE_SIZE];
        device
            .read_sector(self.layout.inode_lba + index as u64, &mut bytes)
            .map_err(DfsError::Block)?;
        decode_node(&bytes)
    }

    fn store_node(
        &self,
        device: &impl BlockDevice,
        index: usize,
        node: &Node,
    ) -> Result<(), DfsError> {
        let bytes = encode_node(node)?;
        self.commit(device, &[(self.layout.inode_lba + index as u64, &bytes)])
    }

    fn extent_for(&self, node: &Node, logical: u32) -> Option<u64> {
        node.extents[..node.extent_count]
            .iter()
            .find(|extent| {
                extent
                    .logical_start
                    .checked_add(extent.sector_count)
                    .is_some_and(|end| logical >= extent.logical_start && logical < end)
            })
            .map(|extent| extent.start_lba + (logical - extent.logical_start) as u64)
    }

    fn find_free_run(
        &self,
        device: &impl BlockDevice,
        requested: u64,
    ) -> Result<(u64, u64), DfsError> {
        let max_bitmap_sectors = (TRANSACTION_SECTORS - 1) as u64;
        let mut run_start = 0;
        let mut run_length = 0;
        let mut cached_lba = u64::MAX;
        let mut first_bitmap_lba = u64::MAX;
        let mut bitmap = [0u8; SECTOR_SIZE];
        for lba in self.layout.data_lba..self.layout.start_lba + self.layout.sectors {
            let (bitmap_lba, bit) = self.bitmap_location(lba)?;
            if bitmap_lba != cached_lba {
                device
                    .read_sector(bitmap_lba, &mut bitmap)
                    .map_err(DfsError::Block)?;
                cached_lba = bitmap_lba;
            }
            if bitmap[bit / 8] & (1 << (bit % 8)) == 0 {
                if run_length == 0 {
                    run_start = lba;
                    first_bitmap_lba = bitmap_lba;
                } else if bitmap_lba - first_bitmap_lba >= max_bitmap_sectors {
                    return Ok((run_start, run_length));
                }
                run_length += 1;
                if run_length == requested {
                    return Ok((run_start, run_length));
                }
            } else if run_length != 0 {
                return Ok((run_start, run_length));
            }
        }
        if run_length != 0 {
            Ok((run_start, run_length))
        } else {
            Err(DfsError::NoSpace)
        }
    }

    fn add_extent(
        &self,
        node: &mut Node,
        start_lba: u64,
        logical_start: u32,
        sector_count: u32,
    ) -> Result<(), DfsError> {
        if node.extent_count != 0 {
            let last = &mut node.extents[node.extent_count - 1];
            if last.start_lba + last.sector_count as u64 == start_lba
                && last.logical_start + last.sector_count == logical_start
            {
                last.sector_count = last
                    .sector_count
                    .checked_add(sector_count)
                    .ok_or(DfsError::NoSpace)?;
                return Ok(());
            }
        }
        if node.extent_count == MAX_EXTENTS {
            return Err(DfsError::NoSpace);
        }
        node.extents[node.extent_count] = Extent {
            start_lba,
            logical_start,
            sector_count,
        };
        node.extent_count += 1;
        Ok(())
    }

    fn reserve_run(
        &self,
        device: &impl BlockDevice,
        start_lba: u64,
        count: u64,
        index: usize,
        node: &Node,
    ) -> Result<(), DfsError> {
        let mut transaction = Transaction::new();
        let mut current_bitmap_lba = u64::MAX;
        let mut bitmap = [0u8; SECTOR_SIZE];
        for lba in start_lba..start_lba + count {
            let (bitmap_lba, bit) = self.bitmap_location(lba)?;
            if bitmap_lba != current_bitmap_lba {
                if current_bitmap_lba != u64::MAX {
                    transaction.add(current_bitmap_lba, &bitmap)?;
                }
                device
                    .read_sector(bitmap_lba, &mut bitmap)
                    .map_err(DfsError::Block)?;
                current_bitmap_lba = bitmap_lba;
            }
            set_bit(&mut bitmap, bit, true);
        }
        transaction.add(current_bitmap_lba, &bitmap)?;
        let inode = encode_node(node)?;
        transaction.add(self.layout.inode_lba + index as u64, &inode)?;
        self.commit_transaction(device, &transaction)
    }

    fn release_run(
        &self,
        device: &impl BlockDevice,
        start_lba: u64,
        count: u64,
    ) -> Result<(), DfsError> {
        let mut transaction = Transaction::new();
        let mut current_bitmap_lba = u64::MAX;
        let mut bitmap = [0u8; SECTOR_SIZE];
        for lba in start_lba..start_lba + count {
            let (bitmap_lba, bit) = self.bitmap_location(lba)?;
            if bitmap_lba != current_bitmap_lba {
                if current_bitmap_lba != u64::MAX {
                    transaction.add(current_bitmap_lba, &bitmap)?;
                }
                if transaction.count == TRANSACTION_SECTORS {
                    self.commit_transaction(device, &transaction)?;
                    transaction = Transaction::new();
                }
                device
                    .read_sector(bitmap_lba, &mut bitmap)
                    .map_err(DfsError::Block)?;
                current_bitmap_lba = bitmap_lba;
            }
            set_bit(&mut bitmap, bit, false);
        }
        if current_bitmap_lba != u64::MAX {
            transaction.add(current_bitmap_lba, &bitmap)?;
        }
        self.commit_transaction(device, &transaction)
    }

    fn bitmap_location(&self, lba: u64) -> Result<(u64, usize), DfsError> {
        let relative = lba
            .checked_sub(self.layout.start_lba)
            .filter(|relative| *relative < self.layout.sectors)
            .ok_or(DfsError::InvalidFilesystem)?;
        let bit = relative as usize;
        let sector = bit / (SECTOR_SIZE * 8);
        if sector as u64 >= self.layout.bitmap_sectors {
            return Err(DfsError::CorruptMetadata);
        }
        Ok((
            self.layout.bitmap_lba + sector as u64,
            bit % (SECTOR_SIZE * 8),
        ))
    }

    fn commit(
        &self,
        device: &impl BlockDevice,
        updates: &[(u64, &[u8; SECTOR_SIZE])],
    ) -> Result<(), DfsError> {
        if updates.is_empty() {
            return Ok(());
        }
        if updates.len() > TRANSACTION_SECTORS {
            return Err(DfsError::JournalFull);
        }
        let journal_end = self.layout.journal_lba + JOURNAL_SECTORS;
        let mut transaction = Transaction::new();
        for (lba, data) in updates {
            if *lba < self.layout.start_lba
                || *lba >= self.layout.start_lba + self.layout.sectors
                || (*lba >= self.layout.journal_lba && *lba < journal_end)
            {
                return Err(DfsError::InvalidFilesystem);
            }
            transaction.add(*lba, data)?;
        }
        self.commit_transaction(device, &transaction)
    }

    fn commit_transaction(
        &self,
        device: &impl BlockDevice,
        transaction: &Transaction,
    ) -> Result<(), DfsError> {
        // Flush journal payload before publishing the commit record for replay.
        let mut header = [0u8; SECTOR_SIZE];
        header[..8].copy_from_slice(JOURNAL_MAGIC);
        header[8] = 1;
        header[9] = transaction.count as u8;
        for index in 0..transaction.count {
            header[16 + index * 8..24 + index * 8]
                .copy_from_slice(&transaction.targets[index].to_le_bytes());
            device
                .write_sector(
                    self.layout.journal_lba + 1 + index as u64,
                    &transaction.data[index],
                )
                .map_err(DfsError::Block)?;
        }
        header[10..14].copy_from_slice(&transaction_checksum(&transaction).to_le_bytes());
        device.flush().map_err(DfsError::Block)?;
        device
            .write_sector(self.layout.journal_lba, &header)
            .map_err(DfsError::Block)?;
        device.flush().map_err(DfsError::Block)?;
        for index in 0..transaction.count {
            device
                .write_sector(transaction.targets[index], &transaction.data[index])
                .map_err(DfsError::Block)?;
        }
        device.flush().map_err(DfsError::Block)?;
        device
            .write_sector(self.layout.journal_lba, &[0; SECTOR_SIZE])
            .map_err(DfsError::Block)?;
        device.flush().map_err(DfsError::Block)?;
        Ok(())
    }

    fn replay_journal(&self, device: &impl BlockDevice) -> Result<(), DfsError> {
        // Keep a committed record until all target sectors have been flushed.
        let mut header = [0u8; SECTOR_SIZE];
        device
            .read_sector(self.layout.journal_lba, &mut header)
            .map_err(DfsError::Block)?;
        if header.iter().all(|byte| *byte == 0) {
            return Ok(());
        }
        if &header[..8] != JOURNAL_MAGIC {
            return Err(DfsError::CorruptMetadata);
        }
        if header[8] == 0 {
            device
                .write_sector(self.layout.journal_lba, &[0; SECTOR_SIZE])
                .map_err(DfsError::Block)?;
            return device.flush().map_err(DfsError::Block);
        }
        let count = header[9] as usize;
        if header[8] != 1 || count == 0 || count > TRANSACTION_SECTORS {
            return Err(DfsError::CorruptMetadata);
        }
        let mut transaction = Transaction::new();
        transaction.count = count;
        let journal_end = self.layout.journal_lba + JOURNAL_SECTORS;
        for index in 0..count {
            let target =
                u64::from_le_bytes(header[16 + index * 8..24 + index * 8].try_into().unwrap());
            if target < self.layout.start_lba
                || target >= self.layout.start_lba + self.layout.sectors
                || (target >= self.layout.journal_lba && target < journal_end)
            {
                return Err(DfsError::CorruptMetadata);
            }
            transaction.targets[index] = target;
            device
                .read_sector(
                    self.layout.journal_lba + 1 + index as u64,
                    &mut transaction.data[index],
                )
                .map_err(DfsError::Block)?;
        }
        let expected = u32::from_le_bytes(header[10..14].try_into().unwrap());
        if transaction_checksum(&transaction) != expected {
            return Err(DfsError::CorruptMetadata);
        }
        for index in 0..count {
            device
                .write_sector(transaction.targets[index], &transaction.data[index])
                .map_err(DfsError::Block)?;
        }
        device.flush().map_err(DfsError::Block)?;
        device
            .write_sector(self.layout.journal_lba, &[0; SECTOR_SIZE])
            .map_err(DfsError::Block)?;
        device.flush().map_err(DfsError::Block)?;
        Ok(())
    }
}

pub fn mount_primary() -> Result<Option<Dfs>, DfsError> {
    let device = crate::ata::PrimaryMaster;
    let table = match crate::gpt::Gpt::read_primary(&device) {
        Ok(table) => table,
        Err(crate::gpt::GptError::InvalidSignature) => return Ok(None),
        Err(_) => return Err(DfsError::InvalidFilesystem),
    };
    let partition = match table.find_partition(&device, &crate::gpt::DFS_PARTITION_TYPE_GUID) {
        Ok(partition) => partition,
        Err(crate::gpt::GptError::PartitionNotFound) => return Ok(None),
        Err(_) => return Err(DfsError::InvalidFilesystem),
    };
    let fs = Dfs::mount(
        &device,
        partition.first_lba,
        partition.last_lba - partition.first_lba + 1,
    )?;
    unsafe {
        *MOUNTED_DFS.0.get() = Some(fs);
    }
    Ok(Some(fs))
}

pub fn is_mounted() -> bool {
    unsafe { (*MOUNTED_DFS.0.get()).is_some() }
}

/// Returns the mounted DFS instance, if a persistent root is active.
pub fn mounted() -> Option<Dfs> {
    unsafe { *MOUNTED_DFS.0.get() }
}

pub fn read_static(path: &str) -> Option<&'static [u8]> {
    let fs = unsafe { (*MOUNTED_DFS.0.get())? };
    let device = crate::ata::PrimaryMaster;
    let metadata = fs.stat(&device, path).ok()?;
    if metadata.is_directory || metadata.size > MAX_EXECUTABLE_SIZE as u64 {
        return None;
    }
    let buffer = unsafe { &mut *EXECUTABLE_BUFFER.0.get() };
    let length = fs
        .read_file(&device, path, 0, &mut buffer[..metadata.size as usize])
        .ok()?;
    Some(unsafe { core::slice::from_raw_parts(buffer.as_ptr(), length) })
}

pub fn read_at(path: &str, offset: usize, output: &mut [u8]) -> Result<usize, DfsError> {
    let fs = unsafe { (*MOUNTED_DFS.0.get()).ok_or(DfsError::InvalidFilesystem)? };
    fs.read_file(&crate::ata::PrimaryMaster, path, offset as u64, output)
}

pub fn file_len(path: &str) -> Result<usize, DfsError> {
    let metadata = stat(path)?;
    if metadata.is_directory {
        return Err(DfsError::IsDirectory);
    }
    usize::try_from(metadata.size).map_err(|_| DfsError::NoSpace)
}

pub fn write_at(path: &str, offset: usize, input: &[u8], create: bool) -> Result<usize, DfsError> {
    write_at_as(path, offset, input, create, 0o644, 0, 0)
}

pub fn write_at_as(
    path: &str,
    offset: usize,
    input: &[u8],
    create: bool,
    mode: u16,
    uid: u32,
    gid: u32,
) -> Result<usize, DfsError> {
    let fs = unsafe { (*MOUNTED_DFS.0.get()).ok_or(DfsError::InvalidFilesystem)? };
    fs.write_file(
        &crate::ata::PrimaryMaster,
        path,
        offset as u64,
        input,
        create,
        mode,
        uid,
        gid,
    )
}

pub fn write_file(path: &str, bytes: &[u8]) -> Result<(), DfsError> {
    write_file_as(path, bytes, 0o644, 0, 0)
}

pub fn write_file_as(
    path: &str,
    bytes: &[u8],
    mode: u16,
    uid: u32,
    gid: u32,
) -> Result<(), DfsError> {
    if stat(path).is_ok() {
        let fs = unsafe { (*MOUNTED_DFS.0.get()).ok_or(DfsError::InvalidFilesystem)? };
        fs.truncate(&crate::ata::PrimaryMaster, path)?;
    }
    write_at_as(path, 0, bytes, true, mode, uid, gid).map(|_| ())
}

pub fn create_file(path: &str) -> Result<(), DfsError> {
    write_at(path, 0, &[], true).map(|_| ())
}

pub fn create_directory_with_parents(path: &str, parents: bool) -> Result<(), DfsError> {
    create_directory_with_parents_as(path, parents, 0, 0)
}

pub fn create_directory_with_parents_as(
    path: &str,
    parents: bool,
    uid: u32,
    gid: u32,
) -> Result<(), DfsError> {
    let path = canonical_path(path)?;
    if !parents {
        return create_directory_as(path, 0o755, uid, gid);
    }
    let mut current = [0u8; MAX_PATH + 1];
    let mut length = 0;
    for component in path.split('/') {
        if length != 0 {
            current[length] = b'/';
            length += 1;
        }
        if length + component.len() > MAX_PATH {
            return Err(DfsError::InvalidPath);
        }
        current[length..length + component.len()].copy_from_slice(component.as_bytes());
        length += component.len();
        let current_path =
            core::str::from_utf8(&current[..length]).map_err(|_| DfsError::InvalidPath)?;
        match stat(current_path) {
            Ok(metadata) if metadata.is_directory => continue,
            Ok(_) => return Err(DfsError::AlreadyExists),
            Err(DfsError::NotFound) => {
                create_directory_as_with_mode(current_path, 0o755, uid, gid)?
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

pub fn create_directory(path: &str) -> Result<(), DfsError> {
    create_directory_as(path, 0o755, 0, 0)
}

pub fn create_directory_as(path: &str, mode: u16, uid: u32, gid: u32) -> Result<(), DfsError> {
    let fs = unsafe { (*MOUNTED_DFS.0.get()).ok_or(DfsError::InvalidFilesystem)? };
    fs.create_directory(&crate::ata::PrimaryMaster, path, mode, uid, gid)
}

fn create_directory_as_with_mode(
    path: &str,
    mode: u16,
    uid: u32,
    gid: u32,
) -> Result<(), DfsError> {
    create_directory_as(path, mode, uid, gid)
}

pub fn remove(path: &str, recursive: bool) -> Result<(), DfsError> {
    let fs = unsafe { (*MOUNTED_DFS.0.get()).ok_or(DfsError::InvalidFilesystem)? };
    fs.remove(&crate::ata::PrimaryMaster, path, recursive)
}

pub fn stat(path: &str) -> Result<Metadata, DfsError> {
    let fs = unsafe { (*MOUNTED_DFS.0.get()).ok_or(DfsError::InvalidFilesystem)? };
    fs.stat(&crate::ata::PrimaryMaster, path)
}

pub fn list_directory(
    path: &str,
    include_hidden: bool,
    output: &mut [u8],
) -> Result<usize, DfsError> {
    let fs = unsafe { (*MOUNTED_DFS.0.get()).ok_or(DfsError::InvalidFilesystem)? };
    fs.read_directory(&crate::ata::PrimaryMaster, path, include_hidden, output)
}

fn tar_field(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    &bytes[..end]
}

fn tar_octal(bytes: &[u8]) -> Option<usize> {
    let mut value = 0usize;
    let mut found_digit = false;
    for &byte in bytes {
        match byte {
            b'0'..=b'7' => {
                found_digit = true;
                value = value.checked_mul(8)?.checked_add((byte - b'0') as usize)?;
            }
            0 | b' ' if found_digit || value == 0 => {}
            _ => return None,
        }
    }
    Some(value)
}

fn make_layout(start_lba: u64, sectors: u64) -> Result<Layout, DfsError> {
    let bitmap_sectors = sectors
        .checked_add((SECTOR_SIZE * 8 - 1) as u64)
        .ok_or(DfsError::InvalidFilesystem)?
        / (SECTOR_SIZE * 8) as u64;
    let journal_lba = start_lba
        .checked_add(1)
        .ok_or(DfsError::InvalidFilesystem)?;
    let inode_lba = journal_lba
        .checked_add(JOURNAL_SECTORS)
        .ok_or(DfsError::InvalidFilesystem)?;
    let bitmap_lba = inode_lba
        .checked_add(INODE_COUNT as u64)
        .ok_or(DfsError::InvalidFilesystem)?;
    let data_lba = bitmap_lba
        .checked_add(bitmap_sectors)
        .ok_or(DfsError::InvalidFilesystem)?;
    if sectors <= data_lba - start_lba || start_lba.checked_add(sectors).is_none() {
        return Err(DfsError::InvalidFilesystem);
    }
    Ok(Layout {
        start_lba,
        sectors,
        journal_lba,
        inode_lba,
        bitmap_lba,
        bitmap_sectors,
        data_lba,
    })
}

fn write_superblock(device: &impl BlockDevice, layout: Layout) -> Result<(), DfsError> {
    let mut sector = [0u8; SECTOR_SIZE];
    sector[..8].copy_from_slice(MAGIC);
    sector[8..12].copy_from_slice(&VERSION.to_le_bytes());
    sector[16..24].copy_from_slice(&layout.sectors.to_le_bytes());
    sector[24..32].copy_from_slice(&JOURNAL_SECTORS.to_le_bytes());
    sector[32..40].copy_from_slice(&layout.journal_lba.to_le_bytes());
    sector[40..48].copy_from_slice(&layout.inode_lba.to_le_bytes());
    sector[48..52].copy_from_slice(&(INODE_COUNT as u32).to_le_bytes());
    sector[52..60].copy_from_slice(&layout.bitmap_lba.to_le_bytes());
    sector[60..68].copy_from_slice(&layout.bitmap_sectors.to_le_bytes());
    sector[68..76].copy_from_slice(&layout.data_lba.to_le_bytes());
    let checksum = crc32(&sector[..508]);
    sector[508..512].copy_from_slice(&checksum.to_le_bytes());
    device
        .write_sector(layout.start_lba, &sector)
        .map_err(DfsError::Block)
}

fn read_superblock(
    device: &impl BlockDevice,
    start_lba: u64,
    sectors: u64,
) -> Result<Layout, DfsError> {
    let mut sector = [0u8; SECTOR_SIZE];
    device
        .read_sector(start_lba, &mut sector)
        .map_err(DfsError::Block)?;
    let checksum = u32::from_le_bytes(sector[508..512].try_into().unwrap());
    if &sector[..8] != MAGIC
        || u32::from_le_bytes(sector[8..12].try_into().unwrap()) != VERSION
        || crc32(&sector[..508]) != checksum
    {
        return Err(DfsError::InvalidFilesystem);
    }
    let layout = make_layout(start_lba, sectors)?;
    if u64::from_le_bytes(sector[16..24].try_into().unwrap()) != layout.sectors
        || u64::from_le_bytes(sector[24..32].try_into().unwrap()) != JOURNAL_SECTORS
        || u64::from_le_bytes(sector[32..40].try_into().unwrap()) != layout.journal_lba
        || u64::from_le_bytes(sector[40..48].try_into().unwrap()) != layout.inode_lba
        || u32::from_le_bytes(sector[48..52].try_into().unwrap()) != INODE_COUNT as u32
        || u64::from_le_bytes(sector[52..60].try_into().unwrap()) != layout.bitmap_lba
        || u64::from_le_bytes(sector[60..68].try_into().unwrap()) != layout.bitmap_sectors
        || u64::from_le_bytes(sector[68..76].try_into().unwrap()) != layout.data_lba
    {
        return Err(DfsError::InvalidFilesystem);
    }
    Ok(layout)
}

fn encode_node(node: &Node) -> Result<[u8; NODE_SIZE], DfsError> {
    let mut sector = [0u8; NODE_SIZE];
    if node.kind == 0 {
        return Ok(sector);
    }
    if node.path_length > MAX_PATH || node.extent_count > MAX_EXTENTS {
        return Err(DfsError::CorruptMetadata);
    }
    sector[..node.path_length].copy_from_slice(&node.path[..node.path_length]);
    sector[NODE_KIND_OFFSET] = node.kind;
    sector[NODE_KIND_OFFSET + 1] = node.extent_count as u8;
    sector[258..260].copy_from_slice(&node.mode.to_le_bytes());
    sector[260..264].copy_from_slice(&node.uid.to_le_bytes());
    sector[264..268].copy_from_slice(&node.gid.to_le_bytes());
    sector[268..276].copy_from_slice(&node.size.to_le_bytes());
    for (index, extent) in node.extents[..node.extent_count].iter().enumerate() {
        let offset = NODE_EXTENTS_OFFSET + index * 16;
        sector[offset..offset + 8].copy_from_slice(&extent.start_lba.to_le_bytes());
        sector[offset + 8..offset + 12].copy_from_slice(&extent.logical_start.to_le_bytes());
        sector[offset + 12..offset + 16].copy_from_slice(&extent.sector_count.to_le_bytes());
    }
    let checksum = crc32(&sector[..NODE_CHECKSUM_OFFSET]);
    sector[NODE_CHECKSUM_OFFSET..].copy_from_slice(&checksum.to_le_bytes());
    Ok(sector)
}

fn decode_node(sector: &[u8; NODE_SIZE]) -> Result<Node, DfsError> {
    let kind = sector[NODE_KIND_OFFSET];
    if kind == 0 {
        return Ok(Node::EMPTY);
    }
    let expected = u32::from_le_bytes(sector[NODE_CHECKSUM_OFFSET..].try_into().unwrap());
    if crc32(&sector[..NODE_CHECKSUM_OFFSET]) != expected {
        return Err(DfsError::CorruptMetadata);
    }
    let path_length = sector[..NODE_PATH_END]
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(NODE_PATH_END);
    if !matches!(kind, NODE_FILE | NODE_DIRECTORY) || path_length > MAX_PATH {
        return Err(DfsError::CorruptMetadata);
    }
    let mut node = Node::EMPTY;
    node.path[..path_length].copy_from_slice(&sector[..path_length]);
    node.path_length = path_length;
    node.kind = kind;
    node.extent_count = sector[NODE_KIND_OFFSET + 1] as usize;
    if node.extent_count > MAX_EXTENTS || (kind == NODE_DIRECTORY && node.extent_count != 0) {
        return Err(DfsError::CorruptMetadata);
    }
    node.mode = u16::from_le_bytes(sector[258..260].try_into().unwrap());
    node.uid = u32::from_le_bytes(sector[260..264].try_into().unwrap());
    node.gid = u32::from_le_bytes(sector[264..268].try_into().unwrap());
    node.size = u64::from_le_bytes(sector[268..276].try_into().unwrap());
    for index in 0..node.extent_count {
        let offset = NODE_EXTENTS_OFFSET + index * 16;
        node.extents[index] = Extent {
            start_lba: u64::from_le_bytes(sector[offset..offset + 8].try_into().unwrap()),
            logical_start: u32::from_le_bytes(sector[offset + 8..offset + 12].try_into().unwrap()),
            sector_count: u32::from_le_bytes(sector[offset + 12..offset + 16].try_into().unwrap()),
        };
        if node.extents[index].sector_count == 0 {
            return Err(DfsError::CorruptMetadata);
        }
    }
    Ok(node)
}

fn transaction_checksum(transaction: &Transaction) -> u32 {
    let mut crc = !0u32;
    for index in 0..transaction.count {
        crc = crc32_update(crc, &transaction.targets[index].to_le_bytes());
        crc = crc32_update(crc, &transaction.data[index]);
    }
    !crc
}

fn crc32(bytes: &[u8]) -> u32 {
    !crc32_update(!0, bytes)
}

fn crc32_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    crc
}

fn set_bit(bitmap: &mut [u8; SECTOR_SIZE], bit: usize, value: bool) {
    if value {
        bitmap[bit / 8] |= 1 << (bit % 8);
    } else {
        bitmap[bit / 8] &= !(1 << (bit % 8));
    }
}

fn canonical_path(path: &str) -> Result<&str, DfsError> {
    let path = path.strip_prefix('/').unwrap_or(path);
    if path.len() > MAX_PATH
        || path
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return if path.is_empty() {
            Ok(path)
        } else {
            Err(DfsError::InvalidPath)
        };
    }
    Ok(path)
}

fn parent_path(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn immediate_child<'a>(path: &'a str, parent: &str) -> Option<&'a str> {
    let remainder = if parent.is_empty() {
        path
    } else {
        path.strip_prefix(parent)?.strip_prefix('/')?
    };
    if remainder.is_empty() || remainder.contains('/') {
        None
    } else {
        Some(remainder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;
    use std::vec::Vec;

    struct MemoryDisk(Vec<[u8; SECTOR_SIZE]>);

    impl MemoryDisk {
        fn new(sectors: usize) -> Self {
            Self(vec![[0; SECTOR_SIZE]; sectors])
        }
    }

    impl BlockDevice for MemoryDisk {
        fn sector_count(&self) -> u64 {
            self.0.len() as u64
        }

        fn read_sector(&self, lba: u64, buffer: &mut [u8; SECTOR_SIZE]) -> Result<(), BlockError> {
            let sector = self.0.get(lba as usize).ok_or(BlockError::OutOfBounds)?;
            buffer.copy_from_slice(sector);
            Ok(())
        }

        fn write_sector(&self, lba: u64, buffer: &[u8; SECTOR_SIZE]) -> Result<(), BlockError> {
            let sector = unsafe {
                (self.0.as_ptr() as *mut [u8; SECTOR_SIZE])
                    .add(lba as usize)
                    .as_mut()
            }
            .ok_or(BlockError::OutOfBounds)?;
            sector.copy_from_slice(buffer);
            Ok(())
        }

        fn flush(&self) -> Result<(), BlockError> {
            Ok(())
        }
    }

    #[test]
    fn persists_files_metadata_and_directories_after_remount() {
        let disk = MemoryDisk::new(8192);
        let fs = Dfs::format(&disk, 1, 8191).unwrap();
        fs.create_directory(&disk, "home", 0o750, 1000, 1000)
            .unwrap();
        fs.write_file(
            &disk,
            "home/.profile",
            0,
            b"persisted",
            true,
            0o640,
            1000,
            1000,
        )
        .unwrap();

        let mounted = Dfs::mount(&disk, 1, 8191).unwrap();
        let mut bytes = [0; 16];
        assert_eq!(
            mounted
                .read_file(&disk, "/home/.profile", 0, &mut bytes)
                .unwrap(),
            9
        );
        assert_eq!(&bytes[..9], b"persisted");
        assert_eq!(
            mounted.stat(&disk, "home/.profile").unwrap(),
            Metadata {
                size: 9,
                mode: 0o640,
                uid: 1000,
                gid: 1000,
                is_directory: false,
            }
        );
        let mut entries = [0; 64];
        assert_eq!(
            mounted
                .read_directory(&disk, "home", false, &mut entries)
                .unwrap(),
            0
        );
        assert_eq!(
            mounted
                .read_directory(&disk, "home", true, &mut entries)
                .unwrap(),
            9
        );
        assert_eq!(&entries[..9], b".profile\n");
    }

    #[test]
    fn replays_committed_journal_after_interrupted_metadata_write() {
        let disk = MemoryDisk::new(8192);
        let fs = Dfs::format(&disk, 1, 8191).unwrap();
        let mut node = Node::EMPTY;
        node.path[..4].copy_from_slice(b"lost");
        node.path_length = 4;
        node.kind = NODE_FILE;
        node.mode = 0o600;
        let node_sector = encode_node(&node).unwrap();
        let target = fs.layout.inode_lba + 1;
        let mut transaction = Transaction::new();
        transaction.add(target, &node_sector).unwrap();
        for index in 0..transaction.count {
            disk.write_sector(
                fs.layout.journal_lba + 1 + index as u64,
                &transaction.data[index],
            )
            .unwrap();
        }
        let mut header = [0u8; SECTOR_SIZE];
        header[..8].copy_from_slice(JOURNAL_MAGIC);
        header[8] = 1;
        header[9] = transaction.count as u8;
        header[16..24].copy_from_slice(&target.to_le_bytes());
        header[10..14].copy_from_slice(&transaction_checksum(&transaction).to_le_bytes());
        disk.write_sector(fs.layout.journal_lba, &header).unwrap();

        let mounted = Dfs::mount(&disk, 1, 8191).unwrap();
        assert_eq!(mounted.stat(&disk, "lost").unwrap().mode, 0o600);
        assert_eq!(disk.0[fs.layout.journal_lba as usize], [0; SECTOR_SIZE]);
    }

    #[test]
    fn rejects_superblock_and_metadata_checksum_corruption() {
        let disk = MemoryDisk::new(8192);
        let fs = Dfs::format(&disk, 1, 8191).unwrap();
        disk.write_sector(fs.layout.inode_lba + 1, &[1; SECTOR_SIZE])
            .unwrap();
        assert!(matches!(
            fs.read_node(&disk, 1),
            Err(DfsError::CorruptMetadata)
        ));
        disk.write_sector(fs.layout.start_lba, &[0; SECTOR_SIZE])
            .unwrap();
        assert!(matches!(
            Dfs::mount(&disk, 1, 8191),
            Err(DfsError::InvalidFilesystem)
        ));
    }
}
