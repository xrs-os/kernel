const NCCS: usize = 19;
/// POSIX Termios
/// https://manpages.debian.org/bullseye/manpages-dev/termios.3.en.html
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Termios {
    /// input mode flags
    iflag: IFlag,
    /// output mode flags
    oflag: OFlag,
    /// control mode flags
    cflag: CFlag,
    /// local mode flags
    lflag: LFlag,
    /// line discipline
    line: u8,
    /// control characters
    cc: [u8; NCCS],
    /// input speed
    ispeed: u32,
    /// output speed
    ospeed: u32,
}

impl Default for Termios {
    fn default() -> Self {
        let mut cc: [u8; NCCS] = Default::default();
        // EOT, Ctrl-D
        cc[VEOF] = 0o04;
        // Additional end-of-line character (EOL).
        cc[VEOL] = 0o0;
        cc[VERASE] = 0o117;
        // Ctrl-C
        cc[VINTR] = 0o03;
        // NAK, Ctrl-U, or Ctrl-X, or also @
        cc[VKILL] = 0o25;
        Self {
            iflag: Default::default(),
            oflag: Default::default(),
            cflag: Default::default(),
            lflag: LFlag::ISIG | LFlag::ICANON | LFlag::ECHO,
            line: 0,
            cc,
            ispeed: 0,
            ospeed: 0,
        }
    }
}

/// tcsetattr options
const TCSANOW: u32 = 0;
const TCSADRAIN: u32 = 1;
const TCSAFLUSH: u32 = 2;

// cc array indexes
const VINTR: usize = 0;
const VQUIT: usize = 1;
const VERASE: usize = 2;
const VKILL: usize = 3;
const VEOF: usize = 4;
const VTIME: usize = 5;
const VMIN: usize = 6;
const VSWTC: usize = 7;
const VSTART: usize = 8;
const VSTOP: usize = 9;
const VSUSP: usize = 10;
const VEOL: usize = 11;
const VREPRINT: usize = 12;
const VDISCARD: usize = 13;
const VWERASE: usize = 14;
const VLNEXT: usize = 15;
const VEOL2: usize = 16;

bitflags! {
    #[derive(Default)]
    /// input mode flags
    pub struct IFlag: u32 {
        /// Ignore break condition.
        const IGNBRK = 1;
        /// Signal interrupt on break.
        const BRKINT = 2;
        /// Ignore characters with parity errors.
        const IGNPAR = 4;
        /// Mark parity and framing errors.
        const PARMRK = 10;
        /// Enable input parity check.
        const INPCK = 20;
        /// Strip 8th b	it off characters.
        const ISTRIP = 40;
        /// Map NL to CR on input.
        const INLCR = 100;
        /// Ignore CR.
        const IGNCR = 200;
        /// Map CR to NL on input.
        const ICRNL = 400;
        /// Map upper case to lower case on input.
        const IUCLC = 1000;
        /// Enable start/stop output control.
        const IXON = 2000;
        /// Any character will restart after stop.
        const IXANY = 4000;
        /// Enable start/stop input control.
        const IXOFF = 10000;
        /// Ring bell when input queue is full.
        const IMAXBEL = 20000;
        /// Input is UTF-8
        const IUTF8 = 40000;
    }

    /// output mode flags
    #[derive(Default)]
    pub struct OFlag: u32 {
        const OPOST = 1;
        const OLCUC = 2;
        const ONLCR = 4;
        const OCRNL = 10;
        const ONOCR = 20;
        const ONLRET = 40;
        const OFILL = 100;
        const OFDEL = 200;
        const NLDLY = 400;
        const NL0 = 0;
        const NL1 = 400;
        const CRDLY = 3000;
        const CR0 = 0;
        const CR1 = 1000;
        const CR2 = 2000;
        const CR3 = 3000;
        const TABDLY = 14000;
        const TAB0 = 0;
        const TAB1 = 4000;
        const TAB2 = 10000;
        const TAB3 = 14000;
        const XTABS = 14000;
        const BSDLY = 20000;
        const BS0 = 0;
        const BS1 = 20000;
        const VTDLY = 40000;
        const VT0 = 0;
        const VT1 = 40000;
        const FFDLY = 100000;
        const FF0 = 0;
        const FF1 =100000;
    }

    /// control mode flags
    #[derive(Default)]
    pub struct CFlag:u32 {
        const CS5 = 1;
        const CS6 = 2;
    }

    /// local mode flags
    pub struct LFlag: u32 {
        const ISIG = 1;
        const ICANON = 2;
        const XCASE = 4;
        const ECHO = 10;
        const ECHOE = 20;
        const ECHOK = 40;
        const ECHONL = 100;
        const NOFLSH = 200;
        const TOSTOP = 400;
        const ECHOCTL = 1000;
        const ECHOPRT = 2000;
        const ECHOKE = 4000;
        const FLUSHO = 10000;
        const PENDIN = 40000;
        const IEXTEN = 100000;
        const EXTPROC = 200000;
    }

}

#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct Winsize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}
