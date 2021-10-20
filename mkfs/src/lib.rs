use std::io::{self, SeekFrom};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt};

pub type SleepMutex<T> = sleeplock::Mutex<spin::Mutex<()>, T>;

pub struct IODisk<IO> {
    io: SleepMutex<IO>,
}

impl<IO> IODisk<IO> {
    pub fn new(io: IO) -> Self {
        Self {
            io: SleepMutex::new(io),
        }
    }
}

impl<IO: AsyncRead + AsyncSeek + Unpin> IODisk<IO> {
    pub async fn read_at(&self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        let mut io = self.io.lock().await;
        io.seek(SeekFrom::Start(offset)).await?;
        io.read(buf).await
    }
}

impl<IO: AsyncWrite + AsyncSeek + Unpin> IODisk<IO> {
    pub async fn write_at(&self, offset: u64, buf: &[u8]) -> io::Result<usize> {
        let mut io = self.io.lock().await;
        io.seek(SeekFrom::Start(offset)).await?;
        io.write(buf).await
    }

    pub async fn sync(&self) -> io::Result<()> {
        self.io.lock().await.flush().await
    }
}
