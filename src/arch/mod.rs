// #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub mod riscv;
// #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub use self::riscv::*;

// #[cfg(target_arch = "x86_64")]
// pub mod x86_64;
// #[cfg(target_arch = "x86_64")]
// pub use self::x86_64::*;

// #[cfg(target_arch = "aarch64")]
// pub mod aarch64;
// #[cfg(target_arch = "aarch64")]
// pub use self::aarch64::*;
