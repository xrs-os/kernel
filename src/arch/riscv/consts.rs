use mm::PhysicalAddress;

// Start address of the user stack
pub const USER_STACK_OFFSET: usize = 0x3f_ffff_f000;
// User Stack Size (1MB)
pub const USER_STACK_SIZE: usize = 1024 * 1024;
// Memory end address
pub const MEMORY_END_ADDRESS: PhysicalAddress = PhysicalAddress(0x88000000);
