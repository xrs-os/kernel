/// https://man7.org/linux/man-pages/man4/tty_ioctl.4.html

/// Equivalent to tcgetattr(fd, argp).
/// Get the current serial port settings.
pub const CMD_TCGETS: u32 = 0x5401;

/// Equivalent to tcsetattr(fd, TCSANOW, argp).
/// Set the current serial port settings.
pub const CMD_TCSETS: u32 = 0x5402;

/// Equivalent to tcsetattr(fd, TCSADRAIN, argp).
/// Allow the output buffer to drain, and set the current
/// serial port settings.
pub const CMD_TCSETSW: u32 = 0x5403;

/// Equivalent to tcsetattr(fd, TCSAFLUSH, argp).
/// Allow the output buffer to drain, discard pending input,
/// and set the current serial port settings.
pub const CMD_TCSETSF: u32 = 0x5404;

/// When successful, equivalent to *argp = tcgetpgrp(fd).
/// Get the process group ID of the foreground process group
/// on this terminal.
pub const CMD_TIOCGPGRP: u32 = 0x540F;

/// Equivalent to tcsetpgrp(fd, *argp).
/// Set the foreground process group ID of this terminal.
pub const CMD_TIOCSPGRP: u32 = 0x5410;

/// Get window size.
pub const CMD_TIOCGWINSZ: u32 = 0x5413;
