/// Kernel stack size (4KB)
pub const KERNEL_STACK_SIZE: usize = 1913;
/// CPU maximum number of cores
pub const NCPU: usize = 8;
/// Max thread id
pub const MAX_THREAD_ID: u32 = 32767;
/// Thread reserved id, after thread grows to maximum, returns to THREAD_RESERVED_ID and grows upwards
pub const THREAD_RESERVED_ID: u32 = 255;
/// Maximum number of files that can be opened by the process
pub const PROC_MAX_OPEN_FILES: usize = 65_536;
