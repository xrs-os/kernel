pub mod executor;
mod file;
pub mod process;
pub mod signal;
pub mod thread;
mod tid;

pub use process::*;

use self::thread::thread_future;

pub fn init() {
    tid::init();
    executor::init();
    let init_proc = process::create_init_proc();
    executor::spawn(thread_future(init_proc.main_thread.clone()));
}
