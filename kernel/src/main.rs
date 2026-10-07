#![no_std]
#![no_main]

use bootloader_api::{entry_point, BootInfo};
use core::arch::asm;
use core::panic::PanicInfo;

entry_point!(kernel_main);

unsafe fn inb(port: u16) -> u8 {
    let value: u8;

    asm!(
        "in al, dx",
        in("dx") port,
        out("al") value,
        options(nomem, nostack, preserves_flags)
    );

    value
}

unsafe fn outb(port: u16, value: u8) {
    asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nomem, nostack, preserves_flags)
    );
}

fn read_scancode() -> Option<u8> {
    let status = unsafe { inb(0x64) };

    if (status & 1) == 0 {
        return None;
    }

    Some(unsafe { inb(0x60) })
}

fn scancode_to_char(scancode: u8) -> Option<char> {
    match scancode {
        0x10 => Some('q'),
        0x11 => Some('w'),
        0x12 => Some('e'),
        0x13 => Some('r'),
        0x14 => Some('t'),
        0x15 => Some('y'),
        0x16 => Some('u'),
        0x17 => Some('i'),
        0x18 => Some('o'),
        0x19 => Some('p'),

        0x1E => Some('a'),
        0x1F => Some('s'),
        0x20 => Some('d'),
        0x21 => Some('f'),
        0x22 => Some('g'),
        0x23 => Some('h'),
        0x24 => Some('j'),
        0x25 => Some('k'),
        0x26 => Some('l'),

        0x2C => Some('z'),
        0x2D => Some('x'),
        0x2E => Some('c'),
        0x2F => Some('v'),
        0x30 => Some('b'),
        0x31 => Some('n'),
        0x32 => Some('m'),

        0x39 => Some(' '),

        _ => None,
    }
}

fn draw_pixel(
    buffer: &mut [u8],
    stride: usize,
    bpp: usize,
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    r: u8,
    g: u8,
    b: u8,
) {
    if x >= width || y >= height {
        return;
    }

    let offset = (y * stride + x) * bpp;

    if offset + 2 < buffer.len() {
        buffer[offset] = b;
        buffer[offset + 1] = g;
        buffer[offset + 2] = r;
    }
}

fn draw_char(
    buffer: &mut [u8],
    stride: usize,
    bpp: usize,
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    c: char,
    scale: usize,
) {
    let code = c as usize;

    let glyph = if code < 128 {
        font8x8::legacy::BASIC_LEGACY[code]
    } else {
        [0; 8]
    };

    for row in 0..8 {
        let byte = glyph[row];

        for col in 0..8 {
            if ((byte >> col) & 1) == 1 {
                for dy in 0..scale {
                    for dx in 0..scale {
                        draw_pixel(
                            buffer,
                            stride,
                            bpp,
                            width,
                            height,
                            x + col * scale + dx,
                            y + row * scale + dy,
                            255,
                            255,
                            255,
                        );
                    }
                }
            }
        }
    }
}

fn draw_string(
    buffer: &mut [u8],
    stride: usize,
    bpp: usize,
    width: usize,
    height: usize,
    mut x: usize,
    y: usize,
    text: &str,
    scale: usize,
) {
    for c in text.chars() {
        draw_char(buffer, stride, bpp, width, height, x, y, c, scale);

        x += 8 * scale;
    }
}

unsafe fn reboot() -> ! {
    core::arch::asm!(
        "out 0x64, al",
        in("al") 0xFEu8,
        options(nostack, preserves_flags)
    );

    loop {
        core::arch::asm!("hlt");
    }
}

unsafe fn shutdown() -> ! {
    core::arch::asm!(
        "out dx, ax",
        in("dx") 0x604u16,
        in("ax") 0x2000u16,
        options(nomem, nostack, preserves_flags)
    );

    core::arch::asm!(
        "out dx, ax",
        in("dx") 0xB004u16,
        in("ax") 0x2000u16,
        options(nomem, nostack, preserves_flags)
    );

    loop {
        core::arch::asm!("hlt");
    }
}

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        let info = framebuffer.info();
        let buffer = framebuffer.buffer_mut();

        // Синий фон
        for byte_idx in (0..buffer.len()).step_by(info.bytes_per_pixel) {
            buffer[byte_idx] = 0xAA;

            if byte_idx + 1 < buffer.len() {
                buffer[byte_idx + 1] = 0x00;
            }

            if byte_idx + 2 < buffer.len() {
                buffer[byte_idx + 2] = 0x00;
            }
        }

        // =========================
        // PASSWORD
        // =========================

        draw_string(
            buffer,
            info.stride,
            info.bytes_per_pixel,
            info.width,
            info.height,
            100,
            100,
            "PASSWORD:",
            3,
        );

        let start_x = 100 + "PASSWORD:".len() * 8 * 3;
        let mut cursor_x = start_x;

        for i in 100..(100 + 8 * 3) {
            draw_pixel(
                buffer,
                info.stride,
                info.bytes_per_pixel,
                info.width,
                info.height,
                cursor_x,
                i,
                255,
                255,
                255,
            );
        }

        let mut pass_buf = [0u8; 32];
        let mut pass_len = 0usize;

        loop {
            if let Some(code) = read_scancode() {
                if code >= 0x80 {
                    continue;
                }

                if code == 0x1C {
                    let success = &pass_buf[..pass_len] == b"magnus";

                    pass_buf.iter_mut().for_each(|b| *b = 0);

                    if success {
                        break;
                    }

                    draw_string(
                        buffer,
                        info.stride,
                        info.bytes_per_pixel,
                        info.width,
                        info.height,
                        100,
                        150,
                        "DENIED",
                        3,
                    );

                    pass_len = 0;
                } else if code == 0x0E {
                    if pass_len > 0 {
                        pass_len -= 1;
                        pass_buf[pass_len] = 0;
                    }
                } else if let Some(c) = scancode_to_char(code) {
                    if pass_len < pass_buf.len() {
                        pass_buf[pass_len] = c as u8;
                        pass_len += 1;
                    }
                }
            }
        }

        // =========================
        // SHELL
        // =========================

        for byte_idx in (0..buffer.len()).step_by(info.bytes_per_pixel) {
            buffer[byte_idx] = 0xAA;

            if byte_idx + 1 < buffer.len() {
                buffer[byte_idx + 1] = 0x00;
            }

            if byte_idx + 2 < buffer.len() {
                buffer[byte_idx + 2] = 0x00;
            }
        }

        draw_string(
            buffer,
            info.stride,
            info.bytes_per_pixel,
            info.width,
            info.height,
            20,
            20,
            "WELCOME TO SHELL!",
            2,
        );

        let prompt = "> ";
        let prompt_len_px = prompt.len() * 8 * 2;

        let mut shell_x = 20 + prompt_len_px;
        let mut shell_y = 60usize;

        draw_string(
            buffer,
            info.stride,
            info.bytes_per_pixel,
            info.width,
            info.height,
            20,
            shell_y,
            prompt,
            2,
        );

        let mut cmd_buf = [0u8; 64];
        let mut cmd_len = 0usize;

        // =========================
        // SHELL LOOP
        // =========================

        loop {
            if let Some(code) = read_scancode() {
                if code >= 0x80 {
                    continue;
                }

                if code == 0x1C {
                    // ENTER

                    let cmd = &cmd_buf[..cmd_len];

                    shell_y += 24;

                    if cmd == b"help" {
                        draw_string(
                            buffer,
                            info.stride,
                            info.bytes_per_pixel,
                            info.width,
                            info.height,
                            20,
                            shell_y,
                            "COMMANDS: HELP, CLEAR, REBOOT, HI, SHUTDOWN",
                            2,
                        );

                        shell_y += 24;
                    } else if cmd == b"shutdown" {
                        unsafe {
                            shutdown();
                        }
                    } else if cmd == b"hi" {
                        draw_string(
                            buffer,
                            info.stride,
                            info.bytes_per_pixel,
                            info.width,
                            info.height,
                            20,
                            shell_y,
                            "HELLO FROM BARE METAL RUST!",
                            2,
                        );

                        shell_y += 24;
                    } else if cmd == b"reboot" {
                        unsafe {
                            reboot();
                        }
                    } else if cmd.starts_with(b"echo ") {
                        let text = &cmd[5..];
                        let text = core::str::from_utf8(text).unwrap();
                        draw_string(
                            buffer,
                            info.stride,
                            info.bytes_per_pixel,
                            info.width,
                            info.height,
                            20,
                            shell_y,
                            text,
                            2,
                        );
                        shell_y += 24;
                    } else if cmd == b"clear" {
                        for byte_idx in (0..buffer.len()).step_by(info.bytes_per_pixel) {
                            buffer[byte_idx] = 0xAA;

                            if byte_idx + 1 < buffer.len() {
                                buffer[byte_idx + 1] = 0x00;
                            }

                            if byte_idx + 2 < buffer.len() {
                                buffer[byte_idx + 2] = 0x00;
                            }
                        }

                        shell_y = 20;
                    } else if cmd_len > 0 {
                        draw_string(
                            buffer,
                            info.stride,
                            info.bytes_per_pixel,
                            info.width,
                            info.height,
                            20,
                            shell_y,
                            "UNKNOWN COMMAND",
                            2,
                        );

                        shell_y += 24;
                    }

                    cmd_buf.iter_mut().for_each(|b| *b = 0);
                    cmd_len = 0;

                    if shell_y > 500 {
                        shell_y = 20;
                    }

                    draw_string(
                        buffer,
                        info.stride,
                        info.bytes_per_pixel,
                        info.width,
                        info.height,
                        20,
                        shell_y,
                        prompt,
                        2,
                    );

                    shell_x = 20 + prompt_len_px;
                } else if code == 0x0E {
                    // BACKSPACE

                    if cmd_len > 0 {
                        cmd_len -= 1;
                        cmd_buf[cmd_len] = 0;

                        shell_x -= 8 * 2;

                        // Стираем символ
                        for dy in 0..16 {
                            for dx in 0..16 {
                                let px = shell_x + dx;
                                let py = shell_y + dy;

                                if px < info.width && py < info.height {
                                    let offset = (py * info.stride + px) * info.bytes_per_pixel;

                                    if offset + 2 < buffer.len() {
                                        buffer[offset] = 0xAA;
                                        buffer[offset + 1] = 0x00;
                                        buffer[offset + 2] = 0x00;
                                    }
                                }
                            }
                        }
                    }
                } else if let Some(c) = scancode_to_char(code) {
                    // Печать символа

                    if cmd_len < cmd_buf.len() && shell_x + 16 < info.width {
                        cmd_buf[cmd_len] = c as u8;
                        cmd_len += 1;

                        draw_char(
                            buffer,
                            info.stride,
                            info.bytes_per_pixel,
                            info.width,
                            info.height,
                            shell_x,
                            shell_y,
                            c,
                            2,
                        );

                        shell_x += 16;
                    }
                }
            }
        }
    }

    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
