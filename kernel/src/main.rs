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

unsafe fn wait_input_clear() {
    for _ in 0..10000 {
        if (inb(0x64) & 2) == 0 {
            break;
        }
        core::arch::asm!("pause");
    }
}

unsafe fn mouse_write(data: u8) {
    wait_input_clear();
    outb(0x64, 0xD4);
    wait_input_clear();
    outb(0x60, data);
}

unsafe fn mouse_init() {
    // 1. Включаем вспомогательный порт мыши
    wait_input_clear();
    outb(0x64, 0xA8);

    // 2. Включаем прерывания мыши в контроллере (Compaq Status Byte)
    wait_input_clear();
    outb(0x64, 0x20); // Читать байт конфигурации
    wait_input_clear();
    let mut status = inb(0x60);
    status |= 2;    // Включить прерывание мыши (bit 1)
    status &= !0x20; // Отключить отключение мыши (если было)
    wait_input_clear();
    outb(0x64, 0x60); // Записать байт конфигурации
    wait_input_clear();
    outb(0x60, status);

    // 3. Установка дефолтов мыши
    mouse_write(0xF6);
    let _ = read_scancode_raw(); // сброс ACK (0xFA)

    // 4. Включение передачи данных (Enable Data Reporting)
    mouse_write(0xF4);
    let _ = read_scancode_raw(); // сброс ACK (0xFA)
}

fn read_scancode_raw() -> Option<u8> {
    let keyb_byte = unsafe { inb(0x64) };
    if (keyb_byte & 1) == 1 {
        let scancode = unsafe { inb(0x60) };
        Some(scancode)
    } else {
        None
    }
}

struct MousePacket {
    dx: i32,
    dy: i32,
    _left_click: bool,
}

fn read_mouse_packet(state: &mut usize, bytes: &mut [u8; 3]) -> Option<MousePacket> {
    let keyb_byte = unsafe { inb(0x64) };
    if (keyb_byte & 1) == 1 {
        // Проверяем, действительно ли это данные от мыши (бит 5 установлен)
        if (keyb_byte & 0x20) != 0 {
            let data = unsafe { inb(0x60) };
            
            // Синхронизация пакета: первый байт всегда имеет бит 3 установленным (в стандарте PS/2)
            if *state == 0 && (data & 0x08) == 0 {
                return None;
            }

            bytes[*state] = data;
            *state += 1;

            if *state == 3 {
                *state = 0;
                let flags = bytes[0];
                let mut dx = bytes[1] as i32;
                let mut dy = bytes[2] as i32;

                if (flags & 0x10) != 0 {
                    dx |= 0xFFFFFF00u32 as i32; // знаковое расширение X
                }
                if (flags & 0x20) != 0 {
                    dy |= 0xFFFFFF00u32 as i32; // знаковое расширение Y
                }

                // В PS/2 Y инвертирован (вверх - положительный)
                dy = -dy;

                let _left_click = (flags & 0x01) != 0;

                return Some(MousePacket { dx, dy, _left_click });
            }
        } else {
            // Клавиатурный байт, игнорируем здесь (или пропускаем в read_scancode)
            return None;
        }
    }
    None
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
    // 1. Попытка выключения через ACPI порт QEMU (0x604)
    core::arch::asm!(
        "out dx, ax",
        in("dx") 0x604u16,
        in("ax") 0x2000u16,
        options(nomem, nostack, preserves_flags)
    );

    // 2. Попытка через старый порт Bochs/QEMU (0xB004)
    core::arch::asm!(
        "out dx, ax",
        in("dx") 0xB004u16,
        in("ax") 0x2000u16,
        options(nomem, nostack, preserves_flags)
    );

    // Если порт не сработал (например, другое железо), останавливаем процессор
    loop {
        core::arch::asm!("hlt");
    }
}

fn read_scancode() -> Option<u8> {
    let keyb_byte = unsafe { inb(0x64) };
    if (keyb_byte & 1) == 1 {
        let scancode = unsafe { inb(0x60) };
        Some(scancode)
    } else {
        unsafe { core::arch::asm!("pause"); }
        None
    }
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

// 1. Отрисовка одного пикселя по координатам (x, y)
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
        // Формат BGR: Blue, Green, Red
        buffer[offset] = b;
        buffer[offset + 1] = g;
        buffer[offset + 2] = r;
    }
}

// 2. Отрисовка одного символа с учетом коэффициента масштабирования (scale)
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

// 3. Отрисовка строки с автоматическим сдвигом курсора
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

fn draw_cursor(buffer: &mut [u8], stride: usize, bpp: usize, width: usize, height: usize, x: usize, y: usize, r: u8, g: u8, b: u8) {
    for dy in 0..10 {
        for dx in 0..6 {
            // Простейшая стрелочка мыши
            if dx <= dy {
                draw_pixel(buffer, stride, bpp, width, height, x + dx, y + dy, r, g, b);
            }
        }
    }
}

fn erase_cursor(buffer: &mut [u8], stride: usize, bpp: usize, width: usize, height: usize, x: usize, y: usize) {
    for dy in 0..10 {
        for dx in 0..6 {
            if dx <= dy {
                draw_pixel(buffer, stride, bpp, width, height, x + dx, y + dy, 0x00, 0x00, 0xAA);
            }
        }
    }
}

// 4. Главная точка входа ядра
fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        let info = framebuffer.info();
        let buffer = framebuffer.buffer_mut();

        // Заливаем экран синим фоном
        for byte_idx in (0..buffer.len()).step_by(info.bytes_per_pixel) {
            buffer[byte_idx] = 0xAA; // Blue
            if byte_idx + 1 < buffer.len() {
                buffer[byte_idx + 1] = 0x00; // Green
            }
            if byte_idx + 2 < buffer.len() {
                buffer[byte_idx + 2] = 0x00; // Red
            }
        }

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
            draw_pixel(buffer, info.stride, info.bytes_per_pixel, info.width, info.height, cursor_x, i, 0xFF, 0xFF, 0xFF);
        }

        let mut pass_buf = [0u8; 32];
        let mut pass_len = 0usize;

        // ЦИКЛ АВТОРИЗАЦИИ
        loop {
            if let Some(code) = read_scancode() {
                if code < 0x80 {
                    if code == 0x1C {
                        // ENTER НАЖАТ
                        let success = &pass_buf[..pass_len] == b"magnus";
                        // Зануляем буфер пароля в памяти для безопасности
                        pass_buf.iter_mut().for_each(|b| *b = 0);

                        if success {
                            // 1. Очищаем экран синим фоном
                            for byte_idx in (0..buffer.len()).step_by(info.bytes_per_pixel) {
                                buffer[byte_idx] = 0xAA;
                                if byte_idx + 1 < buffer.len() {
                                    buffer[byte_idx + 1] = 0x00;
                                }
                                if byte_idx + 2 < buffer.len() {
                                    buffer[byte_idx + 2] = 0x00;
                                }
                            }

                            // Приветственное сообщение
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

                            let mut shell_y = 60usize;
                            let prompt = "> ";
                            let prompt_len_px = prompt.len() * 8 * 2;
                            let mut shell_x = 20 + prompt_len_px;

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

                              let mouse_x = info.width / 2;
                              let mouse_y = info.height / 2;
                              let _mouse_state = 0usize;
                              let _mouse_bytes = [0u8; 3];

                             // Инициализируем PS/2 мышь
                             unsafe {
                                 mouse_init();
                             }

                             draw_cursor(buffer, info.stride, info.bytes_per_pixel, info.width, info.height, mouse_x, mouse_y, 0xFF, 0xFF, 0xFF);

                              // ОСНОВНОЙ ЦИКЛ ШЕЛЛА
                              loop {
                                  if let Some(code) = read_scancode() {
                                    if code < 0x80 {
                                        if code == 0x1C {
                                            // ENTER В ШЕЛЛЕ
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
                                            } else if cmd == b"clear" {
                                                for byte_idx in
                                                    (0..buffer.len()).step_by(info.bytes_per_pixel)
                                                {
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

                             for i in shell_y..(shell_y + 8 * 2) {
                                 draw_pixel(buffer, info.stride, info.bytes_per_pixel, info.width, info.height, shell_x, i, 0xFF, 0xFF, 0xFF);
                             }
                                        } else if code == 0x0E {
                                            // BACKSPACE В ШЕЛЛЕ
                                            if cmd_len > 0 {
                                                // Стираем старую палочку
                                                for dy in 0..(8 * 2) {
                                                    for dx in 0..(8 * 2) {
                                                        let px = shell_x + dx;
                                                        let py = shell_y + dy;
                                                        if px < info.width && py < info.height {
                                                            let offset = (py * info.stride + px)
                                                                * info.bytes_per_pixel;
                                                            if offset + 2 < buffer.len() {
                                                                buffer[offset] = 0xAA;
                                                                buffer[offset + 1] = 0x00;
                                                                buffer[offset + 2] = 0x00;
                                                            }
                                                        }
                                                    }
                                                }

                                                cmd_len -= 1;
                                                cmd_buf[cmd_len] = 0;
                                                shell_x -= 8 * 2;

                                                for dy in 0..(8 * 2) {
                                                    for dx in 0..(8 * 2) {
                                                        let px = shell_x + dx;
                                                        let py = shell_y + dy;
                                                        if px < info.width && py < info.height {
                                                            let offset = (py * info.stride + px)
                                                                * info.bytes_per_pixel;
                                                            if offset + 2 < buffer.len() {
                                                                buffer[offset] = 0xAA;
                                                                buffer[offset + 1] = 0x00;
                                                                buffer[offset + 2] = 0x00;
                                                            }
                                                        }
                                                    }
                                                }

                                                // Рисуем палочку на новой позиции
                                                for i in shell_y..(shell_y + 8 * 2) {
                                                    draw_pixel(buffer, info.stride, info.bytes_per_pixel, info.width, info.height, shell_x, i, 0xFF, 0xFF, 0xFF);
                                                }
                                            }
                                        } else if let Some(c) = scancode_to_char(code) {
                                            // ПЕЧАТЬ В ШЕЛЛЕ
                                            if cmd_len < cmd_buf.len()
                                                && shell_x + (8 * 2) < info.stride
                                            {
                                                // Стираем старую палочку (на месте курсора)
                                                for dy in 0..(8 * 2) {
                                                    for dx in 0..(8 * 2) {
                                                        let px = shell_x + dx;
                                                        let py = shell_y + dy;
                                                        if px < info.width && py < info.height {
                                                            let offset = (py * info.stride + px)
                                                                * info.bytes_per_pixel;
                                                            if offset + 2 < buffer.len() {
                                                                buffer[offset] = 0xAA;
                                                                buffer[offset + 1] = 0x00;
                                                                buffer[offset + 2] = 0x00;
                                                            }
                                                        }
                                                    }
                                                }

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
                                                shell_x += 8 * 2;

                                                // Рисуем палочку на новой позиции
                                                for i in shell_y..(shell_y + 8 * 2) {
                                                    draw_pixel(buffer, info.stride, info.bytes_per_pixel, info.width, info.height, shell_x, i, 0xFF, 0xFF, 0xFF);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            // НЕВЕРНЫЙ ПАРОЛЬ
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
                        }
                    } else if code == 0x0E {
                        // BACKSPACE ПРИ ВВОДЕ ПАРОЛЯ (тихий режим)
                        if pass_len > 0 {
                            pass_len -= 1;
                            pass_buf[pass_len] = 0;
                        }
                    } else if let Some(c) = scancode_to_char(code) {
                        // ВВОД СИМВОЛА ПАРОЛЯ (тихий режим)
                        if pass_len < pass_buf.len() {
                            pass_buf[pass_len] = c as u8;
                            pass_len += 1;
                        }
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
