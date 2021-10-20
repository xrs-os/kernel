use crate::{BlkId, InodeId};

/// The inode id of the root directory
pub const NAIVE_FS_ROOT_INO: InodeId = 2;

/// Number of direct blocks in inode
pub const INODE_DIRECT_BLK_COUNT: usize = 12;

pub const SUPER_BLK_OFFSET: u32 = 0;

pub const BLK_BITMAP_BLK_ID: BlkId = 1;
pub const INODE_BITMAP_BLK_ID: BlkId = BLK_BITMAP_BLK_ID + 1;
pub const INODE_TABLE_BLK_ID: BlkId = INODE_BITMAP_BLK_ID + 1;
