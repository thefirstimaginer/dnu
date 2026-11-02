//! Gerenciador de Dispositivo Gráfico (GDI) - Simplificado
//! Estilo Windows NT 4.0

extern crate alloc;

use alloc::boxed::Box;
use core::ptr;

// ============================================================================
// Constantes de estilo de janela (WS_*) — estilo Windows NT 4.0
// ============================================================================

pub const WS_OVERLAPPED: u32 = 0x00000000;
pub const WS_POPUP: u32 = 0x80000000;
pub const WS_CHILD: u32 = 0x40000000;
pub const WS_MINIMIZE: u32 = 0x20000000;
pub const WS_VISIBLE: u32 = 0x10000000;
pub const WS_DISABLED: u32 = 0x08000000;
pub const WS_CLIPSIBLINGS: u32 = 0x04000000;
pub const WS_CLIPCHILDREN: u32 = 0x02000000;
pub const WS_MAXIMIZE: u32 = 0x01000000;
pub const WS_CAPTION: u32 = 0x00C00000;
pub const WS_BORDER: u32 = 0x00800000;
pub const WS_DLGFRAME: u32 = 0x00400000;
pub const WS_VSCROLL: u32 = 0x00200000;
pub const WS_HSCROLL: u32 = 0x00100000;
pub const WS_SYSMENU: u32 = 0x00080000;
pub const WS_THICKFRAME: u32 = 0x00040000;
pub const WS_GROUP: u32 = 0x00020000;
pub const WS_TABSTOP: u32 = 0x00010000;
pub const WS_MINIMIZEBOX: u32 = 0x00020000;
pub const WS_MAXIMIZEBOX: u32 = 0x00010000;

// Estilos estendidos (WS_EX_*)
pub const WS_EX_WINDOWEDGE: u32 = 0x00000100;
pub const WS_EX_CLIENTEDGE: u32 = 0x00000200;

// Raster operations
pub const SRCCOPY: u32 = 0x00CC0020;

// ============================================================================
// Tipos GDI
// ============================================================================

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HDC(pub *mut core::ffi::c_void);

impl HDC {
    pub const fn null() -> HDC {
        HDC(ptr::null_mut())
    }
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HWND(pub *mut core::ffi::c_void);

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HBRUSH(pub *mut core::ffi::c_void);

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HPEN(pub *mut core::ffi::c_void);

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HFONT(pub *mut core::ffi::c_void);

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HGDIOBJ(pub *mut core::ffi::c_void);

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HBITMAP(pub *mut core::ffi::c_void);

pub type WNDPROC = extern "C" fn(HWND, u32, usize, isize) -> isize;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct WNDCLASSA {
    pub style: u32,
    pub lpfn_wnd_proc: WNDPROC,
    pub cb_cls_extra: i32,
    pub cb_wnd_extra: i32,
    pub h_instance: usize,
    pub h_icon: usize,
    pub h_cursor: usize,
    pub hbr_background: HBRUSH,
    pub lpsz_menu_name: *const i8,
    pub lpsz_class_name: *const i8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct WNDCLASSEXA {
    pub cb_size: u32,
    pub style: u32,
    pub lpfn_wnd_proc: WNDPROC,
    pub cb_cls_extra: i32,
    pub cb_wnd_extra: i32,
    pub h_instance: usize,
    pub h_icon: usize,
    pub h_cursor: usize,
    pub hbr_background: HBRUSH,
    pub lpsz_menu_name: *const i8,
    pub lpsz_class_name: *const i8,
    pub h_icon_sm: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CREATESTRUCTA {
    pub lp_create_params: *mut core::ffi::c_void,
    pub h_instance: usize,
    pub h_menu: usize,
    pub hwnd_parent: HWND,
    pub cy: i32,
    pub cx: i32,
    pub y: i32,
    pub x: i32,
    pub style: u32,
    pub lpsz_name: *const i8,
    pub lpsz_class: *const i8,
    pub dw_ex_style: u32,
}

// ============================================================================
// Tipos auxiliares
// ============================================================================

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct COLORREF(pub u32);

impl COLORREF {
    pub const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
        COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
    }
    pub const fn black() -> COLORREF { COLORREF(0x000000) }
    pub const fn white() -> COLORREF { COLORREF(0xFFFFFF) }
    pub const fn red() -> COLORREF { COLORREF(0x0000FF) }
    pub const fn green() -> COLORREF { COLORREF(0x00FF00) }
    pub const fn blue() -> COLORREF { COLORREF(0xFF0000) }
    pub const fn gray() -> COLORREF { COLORREF(0x808080) }
    pub const fn light_gray() -> COLORREF { COLORREF(0xC0C0C0) }
}

// Cores do sistema NT 4.0
pub const COLOR_3DFACE: COLORREF = COLORREF(0xC0C0C0);
pub const COLOR_3DHILIGHT: COLORREF = COLORREF(0xFFFFFF);
pub const COLOR_3DSHADOW: COLORREF = COLORREF(0x808080);
pub const COLOR_WINDOWTEXT: COLORREF = COLORREF(0x000000);

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl RECT {
    pub const fn new(l: i32, t: i32, r: i32, b: i32) -> RECT {
        RECT { left: l, top: t, right: r, bottom: b }
    }
}

// ============================================================================
// Estrutura interna do HDC
// ============================================================================

#[repr(C)]
pub struct HDC_STRUCT {
    pub hdc: HDC,
    pub hdc_surface: *mut u32,   // Superfície como u32 (32bpp)
    pub hdc_surface_width: u32,  // Largura em pixels
    pub hdc_surface_height: u32, // Altura em pixels
    pub hdc_surface_pitch: u32,  // Pitch em BYTES (não pixels)
}

impl HDC_STRUCT {
    /// Converte pitch em bytes para pitch em u32
    pub fn pitch_in_u32(&self) -> usize {
        self.hdc_surface_pitch as usize / 4
    }
}

// ============================================================================
// Estado global do GDI
// ============================================================================

pub mod gdi_global {
    use core::sync::atomic::AtomicPtr;
    use core::sync::atomic::Ordering;
    use crate::gdi::HDC;

    static MAIN_DC: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(core::ptr::null_mut());
    static CURSOR_X: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(core::ptr::null_mut());
    static CURSOR_Y: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(core::ptr::null_mut());

    pub fn set_main_dc(hdc: HDC) { MAIN_DC.store(hdc.0, Ordering::Relaxed); }
    pub fn get_main_dc() -> HDC { HDC(MAIN_DC.load(Ordering::Relaxed)) }

    pub fn set_cursor_pos(x: i32, y: i32) {
        CURSOR_X.store(x as *mut core::ffi::c_void, Ordering::Relaxed);
        CURSOR_Y.store(y as *mut core::ffi::c_void, Ordering::Relaxed);
    }
    pub fn get_cursor_pos() -> (i32, i32) {
        let x = CURSOR_X.load(Ordering::Relaxed) as i32;
        let y = CURSOR_Y.load(Ordering::Relaxed) as i32;
        (x, y)
    }
}

// ============================================================================
// Funções GDI públicas
// ============================================================================

pub fn gdi_init() {}

pub fn gdi_subsystem_init(hdc_mem: *mut u8, width: u32, height: u32, pitch: u32) {
    let surface = hdc_mem as *mut u32;
    let hdc = HDC_STRUCT {
        hdc: HDC(ptr::null_mut()),
        hdc_surface: surface,
        hdc_surface_width: width,
        hdc_surface_height: height,
        hdc_surface_pitch: pitch,
    };
    let hdc_ptr = Box::into_raw(Box::new(hdc));
    gdi_global::set_main_dc(HDC(hdc_ptr as *mut core::ffi::c_void));
}

pub fn get_main_dc() -> HDC {
    gdi_global::get_main_dc()
}

/// Escreve um pixel diretamente na superfície (usa pitch em bytes).
fn put_pixel_raw(surface: *mut u32, pitch_in_u32: usize, width: u32, height: u32, x: i32, y: i32, color: u32) {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return;
    }
    unsafe {
        let offset = (y as usize * pitch_in_u32) + (x as usize);
        *surface.offset(offset as isize) = color;
    }
}

/// Lê um pixel da superfície.
fn get_pixel_raw(surface: *mut u32, pitch_in_u32: usize, width: u32, height: u32, x: i32, y: i32) -> u32 {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return 0;
    }
    unsafe {
        let offset = (y as usize * pitch_in_u32) + (x as usize);
        *surface.offset(offset as isize)
    }
}

pub fn clear_screen(color: COLORREF) {
    let hdc = get_main_dc();
    let hdc_struct = unsafe { &*(hdc.0 as *const HDC_STRUCT) };
    let width = hdc_struct.hdc_surface_width;
    let height = hdc_struct.hdc_surface_height;
    let pitch_u32 = hdc_struct.pitch_in_u32();
    let surface = hdc_struct.hdc_surface;

    unsafe {
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                put_pixel_raw(surface, pitch_u32, width, height, x, y, color.0);
            }
        }
    }
}

pub fn fill_rect(hdc: HDC, x: i32, y: i32, w: i32, h: i32, color: COLORREF) {
    let hdc_struct = unsafe { &*(hdc.0 as *const HDC_STRUCT) };
    if hdc_struct.hdc_surface.is_null() { return; }

    let pitch_u32 = hdc_struct.pitch_in_u32();
    let width = hdc_struct.hdc_surface_width as i32;
    let height = hdc_struct.hdc_surface_height as i32;
    let surface = hdc_struct.hdc_surface;

    let width_u32 = width as u32;
    let height_u32 = height as u32;

    for yy in y..y + h {
        if yy < 0 || yy >= height { continue; }
        for xx in x..x + w {
            if xx < 0 || xx >= width { continue; }
            put_pixel_raw(surface, pitch_u32, width_u32, height_u32, xx, yy, color.0);
        }
    }
}

pub fn hline(hdc: HDC, x1: i32, x2: i32, y: i32, color: COLORREF) {
    let start = x1.min(x2);
    let end = x1.max(x2);
    for x in start..=end { draw_pixel(hdc, x, y, color); }
}

pub fn vline(hdc: HDC, x: i32, y1: i32, y2: i32, color: COLORREF) {
    let start = y1.min(y2);
    let end = y1.max(y2);
    for y in start..=end { draw_pixel(hdc, x, y, color); }
}

pub fn text_out(hdc: HDC, x: i32, y: i32, text: &str, color: COLORREF) -> bool {
    let mut current_x = x;
    for ch in text.chars() { draw_char(hdc, current_x, y, ch, color); current_x += 8; }
    true
}

pub fn bit_blt(hdc_dest: HDC, x_dest: i32, y_dest: i32, width: i32, height: i32,
               hdc_src: HDC, x_src: i32, y_src: i32, _rop: u32) -> bool {
    for y in 0..height { for x in 0..width {
        let src_color = get_pixel_color(hdc_src, x_src + x, y_src + y);
        draw_pixel(hdc_dest, x_dest + x, y_dest + y, src_color);
    }}
    true
}

pub fn select_object(hdc: HDC, _hgdiobj: HGDIOBJ) -> HGDIOBJ {
    HGDIOBJ(ptr::null_mut())
}

pub fn fill_rect_color(hdc: HDC, x: i32, y: i32, w: i32, h: i32, color: COLORREF) {
    fill_rect(hdc, x, y, w, h, color);
}

pub fn draw_rect(hdc: HDC, x: i32, y: i32, w: i32, h: i32, color: COLORREF) {
    hline(hdc, x, x + w, y, color);
    hline(hdc, x, x + w, y + h, color);
    vline(hdc, x, y, y + h, color);
    vline(hdc, x + w, y, y + h, color);
}

pub fn draw_pixel(hdc: HDC, x: i32, y: i32, color: COLORREF) -> bool {
    let hdc_struct = unsafe { &*(hdc.0 as *const HDC_STRUCT) };
    if hdc_struct.hdc_surface.is_null() { return false; }

    let pitch_u32 = hdc_struct.pitch_in_u32();
    let width = hdc_struct.hdc_surface_width;
    let height = hdc_struct.hdc_surface_height;
    let surface = hdc_struct.hdc_surface;

    put_pixel_raw(surface, pitch_u32, width, height, x, y, color.0);
    true
}

pub fn get_pixel_color(hdc: HDC, x: i32, y: i32) -> COLORREF {
    let hdc_struct = unsafe { &*(hdc.0 as *const HDC_STRUCT) };
    if hdc_struct.hdc_surface.is_null() { return COLORREF::black(); }

    let pitch_u32 = hdc_struct.pitch_in_u32();
    let width = hdc_struct.hdc_surface_width;
    let height = hdc_struct.hdc_surface_height;
    let surface = hdc_struct.hdc_surface;

    let val = get_pixel_raw(surface, pitch_u32, width, height, x, y);
    COLORREF(val)
}

fn draw_char(hdc: HDC, x: i32, y: i32, ch: char, color: COLORREF) {
    for dy in 0..8 { for dx in 0..6 { draw_pixel(hdc, x + dx, y + dy, color); } }
}

// ============================================================================
// Cursor do mouse
// ============================================================================

static CURSOR_BITMAP: [u32; 88] = [
    0xFFFFFFFF, 0xFFFFFFFE, 0xFFFFFFFC, 0xFFFFFFF8,
    0xFFFFFFF0, 0xFFFFFFE0, 0xFFFFFFC0, 0xFFFFFF80,
    0xFFFFFF00, 0xFFFF8000, 0xFFFF0000, 0xFFFE0000,
    0xFFFC0000, 0xFFF80000, 0xFFF00000, 0xFFE00000,
    0xFFC00000, 0xFF800000, 0xFF000000, 0xFE000000,
    0xFC000000, 0xF8000000, 0xF0000000, 0xE0000000,
    0xC0000000, 0x80000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000,
    0x00000000, 0x00000000, 0x00000000, 0x00000000,
];

pub fn draw_cursor(hdc: HDC, x: i32, y: i32) {
    let hdc_struct = unsafe { &*(hdc.0 as *const HDC_STRUCT) };
    if hdc_struct.hdc_surface.is_null() { return; }

    let pitch_u32 = hdc_struct.pitch_in_u32();
    let width = hdc_struct.hdc_surface_width as i32;
    let height = hdc_struct.hdc_surface_height as i32;
    let surface = hdc_struct.hdc_surface;

    // Desenha cursor de seta branco com contorno preto (22x32, limitado pelo bitmap)
    for dy in 0..22 {
        for dx in 0..32 {
            let pixel_val = CURSOR_BITMAP[dy * 4 + dx / 8];
            let bit = 0x80 >> (dx % 8);
            let is_black = (pixel_val & bit) != 0;
            let cx = x + dx as i32;
            let cy = y + dy as i32;

            if cx >= 0 && cx < width && cy >= 0 && cy < height {
                if is_black {
                    put_pixel_raw(surface, pitch_u32, width as u32, height as u32, cx, cy, 0x000000);
                } else if dx > 0 && dy > 0 {
                    // Interior branco
                    put_pixel_raw(surface, pitch_u32, width as u32, height as u32, cx, cy, 0xFFFFFF);
                }
            }
        }
    }
}

pub fn hide_cursor(hdc: HDC, x: i32, y: i32) {
    // Para simplificar, apenas redesenha o fundo na posição do cursor
    // Em uma implementação completa, salvaríamos a região antes de desenhar
    let hdc_struct = unsafe { &*(hdc.0 as *const HDC_STRUCT) };
    if hdc_struct.hdc_surface.is_null() { return; }

    let pitch_u32 = hdc_struct.pitch_in_u32();
    let width = hdc_struct.hdc_surface_width as i32;
    let height = hdc_struct.hdc_surface_height as i32;
    let surface = hdc_struct.hdc_surface;

    // Redesenha um bloco 32x32 com a cor de fundo da área de trabalho
    for dy in 0..32 {
        for dx in 0..32 {
            let cx = x + dx as i32;
            let cy = y + dy as i32;
            if cx >= 0 && cx < width && cy >= 0 && cy < height {
                put_pixel_raw(surface, pitch_u32, width as u32, height as u32, cx, cy, COLOR_3DFACE.0);
            }
        }
    }
}
