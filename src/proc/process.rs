use super::{
    executor, file,
    signal::{self, SigAction, SignalFlags, SignalSet, Signo},
    thread::Thread,
    tid::{self, RawThreadId},
};
use crate::{
    arch::memory::kernel_segments,
    config,
    fs::{
        rootfs::{self, root_fs},
        util::read_all,
        DirEntry, Inode, Path,
    },
    mm::Mem,
    spinlock::{MutexIrq, RwLockIrq},
};
use alloc::{collections::BTreeMap, string::String, sync::Arc, vec::Vec};
use core::{mem, ptr::null};
use mm::{
    arch::page::PageParam as PageParamA,
    memory::{MapType, Segment},
    page::{flush::FlushAllGuard, PageParam as _},
    Addr, Result as MemoryResult, VirtualAddress,
};
use xmas_elf::{header, program, ElfFile};

#[derive(Debug)]
pub enum Error {
    ThreadIdNotEnough,
    MemoryErr(mm::Error),
    ElfErr(&'static str),
}

pub type Result<T> = core::result::Result<T, Error>;

pub struct Proc {
    id: tid::RawThreadId,
    pub main_thread: Arc<Thread>,
    pub group_leader: RwLockIrq<Option<Arc<Proc>>>,
    pub parent: RwLockIrq<Option<Arc<Proc>>>,
    pub children: RwLockIrq<BTreeMap<tid::RawThreadId, Arc<Proc>>>,
    pub threads: RwLockIrq<BTreeMap<tid::RawThreadId, Arc<Thread>>>,
    cmd: String,
    // Current working directory
    pub cwd: RwLockIrq<DirEntry>,
    pub open_files: OpenFiles,
    pub memory: RwLockIrq<Mem>,
    signal: MutexIrq<Signal>,
}

impl Proc {
    pub fn new<S: Into<String>>(
        cmd: S,
        cwd: DirEntry,
        init: bool,
        main_thread: Arc<Thread>,
    ) -> Result<Arc<Self>> {
        let mut signal = Signal::new();
        if init {
            signal.flags |= SignalFlags::UNKILLABLE;
        }

        let mut memory = crate::mm::new_memory().map_err(Error::MemoryErr)?;
        memory.set_asid(*main_thread.id() as usize);

        let mut threads = BTreeMap::new();
        threads.insert(*main_thread.id(), main_thread.clone());

        Ok(Arc::new(Self {
            id: *main_thread.id(),
            main_thread,
            group_leader: RwLockIrq::new(None),
            parent: RwLockIrq::new(None),
            children: RwLockIrq::new(BTreeMap::new()),
            threads: RwLockIrq::new(threads),
            cmd: cmd.into(),
            cwd: RwLockIrq::new(cwd),
            open_files: OpenFiles::new(),
            memory: RwLockIrq::new(memory),
            signal: MutexIrq::new(signal),
        }))
    }

    pub async fn from_elf(
        cmd: impl Into<String>,
        cwd: DirEntry,
        init: bool,
        file: Inode,
        args: Vec<String>,
        envs: Vec<String>,
    ) -> Result<Arc<Self>> {
        // Create main thread
        let tid = tid::alloc().ok_or(Error::ThreadIdNotEnough)?;
        let cmd: String = cmd.into();
        let main_thread = Arc::new(Thread::new(tid, cmd.clone()));

        let proc = Self::new(cmd, cwd, init, main_thread.clone())?;
        {
            let mut proc_mem = proc.memory.write();
            Self::map_kernel_segments(&mut proc_mem);
            proc_mem.activate();
        }
        unsafe { main_thread.init(proc.clone()).map_err(Error::MemoryErr)? };
        proc.load_user_program(file, args, envs).await?;
        Ok(proc)
    }

    fn map_kernel_segments(mem: &mut Mem) {
        for segment in kernel_segments() {
            mem.add_kernel_segment(segment).unwrap().ignore();
        }
    }

    pub async fn load_user_program(
        &self,
        prog: Inode,
        args: Vec<String>,
        envs: Vec<String>,
    ) -> Result<FlushAllGuard<PageParamA>> {
        let bytes = read_all(prog).await.map_err(|_fs_err| {
            // TODO: trace log _fs_err
            Error::ElfErr("Failed to read elf file.")
        })?;

        let elf = ElfFile::new(&bytes).map_err(Error::ElfErr)?;

        // Check ELF type
        match elf.header.pt2.type_().as_type() {
            header::Type::Executable => {}
            header::Type::SharedObject => {}
            _ => return Err(Error::ElfErr("ELF is not executable or shared object")),
        }

        // Check ELF arch
        match elf.header.pt2.machine().as_machine() {
            #[cfg(target_arch = "x86_64")]
            header::Machine::X86_64 => {}
            #[cfg(target_arch = "aarch64")]
            header::Machine::AArch64 => {}
            #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
            header::Machine::RISC_V => {}
            _ => return Err(Error::ElfErr("invalid ELF arch")),
        }

        let mut mem = self.memory.write();
        for ph in elf.program_iter() {
            if ph.get_type() != Ok(program::Type::Load) {
                continue;
            }
            let start = VirtualAddress(ph.virtual_addr() as usize);
            let size = ph.mem_size() as usize;
            let data: &[u8] =
                if let program::SegmentData::Undefined(data) = ph.get_data(&elf).unwrap() {
                    data
                } else {
                    return Err(Error::ElfErr("unsupported elf format"));
                };

            let mut flags = 0;
            if ph.flags().is_read() {
                flags |= PageParamA::FLAG_PTE_READABLE;
            }
            if ph.flags().is_write() {
                flags |= PageParamA::FLAG_PTE_WRITEABLE;
            }
            if ph.flags().is_execute() {
                flags |= PageParamA::FLAG_PTE_EXECUTABLE;
            }
            mem.add_user_segment(
                Segment {
                    addr_range: start..(start.add(size)),
                    flags: PageParamA::flag_set_user(flags),
                    map_type: MapType::Framed,
                },
                data,
            )
            .map_err(Error::MemoryErr)?
            .ignore();
        }
        let proc_init_info = ProcInitInfo {
            args,
            envs,
            auxval: Auxval::from_elf(&elf),
        };
        self.main_thread.reset_context(&proc_init_info);
        Ok(FlushAllGuard::new(Some(self.asid())))
    }

    pub fn fork(&self, asid: usize, main_thread: Arc<Thread>) -> MemoryResult<Self> {
        Ok(Self {
            id: *main_thread.id(),
            main_thread,
            group_leader: RwLockIrq::new(self.group_leader.read().clone()),
            parent: RwLockIrq::new(None),
            children: RwLockIrq::new(BTreeMap::new()),
            threads: RwLockIrq::new(BTreeMap::new()),
            cmd: self.cmd.clone(),
            cwd: RwLockIrq::new(self.cwd.read().clone()),
            open_files: self.open_files.clone(),
            memory: RwLockIrq::new(self.memory.read().borrow_memory(asid)?),
            signal: MutexIrq::new(self.signal.lock().fork()),
        })
    }

    pub fn is_init(&self) -> bool {
        self.id == 1
    }

    pub fn signal(&self) -> &MutexIrq<Signal> {
        &self.signal
    }

    pub fn id(&self) -> &tid::RawThreadId {
        &self.id
    }

    pub fn exit(&self, _status: isize) {
        self.threads
            .read()
            .iter()
            .filter(|(_, t)| !t.is_main_thread())
            .for_each(|(_, t)| {
                t.exit(0);
                t.waker().wake();
            });
        // TODO: Handling sub-processes
    }

    fn asid(&self) -> usize {
        *self.id() as usize
    }
}

pub struct Signal {
    actions: [SigAction; signal::NSIG as usize],
    /// `shared_pending` holds the signals sent to the process group
    pub shared_pending: signal::Pending,
    /// Blocked signals set
    pub blocked: SigBlocked,
    /// Current thread group signal load-balancing target
    /// A signal sent to a process group requires a thread in the process to handle it.
    /// For load balancing purposes,
    /// `current_target` stores the current thread that is handling the signal,
    /// ensure that the threads processing the signal are as different as possible each time.
    pub current_target: Option<RawThreadId>,
    pub flags: SignalFlags,
}

pub struct SigBlocked {
    pub blocked: SignalSet,
    pub real_blocked: SignalSet,
}

impl Signal {
    pub fn new() -> Self {
        Self {
            actions: array_init::array_init(|_| Default::default()),
            shared_pending: signal::Pending::new(),
            blocked: SigBlocked {
                blocked: SignalSet::empty(),
                real_blocked: SignalSet::empty(),
            },
            current_target: None,
            flags: SignalFlags::empty(),
        }
    }

    pub fn fork(&self) -> Self {
        Self {
            actions: self.actions.clone(),
            shared_pending: signal::Pending::new(),
            blocked: SigBlocked {
                blocked: SignalSet::empty(),
                real_blocked: SignalSet::empty(),
            },
            current_target: None,
            flags: SignalFlags::empty(),
        }
    }

    pub fn action(&self, sig: &Signo) -> &SigAction {
        unsafe { self.actions.get_unchecked(sig.to_primitive() as usize - 1) }
    }

    pub fn action_mut(&mut self, sig: &Signo) -> &mut SigAction {
        unsafe {
            self.actions
                .get_unchecked_mut(sig.to_primitive() as usize - 1)
        }
    }

    pub fn replace_action(&mut self, sig: &Signo, sa: SigAction) -> SigAction {
        unsafe {
            mem::replace(
                self.actions.get_unchecked_mut(sig.to_primitive() as usize),
                sa,
            )
        }
    }
}

pub struct OpenFiles(RwLockIrq<OpenFileInner>);

impl Clone for OpenFiles {
    fn clone(&self) -> Self {
        Self(RwLockIrq::new(self.0.read().clone()))
    }
}

#[derive(Clone)]
struct OpenFileInner {
    max_fd: usize,
    next_fd: usize,
    files: Vec<Option<file::Descriptor>>,
}

impl OpenFileInner {
    fn add_file(&mut self, file: file::Descriptor) -> Option<usize> {
        self.insert_file(self.next_fd, file)
    }

    fn add_file_min(&mut self, file: file::Descriptor, min: usize) -> Option<usize> {
        let fd_num = if min <= self.next_fd {
            self.next_fd
        } else {
            self.files
                .iter()
                .skip(min)
                .position(Option::is_none)
                .unwrap_or(self.files.len())
        };

        self.insert_file(fd_num, file)
    }

    fn insert_file(&mut self, fd_num: usize, file: file::Descriptor) -> Option<usize> {
        if fd_num < config::PROC_MAX_OPEN_FILES {
            if fd_num >= self.files.len() {
                self.files.resize(fd_num + 1, None);
            }

            let slot = unsafe { self.files.get_unchecked_mut(fd_num) };

            if slot.is_none() {
                slot.replace(file);

                if fd_num == self.next_fd {
                    self.next_fd = self
                        .files
                        .iter()
                        .skip(self.next_fd + 1)
                        .position(Option::is_none)
                        .unwrap_or(self.files.len())
                }

                if fd_num > self.max_fd {
                    self.max_fd = fd_num;
                }

                return Some(fd_num);
            }
        }
        None
    }

    fn remove_file(&mut self, fd_num: usize) -> Option<file::Descriptor> {
        let removed_file = self.files.get_mut(fd_num).and_then(|f| f.take());
        if removed_file.is_some() {
            if fd_num == self.max_fd {
                let max_fd = self.files.iter().rposition(Option::is_some).unwrap_or(0);
                self.files.truncate(max_fd + 1);
                self.files.shrink_to_fit();
                self.max_fd = max_fd;
            }

            if fd_num < self.next_fd {
                self.next_fd = fd_num
            }
        }

        removed_file
    }
}

impl OpenFiles {
    fn new() -> Self {
        Self(RwLockIrq::new(OpenFileInner {
            max_fd: 0,
            next_fd: 0,
            files: Vec::new(),
        }))
    }

    /// Get a file
    pub fn get_file(&self, fd_num: usize) -> Option<file::Descriptor> {
        self.0.read().files.get(fd_num).and_then(|fd| fd.clone())
    }

    /// Add a file to the lowest available slot.
    /// Return the file descriptor number or None if no slot was found
    pub fn add_file(&self, file: file::Descriptor) -> Option<usize> {
        self.0.write().add_file(file)
    }

    /// Add a file to the lowest available slot greater than or equal to min.
    /// Return the file descriptor number or None if no slot was found
    pub fn add_file_min(&self, file: file::Descriptor, min: usize) -> Option<usize> {
        self.0.write().add_file_min(file, min)
    }

    /// Insert a file with a specific fd number. This is used by dup2
    /// Return the file descriptor number or None if the slot was not empty, or fd_num was invalid
    pub fn insert_file(&self, fd_num: usize, file: file::Descriptor) -> Option<usize> {
        self.0.write().insert_file(fd_num, file)
    }

    /// Remove a file
    pub fn remove_file(&self, fd_num: usize) -> Option<file::Descriptor> {
        self.0.write().remove_file(fd_num)
    }
}

pub fn create_init_proc() -> Arc<Proc> {
    // TODO trace error
    let init_inode = executor::block_on(rootfs::find_inode(Path::from_bytes("/init".as_bytes())))
        .expect("Failed to load init proc")
        .expect("init proc not exist. path: '/init'");

    // TODO trace error
    executor::block_on(async {
        Proc::from_elf(
            "/init",
            root_fs().root().await,
            true,
            init_inode,
            Vec::new(),
            Vec::new(),
        )
        .await
    })
    .expect("Field to create init proc")
}

pub struct ProcInitInfo {
    pub args: Vec<String>,
    pub envs: Vec<String>,
    pub auxval: Auxval,
}

impl ProcInitInfo {
    pub fn push_to_stack(&self, sp: VirtualAddress) -> VirtualAddress {
        fn push_slice<T: Copy>(mut sp: usize, slice: &[T]) -> usize {
            sp -= slice.len() * mem::size_of::<T>();
            sp -= sp % mem::align_of::<T>();
            unsafe { core::slice::from_raw_parts_mut(sp as *mut T, slice.len()) }
                .copy_from_slice(slice);
            sp
        }
        fn push_str(mut sp: usize, s: &str) -> usize {
            sp = push_slice(sp, &['\0']);
            push_slice(sp, s.as_bytes())
        }
        let mut sp = sp.inner();
        let arg_ptrs = self
            .args
            .iter()
            .map(|arg| {
                sp = push_str(sp, arg);
                sp
            })
            .collect::<Vec<_>>();

        let env_ptrs = self
            .envs
            .iter()
            .map(|env| {
                sp = push_str(sp, env);
                sp
            })
            .collect::<Vec<_>>();

        // auxiliary vector entries
        sp = push_slice(sp, &[null::<u8>(), null::<u8>()]);
        self.auxval.as_abi_array().iter().for_each(|item| {
            sp = push_slice(sp, item);
        });

        // envionment pointers
        sp = push_slice(sp, &[null::<u8>()]);
        sp = push_slice(sp, env_ptrs.as_slice());
        // argv pointers
        sp = push_slice(sp, &[null::<u8>()]);
        sp = push_slice(sp, arg_ptrs.as_slice());
        // argc
        sp = push_slice(sp, &[arg_ptrs.len()]);
        VirtualAddress(sp)
    }
}

pub struct Auxval {
    pub at_entry: u64,
    pub at_phdr: u64,
    pub at_phent: u16,
    pub at_phnum: u16,
}

impl Auxval {
    const AT_PHDR: u64 = 3;
    const AT_PHENT: u64 = 4;
    const AT_PHNUM: u64 = 5;
    const AT_PAGESZ: u64 = 6;
    const AT_ENTRY: u64 = 9;

    fn from_elf(elf: &ElfFile) -> Self {
        let phdr = if let Some(phdr) = elf
            .program_iter()
            .find(|ph| ph.get_type() == Ok(program::Type::Phdr))
        {
            // if phdr exists in program header, use it
            Some(phdr.virtual_addr())
        } else if let Some(elf_addr) = elf
            .program_iter()
            .find(|ph| ph.get_type() == Ok(program::Type::Load) && ph.offset() == 0)
        {
            // otherwise, check if elf is loaded from the beginning, then phdr can be inferred.
            Some(elf_addr.virtual_addr() + elf.header.pt2.ph_offset())
        } else {
            None
        };
        Self {
            at_entry: elf.header.pt2.entry_point(),
            at_phdr: phdr.unwrap_or_default(),
            at_phent: elf.header.pt2.ph_entry_size(),
            at_phnum: elf.header.pt2.ph_count(),
        }
    }

    fn as_abi_array(&self) -> [[u64; 2]; 5] {
        return [
            [Self::AT_PHDR, self.at_phdr],
            [Self::AT_PHENT, self.at_phent as u64],
            [Self::AT_PHNUM, self.at_phnum as u64],
            [Self::AT_PAGESZ, PageParamA::PAGE_SIZE as u64],
            [Self::AT_ENTRY, self.at_entry],
        ];
    }
}
