//! Compositor de Janelas

extern crate alloc;

use alloc::boxed::Box;
use core::ptr;
use core::sync::atomic::Ordering;
use crate::gdi::{
    HDC, HWND, COLORREF, RECT,
    SRCCOPY, fill_rect_color, text_out, draw_rect, draw_cursor, hide_cursor,
    COLOR_3DFACE, COLOR_3DSHADOW, COLOR_3DHILIGHT, COLOR_WINDOWTEXT,
};
use crate::window::{
    window_manager,
    draw_window_border,
};

static COMPOSITOR: core::sync::atomic::AtomicPtr<Compositor> = core::sync::atomic::AtomicPtr::new(ptr::null_mut());
static CURSOR_X: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(100);
static CURSOR_Y: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(100);

fn compositor() -> *mut Compositor {
    COMPOSITOR.load(Ordering::Relaxed)
}

pub struct Compositor {
    hdc_screen: HDC,
    framebuffer_addr: *mut u8,
    framebuffer_width: u32,
    framebuffer_height: u32,
    framebuffer_pitch: u32,
    initialized: bool,
}

impl Compositor {
    pub const fn new() -> Compositor {
        Compositor {
            hdc_screen: HDC(ptr::null_mut()),
            framebuffer_addr: ptr::null_mut(),
            framebuffer_width: 0,
            framebuffer_height: 0,
            framebuffer_pitch: 0,
            initialized: false,
        }
    }

    pub fn init(&mut self, framebuffer_addr: *mut u8, width: u32, height: u32, pitch: u32) {
        self.framebuffer_addr = framebuffer_addr;
        self.framebuffer_width = width;
        self.framebuffer_height = height;
        self.framebuffer_pitch = pitch;
        self.initialized = true;

        crate::gdi::gdi_subsystem_init(framebuffer_addr, width, height, pitch);
        self.hdc_screen = crate::gdi::get_main_dc();
    }

    /// Desenha a área de trabalho (desktop) com ícones
    pub fn draw_desktop(&self) {
        let hdc = self.hdc_screen;
        let width = self.framebuffer_width as i32;
        let height = self.framebuffer_height as i32;

        // Fundo cincho claro
        crate::gdi::fill_rect_color(hdc, 0, 0, width, height, COLOR_3DFACE);

        // Desenha ícones da área de trabalho
        self.draw_desktop_icon(hdc, 20, 20, "Meu Computador");
        self.draw_desktop_icon(hdc, 20, 60, "Lixeira");
        self.draw_desktop_icon(hdc, 20, 100, "Rede");
    }

    fn draw_desktop_icon(&self, hdc: HDC, x: i32, y: i32, label: &str) {
        let icon_size = 32;
        let label_y = y + icon_size + 2;

        // Fundo do ícone
        crate::gdi::fill_rect_color(hdc, x, y, icon_size, icon_size, COLOR_3DSHADOW);
        // Borda superior/esquerda (clara)
        crate::gdi::hline(hdc, x, x + icon_size - 1, y, COLOR_3DHILIGHT);
        crate::gdi::vline(hdc, x, y, y + icon_size - 1, COLOR_3DHILIGHT);
        // Borda inferior/direita (escura)
        crate::gdi::hline(hdc, x, x + icon_size - 1, y + icon_size - 1, COLOR_WINDOWTEXT);
        crate::gdi::vline(hdc, x + icon_size - 1, y, y + icon_size - 1, COLOR_WINDOWTEXT);

        // Desenha o texto do ícone
        crate::gdi::text_out(hdc, x + 2, label_y, label, COLOR_WINDOWTEXT);
    }

    pub fn draw_window(&self, hwnd: HWND, rect: &RECT, title: &str, active: bool) {
        let hdc = self.hdc_screen;

        let title_color = if active {
            crate::gdi::COLORREF::rgb(0, 0, 128)
        } else {
            crate::gdi::COLORREF::rgb(128, 128, 128)
        };

        let border_color = if active {
            crate::gdi::COLORREF::rgb(0, 0, 0)
        } else {
            crate::gdi::COLORREF::rgb(128, 128, 128)
        };

        let client_color = COLOR_3DFACE;

        crate::gdi::fill_rect_color(hdc, rect.left, rect.top, rect.right, rect.bottom, border_color);
        fill_rect_color(
            hdc,
            rect.left + 2,
            rect.top + 24,
            rect.right - 2,
            rect.bottom - 2,
            client_color,
        );
        fill_rect_color(
            hdc,
            rect.left + 2,
            rect.top + 2,
            rect.right - 2,
            rect.top + 22,
            title_color,
        );
        crate::gdi::text_out(hdc, rect.left + 6, rect.top + 4, title, crate::gdi::COLORREF::rgb(255, 255, 255));

        // Botão de fechar (X)
        for i in 0..8 {
            crate::gdi::draw_pixel(hdc, rect.right - 16 + i, rect.top + 4 + i, crate::gdi::COLORREF::black());
            crate::gdi::draw_pixel(hdc, rect.right - 16 + 7 - i, rect.top + 4 + i, crate::gdi::COLORREF::black());
        }
    }

    pub fn render_all_windows(&self) {
        // Primeiro desenha a área de trabalho
        self.draw_desktop();

        unsafe {
            let wm = &*window_manager();
            let mut current = wm.windows.head;
            while !current.is_null() {
                let hwnd = (*current).hwnd;
                let rect = (*current).rect;
                let title_bytes = (*current).window_name;
                let title = core::str::from_utf8(&title_bytes).unwrap_or("Window");
                let is_active = (*current).active;

                self.draw_window(hwnd, &rect, title, is_active);
                current = (*current).hwnd_next;
            }
        }

        // Desenha o cursor do mouse por cima de tudo
        let cursor_x = CURSOR_X.load(Ordering::Relaxed);
        let cursor_y = CURSOR_Y.load(Ordering::Relaxed);
        crate::gdi::draw_cursor(self.hdc_screen, cursor_x, cursor_y);
    }

    pub fn clear_screen(&self, color: COLORREF) {
        crate::gdi::fill_rect_color(
            self.hdc_screen,
            0,
            0,
            self.framebuffer_width as i32,
            self.framebuffer_height as i32,
            color,
        );
    }

    pub fn update_screen(&self) {
        // Sem buffer duplo - desenho direto
    }

    pub fn set_cursor_pos(&self, x: i32, y: i32) {
        CURSOR_X.store(x, Ordering::Relaxed);
        CURSOR_Y.store(y, Ordering::Relaxed);
    }

    pub fn get_cursor_pos(&self) -> (i32, i32) {
        (
            CURSOR_X.load(Ordering::Relaxed),
            CURSOR_Y.load(Ordering::Relaxed),
        )
    }
}

pub fn compositor_init(framebuffer_addr: *mut u8, width: u32, height: u32, pitch: u32) {
    let comp = Box::into_raw(Box::new(Compositor::new()));
    unsafe {
        (*comp).init(framebuffer_addr, width, height, pitch);
    }
    COMPOSITOR.store(comp, Ordering::Relaxed);
}

pub fn get_compositor() -> *mut Compositor {
    COMPOSITOR.load(Ordering::Relaxed)
}

pub fn compositor_draw_window(hwnd: HWND, rect: &RECT, title: &str, active: bool) {
    unsafe {
        let comp = &mut *compositor();
        comp.draw_window(hwnd, rect, title, active);
    }
}

pub fn compositor_render_all() {
    unsafe {
        let comp = &mut *compositor();
        comp.render_all_windows();
    }
}

pub fn compositor_clear_screen(color: COLORREF) {
    unsafe {
        let comp = &mut *compositor();
        comp.clear_screen(color);
    }
}

pub fn compositor_update_screen() {
    unsafe {
        let comp = &mut *compositor();
        comp.update_screen();
    }
}

pub fn compositor_set_cursor_pos(x: i32, y: i32) {
    unsafe {
        let comp = &mut *compositor();
        comp.set_cursor_pos(x, y);
    }
}

pub fn compositor_get_cursor_pos() -> (i32, i32) {
    unsafe {
        let comp = &mut *compositor();
        comp.get_cursor_pos()
    }
}
