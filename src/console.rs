#[cfg(feature = "vga_text_mode")]
pub use vga::*;

#[cfg(not(feature = "vga_text_mode"))]
pub use nographic::*;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct ColorCode(u8);

impl ColorCode {
    const fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }

    const fn color_code(&self) -> u16 {
        (self.0 as u16) << 8
    }
}

#[cfg(feature = "vga_text_mode")]
mod vga {

    use crate::{arch, mm::PageParamA, spinlock::MutexIrq};
    use core::{fmt, mem::MaybeUninit, option::Option};
    use mm::{page::PageParam, Addr, PhysicalAddress};
    use volatile::Volatile;

    use super::ColorCode;

    fn char_code(b: u8, color_code: u16) -> u16 {
        color_code | b as u16
    }

    const DEFAULT_COLOR_CODE: ColorCode = ColorCode::new(Color::White, Color::Black);

    const BUFFER_HEIGHT: usize = 25;
    const BUFFER_WIDTH: usize = 80;

    #[repr(transparent)]
    struct Buffer([[Volatile<&'static mut u16>; BUFFER_WIDTH]; BUFFER_HEIGHT]);

    static mut WRITER: MaybeUninit<MutexIrq<Writer>> = MaybeUninit::uninit();

    pub fn init() {
        unsafe {
            WRITER = MaybeUninit::new(MutexIrq::new(Writer::new(None)));
        }
    }

    fn writer() -> &'static MutexIrq<Writer> {
        unsafe { WRITER.assume_init_ref() }
    }

    struct Writer {
        col: usize,
        default_color_code: u16,
        buf: &'static mut Buffer,
    }

    impl Writer {
        pub fn new(default_color_code: Option<ColorCode>) -> Self {
            let buf_va = PageParamA::linear_phys_to_virt(PhysicalAddress(0xb8000));
            Self {
                col: 0,
                default_color_code: default_color_code
                    .unwrap_or(DEFAULT_COLOR_CODE)
                    .color_code(),
                buf: unsafe { &mut *(buf_va.as_mut_ptr()) },
            }
        }

        pub fn write_string(&mut self, s: &str, color_code: Option<ColorCode>) {
            let color_code = color_code
                .map(|x| x.color_code())
                .unwrap_or_else(|| self.default_color_code);
            for b in s.bytes() {
                match b {
                    // printable ASCII byte or newline
                    0x20...0x7e | b'\n' => self.write_byte(b, color_code),
                    // For unprintable bytes, print a `■` character
                    _ => self.write_byte(0xfe, color_code),
                }
            }
        }

        /// Writes an ASCII byte to the buffer.
        fn write_byte(&mut self, b: u8, color_code: u16) {
            if b == b'\n' {
                self.new_line();
            } else {
                if self.col >= BUFFER_WIDTH {
                    self.new_line();
                }
                let row = BUFFER_HEIGHT - 1;
                let col = self.col;
                self.buf.0[row][col].write(char_code(b, color_code));
                self.col += 1;
            }
        }

        fn new_line(&mut self) {
            for row in 1..BUFFER_HEIGHT {
                for col in 0..BUFFER_WIDTH {
                    let c = self.buf.0[row][col].read();
                    self.buf.0[row - 1][col].write(c);
                }
            }
            self.clear_row(BUFFER_HEIGHT - 1);
            self.col = 0;
        }

        fn clear_row(&mut self, row: usize) {
            let blank = char_code(b' ', self.default_color_code);

            for col in 0..BUFFER_WIDTH {
                self.buf.0[row][col].write(blank);
            }
        }
    }

    pub(crate) fn _print(args: fmt::Arguments, color_code: Option<ColorCode>) {
        writer()
            .lock()
            .write_string(format!("{}", args).as_str(), color_code);
    }
}

#[cfg(not(feature = "vga_text_mode"))]
mod nographic {

    use core::{fmt, option::Option};
    use super::ColorCode;
    use crate::arch;

    static mut PRINTER: Option<spin::Mutex<fn(c: u8)>> = None;

    pub fn init() {
        unsafe { PRINTER = Some(spin::Mutex::new(arch::putchar)) }
    }

    pub(crate) fn _print(args: fmt::Arguments, _color_code: Option<ColorCode>) {
        let putchar_fn = unsafe { PRINTER.as_mut().unwrap().lock() };

        for &c in format!("{}", args).as_bytes() {
            putchar_fn(c)
        }
    }
}
