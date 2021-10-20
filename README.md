# XRS-OS (🚧WIP)
Xrs is a POSIX-subset operating system kernel written in Rust.

## Current project team members
- [0x5459](https://github.com/0x5459) core developer (he/him)
- [0x5457](https://github.com/0x5457) core developer (he/him)

## Build

### Environment

* [Rust](https://www.rust-lang.org) toolchain
* [QEMU](https://www.qemu.org) >= 6.1.0
* [Python 3](https://www.python.org/)

### How to run

```bash
git clone https://github.com/xrs-os/kernel.git
cd kernel
python3 bootstrap.py qemu
```


## Inspired by
- [rCore](https://github.com/rcore-os/rCore) Rust version of THU uCore OS, teaching operating system. Linux compatible.
- [Writing an OS in Rust](https://os.phil-opp.com/) An os blog. This blog series creates a small operating system in the Rust programming language.
- [Redox](https://gitlab.redox-os.org/redox-os/redox) Redox is an operating system written in Rust, a language with focus on safety and high performance. Redox, following the microkernel design, aims to be secure, usable, and free.
- [xv6-riscv](https://github.com/mit-pdos/xv6-riscv) xv6 is a teaching operating system developed for MIT's operating systems course. It is a re-implementation of Dennis Ritchie's and Ken Thompson's Unix
Version 6 (v6).  xv6 loosely follows the structure and style of v6,
but is implemented for a modern RISC-V multiprocessor using ANSI C.

- [Linux](https://github.com/torvalds/linux)
The linux kernel.

## TODO list
- Architecture
  - [x] RISC-V
  - [ ] x86/64
  - [ ] aarch64
- Memory management
  - [x] Kernel heap allocator
    - [x] Linkedlist
    - [ ] Slab
  - [x] Virtual address mapping
  - [x] Frame allocator
    - [x] Bump
    - [ ] Buddy
  - [ ] Virtual memory for copy-on-write

- Task management
  - [x] Executor
    - [x] FIFO
    - [ ] HRRN
  - [x] Async Task

- Filesystem
  - [x] NaiveFS (Like ext2, but simpler)
  - [ ] Fat
  - [ ] Ext2/3/4
  - [x] MountFS (Mountable FS wrapper)
  - [x] CacheFS (LRU Cacheable FS wrapper)

- Driver
  - [x] [Async virtio driver](https://github.com/xrs-os/virtio-drivers) (based on [rcore/virtio-drivers](https://github.com/rcore-os/virtio-drivers))
