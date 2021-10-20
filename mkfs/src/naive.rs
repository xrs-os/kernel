#![feature(generic_associated_types)]
#![feature(io_error_more)]
#![feature(is_symlink)]

#[macro_use]
extern crate log;

use std::{
    any::Any,
    fs::Metadata,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::SystemTime,
};

use clap::{AppSettings, Clap};
use mkfs::IODisk;
use naive_fs::{BlkSize, DiskResult};
use tokio::fs::{File as TokioFile, OpenOptions as TokioOpenOptions};
use uuid::Uuid;

type NaiveFs = naive_fs::NaiveFs<spin::Mutex<()>, NaiveFsDisk>;
type Inode = naive_fs::inode::Inode<spin::RwLock<()>, spin::Mutex<()>, NaiveFsDisk>;

#[derive(Clap, Debug)]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    /// Place the output into <FILE>
    #[clap(name = "FILE", short = 'o', long = "output")]
    output: String,
    /// Set disk space (MB)
    #[clap(long, default_value = "128")]
    disk_space: usize,
    /// Copy the initial files to the root of the target filesystem.
    /// init_files_path support glob style pattern.
    #[clap(long)]
    init_files_path: Option<String>,
    /// Set block size (KB)
    #[clap(long, default_value = "4")]
    block_size: u8,
    #[clap(long)]
    volume_uuid: Option<String>,
    #[clap(long)]
    volume_name: Option<String>,
}

struct NaiveOpts {
    output: PathBuf,
    init_files: Vec<PathBuf>,
    disk_space: u32,
    block_size: u32,
    volume_uuid: [u8; 16],
    volume_name: [u8; 16],
}

fn parse_opts() -> core::result::Result<NaiveOpts, String> {
    let opts: Opts = Opts::parse();
    let disk_space_bytes = opts.disk_space as u32 * 1024 * 1024;
    let block_size = opts.block_size as u32 * 1024;
    let volume_uuid = match opts.volume_uuid {
        Some(volume_uuid) => match Uuid::parse_str(&volume_uuid) {
            Err(e) => {
                return Err(format!(
                    "Invalid volume_uuid {}. error: {:?}",
                    volume_uuid, e
                ));
            }
            Ok(volume_uuid) => volume_uuid,
        },
        None => Uuid::new_v4(),
    };

    let mut volume_name = [0_u8; 16];
    let output = Path::new(&opts.output);

    let volume_name_bytes = opts
        .volume_name
        .as_ref()
        .map(|name| name.as_bytes())
        .unwrap_or_else(|| {
            output
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap()
                .as_bytes()
        });
    (&mut volume_name[..volume_name_bytes.len()]).copy_from_slice(volume_name_bytes);

    let glob_paths = opts
        .init_files_path
        .map(|p| glob::glob(&p))
        .transpose()
        .map_err(|_| "Failed to read glob pattern".to_owned())?;

    let init_files = glob_paths
        .map(|p| p.into_iter().collect::<Result<Vec<_>, _>>())
        .transpose()
        .map_err(|e| format!("Failed to load glob pattern. {:?}", e))?
        .unwrap_or_else(|| Vec::new());

    Ok(NaiveOpts {
        output: output.to_owned(),
        init_files,
        disk_space: disk_space_bytes,
        block_size,
        volume_uuid: volume_uuid.as_bytes().clone(),
        volume_name,
    })
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::init();

    let naive_opts = match parse_opts() {
        Ok(x) => x,
        Err(err) => {
            error!("{}", err);
            return;
        }
    };
    let file = match TokioOpenOptions::new()
        .write(true)
        .read(true)
        .create(true)
        .open(naive_opts.output)
        .await
    {
        Err(e) => {
            error!("Field to create file. error: {:?}", e);
            return;
        }
        Ok(file) => file,
    };

    let disk = NaiveFsDisk {
        inner: IODisk::new(file),
        capacity: naive_opts.disk_space,
    };
    let naivefs = Arc::new(NaiveFs::create_blank(
        disk,
        BlkSize::new(naive_opts.block_size),
        naive_opts.volume_uuid,
        naive_opts.volume_name,
    ));

    let now_unix_timestamp = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(n) => n.as_secs() as u32,
        Err(_) => {
            error!("SystemTime before UNIX EPOCH!");
            return;
        }
    };

    let root_inode = match naivefs
        .create_root::<spin::RwLock<()>>(now_unix_timestamp)
        .await
    {
        Err(e) => {
            error!("Failed to create root directory. error: {:?}", e);
            return;
        }
        Ok(root_inode) => root_inode,
    };

    if !naive_opts.init_files.is_empty() {
        if let Err(e) = copy_file(
            &naivefs,
            &naive_opts.init_files,
            root_inode,
            now_unix_timestamp,
        )
        .await
        {
            error!("Failed to copy file. error: {:?}", e);
        }
    } else {
        if let Err(e) = root_inode.sync().await {
            error!("Failed to sync root inode. error: {:?}", e);
        }
    }
}

fn copy_file<'a>(
    naivefs: &'a Arc<NaiveFs>,
    files: &'a Vec<PathBuf>,
    parent: Inode,
    now_unix_timestamp: u32,
) -> BoxFuture<'a, std::io::Result<()>> {
    async fn create_inode(
        naivefs: &Arc<NaiveFs>,
        now_unix_timestamp: u32,
        filetype: naive_fs::inode::Mode,
        metadata: &Metadata,
    ) -> std::io::Result<Inode> {
        let perm_usr = if metadata.permissions().readonly() {
            naive_fs::inode::Mode::PERM_RX_USR
        } else {
            naive_fs::inode::Mode::PERM_RWX_USR
        };
        naivefs
            .create_inode(
                filetype
                    | perm_usr
                    | naive_fs::inode::Mode::PERM_RX_GRP
                    | naive_fs::inode::Mode::PERM_RX_OTH,
                0,
                0,
                now_unix_timestamp,
            )
            .await
            .map_err(naive_fs_err_to_stdio_err)
    }

    Box::pin(async move {
        for file in files {
            let attr = tokio::fs::metadata(&file).await?;
            let filename = file
                .file_name()
                .unwrap()
                .to_string_lossy()
                .as_bytes()
                .into();
            if attr.is_dir() {
                let mut read_dir = tokio::fs::read_dir(&file).await?;
                let mut children = Vec::new();
                while let Some(direntry) = read_dir.next_entry().await? {
                    children.push(direntry.path());
                }

                let dir = create_inode(
                    naivefs,
                    now_unix_timestamp,
                    naive_fs::inode::Mode::TY_DIR,
                    &attr,
                )
                .await?;
                dir.append_dot(parent.inode_id)
                    .await
                    .map_err(naive_fs_err_to_stdio_err)?;
                parent
                    .append(dir.inode_id, filename, naive_fs::dir::FileType::Dir)
                    .await
                    .map_err(naive_fs_err_to_stdio_err)?;
                if children.is_empty() {
                    dir.sync().await.map_err(naive_fs_err_to_stdio_err)?;
                } else {
                    copy_file(naivefs, &children, dir, now_unix_timestamp).await?;
                }
            } else if attr.is_file() {
                let file_inode = create_inode(
                    naivefs,
                    now_unix_timestamp,
                    naive_fs::inode::Mode::TY_REG,
                    &attr,
                )
                .await?;
                file_inode
                    .write_at(0, &tokio::fs::read(file).await?)
                    .await
                    .map_err(naive_fs_err_to_stdio_err)?;
                parent
                    .append(
                        file_inode.inode_id,
                        filename,
                        naive_fs::dir::FileType::RegFile,
                    )
                    .await
                    .map_err(naive_fs_err_to_stdio_err)?;
                file_inode.sync().await.map_err(naive_fs_err_to_stdio_err)?;
            } else if attr.is_symlink() {
                let symlink_inode = create_inode(
                    naivefs,
                    now_unix_timestamp,
                    naive_fs::inode::Mode::TY_LNK,
                    &attr,
                )
                .await?;
                symlink_inode
                    .write_at(
                        0,
                        &tokio::fs::read_link(file)
                            .await?
                            .to_string_lossy()
                            .as_bytes(),
                    )
                    .await
                    .map_err(naive_fs_err_to_stdio_err)?;
                parent
                    .append(
                        symlink_inode.inode_id,
                        filename,
                        naive_fs::dir::FileType::Symlink,
                    )
                    .await
                    .map_err(naive_fs_err_to_stdio_err)?;
                symlink_inode
                    .sync()
                    .await
                    .map_err(naive_fs_err_to_stdio_err)?;
            }
        }

        parent.sync().await.map_err(naive_fs_err_to_stdio_err)?;
        Ok(())
    })
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

struct NaiveFsDisk {
    inner: IODisk<TokioFile>,
    capacity: u32,
}

impl naive_fs::Disk for NaiveFsDisk {
    type ReadAtFut<'a> = BoxFuture<'a, DiskResult<u32>>;
    type WriteAtFut<'a> = BoxFuture<'a, DiskResult<u32>>;
    type SyncFut<'a> = BoxFuture<'a, DiskResult<()>>;

    fn read_at<'a>(&'a self, offset: u32, buf: &'a mut [u8]) -> Self::ReadAtFut<'a> {
        Box::pin(async move {
            self.inner
                .read_at(offset as u64, buf)
                .await
                .map(|len| len as u32)
                .map_err(|e| Box::new(e) as Box<dyn Any + Send>)
        })
    }

    fn write_at<'a>(&'a self, offset: u32, buf: &'a [u8]) -> Self::WriteAtFut<'a> {
        Box::pin(async move {
            self.inner
                .write_at(offset as u64, buf)
                .await
                .map(|len| len as u32)
                .map_err(|e| Box::new(e) as Box<dyn Any + Send>)
        })
    }

    fn sync<'a>(&'a self) -> Self::SyncFut<'a> {
        Box::pin(async move { self.inner.sync().await.map_err(|_| todo!()) })
    }

    fn capacity(&self) -> u32 {
        self.capacity
    }
}

fn naive_fs_err_to_stdio_err(nfe: naive_fs::Error) -> std::io::Error {
    match nfe {
        naive_fs::Error::NoSpace => std::io::ErrorKind::StorageFull.into(),
        naive_fs::Error::NotDir => std::io::ErrorKind::NotADirectory.into(),
        naive_fs::Error::ReadOnly => std::io::ErrorKind::ReadOnlyFilesystem.into(),
        naive_fs::Error::InvalidDirEntryName(den) => std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Invalid direntry name: {}", den.into_string()),
        ),
        _ => std::io::ErrorKind::Other.into(),
    }
}
