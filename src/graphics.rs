//! Gerenciador de Dispositivo Gráfico (GDI) - Estilo Windows NT 4.0
//!
//! Implementa:
//! - Contexto de dispositivo (HDC) para desenho
//! - Cores RGB e paleta do Windows NT 4.0
//! - Desenho de retângulos, linhas, elipses
//! - Renderização de texto com fonte bitmap
//! - BitBlt (transferência de blocos de bits)
//! - Gerenciamento de janelas (HWND)
//! - Sistema de mensagens (mensagem loop)
//! - Comando de renderização para exibir na tela

use core::sync::atomic::{AtomicU32, AtomicBool, Ordering};
use core::ptr;

// ============================================================================
// CORES DO WINDOWS NT 4.0 (Palette)
// ============================================================================

/// Cores clássicas do Windows NT 4.0
pub mod color {
    pub const BLACK: u32 = 0x000000;
    pub const WHITE: u32 = 0xFFFFFF;
    pub const GRAY: u32 = 0x808080;
    pub const LIGHT_GRAY: u32 = 0xC0C0C0;
    pub const DARK_GRAY: u32 = 0x808080;
    pub const BLUE: u32 = 0x0000FF;
    pub const GREEN: u32 = 0x00FF00;
    pub const RED: u32 = 0xFF0000;
    pub const YELLOW: u32 = 0xFFFF00;
    pub const MAGENTA: u32 = 0xFF00FF;
    pub const CYAN: u32 = 0xFFFF00;
    pub const BLACK_TEXT: u32 = 0x000000;
    pub const WHITE_TEXT: u32 = 0xFFFFFF;
    
    // Cores específicas do Windows NT 4.0
    pub const WINDOW_BG: u32 = 0xC0C0C0;      // Cinza claro da área de trabalho
    pub const WINDOW_FRAME: u32 = 0x808080;   // Cinza médio das bordas
    pub const WINDOW_TITLE_ACTIVE: u32 = 0x000080; // Azul escuro da barra de título ativa
    pub const WINDOW_TITLE_INACTIVE: u32 = 0xC0C0C0; // Cinza claro da barra inativa
    pub const WINDOW_TEXT_ACTIVE: u32 = 0xFFFFFF;   // Texto branco na barra ativa
    pub const WINDOW_TEXT_INACTIVE: u32 = 0x000000; // Texto preto na barra inativa
    pub const BUTTON_FACE: u32 = 0xC0C0C0;    // Cinza claro dos botões
    pub const BUTTON_SHADOW: u32 = 0x808080;  // Cinza escuro da sombra do botão
    pub const BUTTON_HIGHLIGHT: u32 = 0xFFFFFF; // Branco do destaque do botão
    pub const MENU_BAR: u32 = 0xC0C0C0;       // Cinza claro da barra de menu
    pub const MENU_ITEM: u32 = 0xFFFFFF;      // Branco do item de menu
    pub const MENU_ITEM_SELECTED: u32 = 0x000080; // Azul escuro do item selecionado
}

// ============================================================================
// Tipos de Dados do GDI
// ============================================================================

/// Handle para um contexto de dispositivo
#[derive(Debug, Clone, Copy)]
pub struct HDC {
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub buffer: *mut u32,
}

/// Handle para um bitmap
#[derive(Debug, Clone, Copy)]
pub struct HBITMAP {
    pub width: u32,
    pub height: u32,
    pub pixels: *mut u32,
    pub owns_memory: bool,
}

/// Handle para um pincel (brush)
#[derive(Clone, Copy)]
pub struct HBRUSH(pub u32);

/// Handle para uma caneta (pen)
#[derive(Clone, Copy)]
pub struct HPEN(pub u32);

/// Handle para uma fonte (font)
#[derive(Clone, Copy)]
pub struct HFONT(pub u32);

/// Rectângulo
#[derive(Clone, Copy, Debug)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl RECT {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self { left, top, right, bottom }
    }
    
    pub fn width(&self) -> i32 {
        self.right - self.left
    }
    
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
    
    pub fn is_empty(&self) -> bool {
        self.left >= self.right || self.top >= self.bottom
    }
    
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

/// Ponto 2D
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct POINT {
    pub x: i32,
    pub y: i32,
}

impl POINT {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

// ============================================================================
// Gerenciamento de Fontes (Bitmap simples)
// ============================================================================

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Font {
    pub width: u8,
    pub height: u8,
    pub data: *const u8,
}

/// Fonte padrão 8x8 (fonte bitmap simples do sistema)
static DEFAULT_FONT: [u8; 256 * 8] = [0; 2048]; // Será preenchido na inicialização

static mut CURRENT_FONT: Font = Font {
    width: 8,
    height: 8,
    data: core::ptr::null(),
};

/// Inicializa a fonte padrão com glifos básicos
pub fn init_font() {
    unsafe {
        CURRENT_FONT = Font {
            width: 8,
            height: 8,
            data: DEFAULT_FONT.as_ptr(),
        };
    }
}

/// Desenha um caractere na tela
fn draw_char(hdc: &mut HDC, x: i32, y: i32, ch: char, color: u32) {
    if hdc.buffer.is_null() {
        return;
    }
    
    let font = unsafe { &CURRENT_FONT };
    let char_index = ch as u32;
    let glyph_offset = char_index * font.height as u32;
    
    for row in 0..font.height as u32 {
        let byte = if font.data.is_null() {
            0xFF // Caractere padrão (bloco)
        } else {
            unsafe { font.data.add((glyph_offset + row) as usize).read() }
        };
        
        for bit in 0..8 {
            if byte & (0x80 >> byte) != 0 {
                let px = x + bit as i32;
                let py = y + row as i32;
                if px >= 0 && py >= 0 && px < hdc.width as i32 && py < hdc_surface_height() as i32 {
                    unsafe {
                        *hdc.buffer.add((py as usize * (hdc_surface_pitch() as usize)) + px as usize) = color;
                    }
                }
            }
        }
    }
}

fn hdc_surface_height() -> u32 {
    // Será definido quando o framebuffer for inicializado
    480
}

fn hdc_surface_pitch() -> u32 {
    // Será definido quando o framebuffer for inicializado
    640
}

// ============================================================================
// Operações de Desenho
// ============================================================================

/// Limpa uma área retangular com uma cor
pub fn fill_rect(hdc: &mut HDC, rect: RECT, color: u32) {
    if hdc.buffer.is_null() {
        return;
    }
    
    let h = hdc_surface_height();
    let p = hdc_surface_pitch();
    let w = hdc.surface_width();
    let h_d = hdc_surface_height();
    
    let left = rect.left.clamp(0, w as i32);
    let top = rect.top.clamp(0, h_d as i32);
    let right = rect.right.min(w as i32);
    let bottom = rect.bottom.min(h_d as i32);
    
    if left >= right || top >= bottom {
        return;
    }
    
    unsafe {
        for y in top..bottom {
            for x in left..right {
                let offset = (y as usize * (p as usize)) + (x as usize);
                *hdc.buffer.add(offset) = color;
            }
        }
    }
}

/// Desenha um retângulo contornado
pub fn draw_rect(hdc: &mut HDC, rect: RECT, color: u32, thickness: u32) {
    if hdc.buffer.is_null() {
        return;
    }
    
    let hdc_struct = unsafe { &*(hdc.0 as *const HDC_STRUCT) };
    let w = hdc_struct.hdc_surface_width;
    let h = hdc_struct.hdc_surface_height;
    
    // Linha superior
    for x in rect.left..rect.right {
        for dy in 0..thickness as i32 {
            draw_pixel(hdc, x, rect.top + dy, color);
        }
    }
    // Linha inferior
    for x in rect.left..rect.right {
        for dy in 0..thickness as i32 {
            draw_pixel(hdc, x, rect.bottom - dy - 1, color);
        }
    }
    // Linha esquerda
    for y in rect.top..rect.bottom {
        for dx in 0..thickness as i32 {
            draw_pixel(hdc, rect.left + dx, y, color);
        }
    }
    // Linha direita
    for y in rect.top..rect.bottom {
        for dx in 0..thickness as i32 {
            draw_pixel(hdc, rect.right - dx - 1, y, color);
        }
    }
}

/// Desenha uma linha (algoritmo de Bresenham simplificado)
pub fn draw_line(hdc: &mut HDC, x1: i32, y1: i32, x2: i32, y2: i32, color: u32) {
    if hdc.buffer.is_null() {
        return;
    }
    
    let mut x = x1;
    let mut y = y1;
    let dx = if x2 > x1 { x2 - x1 } else { x1 - x2 };
    let dy = if y2 > y1 { y2 - y1 } else { y1 - y2 };
    let sx = if x1 < x2 { 1 } else { -1 };
    let sy = if y1 < y2 { 1 } else { -1 };
    let mut err = dx - dy;
    
    loop {
        draw_pixel(hdc, x, y, color);
        if x == x2 && y == y2 { break; }
        let e2 = 2 * err;
        if e2 > -dy { err -= dy; x += sx; }
        if e2 < dx { err += dx; y += sy; }
    }
}

/// Desenha um pixel no framebuffer
pub fn draw_pixel(hdc: HDC, x: i32, y: i32, color: u32) {
    let hdc_struct = unsafe { &*(hdc.0 as *const HDC_STRUCT) };
    
    if x < 0 || y < 0 || x >= hdc_struct.hdc_surface_width as i32 || y >= hdc_struct.hdc_surface_height as i32 {
        return;
    }
    
    unsafe {
        let surface = hdc_struct.hdc_surface as *mut u32;
        let offset = (y as usize * hdc_struct.hdc_surface_pitch as usize) + x as usize;
        *surface.add(offset) = color;
    }
}

/// Obtém a cor de um pixel
pub fn get_pixel(hdc: &HDC, x: i32, y: i32) -> u32 {
    let hdc_struct = unsafe { &*(hdc.0 as *const HDC_STRUCT) };
    
    if x < 0 || y < 0 || x >= hdc_struct.hdc_surface_width as i32 || y >= hdc_struct.hdc_surface_height as i32 {
        return 0;
    }
    
    unsafe {
        let surface = hdc_struct.hdc_surface as *const u32;
        let offset = (y as usize * hdc_struct.hdc_surface_pitch as usize) + x as usize;
        *surface.add(offset)
    }
}

// ============================================================================
// Operações de Blit (Bit Block Transfer)
// ============================================================================

/// Copia um bloco de pixels de um DC para outro
pub fn bit_blt(hdc_dest: HDC, x_dest: i32, y_dest: i32, width: i32, height: i32,
               hdc_src: HDC, x_src: i32, y_src: i32) {
    if hdc_dest.0.is_null() || hdc_src.0.is_null() {
        return;
    }
    
    unsafe {
        let src = &*(hdc_src.0 as *const HDC_STRUCT);
        let dst = &mut *(hdc_dest.0 as *mut HDC_STRUCT);
        
        for y in 0..height {
            for x in 0..width {
                let src_x = x_src + x;
                let src_y = y_src + y;
                
                if src_x < 0 || src_y < 0 || src_x >= src.hdc_surface_width as i32 || src_y >= src.hdc_surface_height as i32 {
                    continue;
                }
                
                let src_pixel = unsafe {
                    let surf = src.hdc_surface as *const u32;
                    let offset = (src_y as usize * src.hdc_surface_pitch as usize) + src_x as usize;
                    *surf.add(offset)
                };
                
                draw_pixel(hdc_dest, x_dest + x, y_dest + y, src_pixel);
            }
        }
    }
}

// ============================================================================
// Funções Auxiliares de Desenho
// ============================================================================

/// Desenha uma linha horizontal
pub fn hline(hdc: HDC, x1: i32, x2: i32, y: i32, color: u32) {
    let start = x1.min(x2);
    let end = x1.max(x2);
    for x in start..=end {
        draw_pixel(hdc, x, y, color);
    }
}

/// Desenha uma linha vertical
pub fn vline(hdc: HDC, x: i32, y1: i32, y2: i32, color: u32) {
    let start = y1.min(y2);
    let end = y1.max(y2);
    for y in start..=end {
        draw_pixel(hdc, x, y, color);
    }
}

/// Desenha um retângulo preenchido
pub fn fill_rect_simple(hdc: HDC, x: i32, y: i32, w: i32, h: i32, color: u32) {
    let rect = RECT { left: x, top: y, right: x + w, bottom: y + h };
    fill_rect(hdc, rect, color);
}

/// Desenha um retângulo com borda
pub fn draw_rect_border(hdc: HDC, x: i32, y: i32, w: i32, h: i32, color: u32, thickness: u32) {
    let rect = RECT { left: x, top: y, right: x + w, bottom: y + h };
    draw_rect(hdc, rect, color, thickness);
}

/// Desenha um retângulo com borda dupla (efeito 3D do Windows 95/NT)
pub fn draw_rect_3d(hdc: HDC, x: i32, y: i32, w: i32, h: i32, raised: bool) {
    let light = COLORREF::rgb(255, 255, 255);
    let shadow = COLORREF::rgb(128, 128, 128);
    let dark_shadow = COLORREF::rgb(128, 128, 128);
    let highlight = COLORREF::rgb(255, 255, 255);
    
    if raised {
        // Borda elevada (botão pressionado para cima)
        draw_rect(hdc, RECT { left: x, top: y, right: x + w, bottom: y + 1 }, light, 1);
        draw_rect(hdc, rect.left, rect.top + 1, rect.right - rect.left - 1, 1, highlight);
        draw_rect(hdc, rect.right - 1, rect.top + 1, 1, rect.height - 2, shadow);
        draw_rect(hdc, rect.left, rect.bottom - 1, rect.width - 1, 1, dark_shadow);
    } else {
        // Borda afundada (botão pressionado)
        draw_rect(hdc, rect.left, rect.top, rect.width - 1, 1, dark_shadow);
        draw_rect(hdc, rect.left, rect.top + 1, 1, rect.height - 2, dark_shadow);
        draw_rect(hdc, rect.right - 1, rect.top + 1, 1, rect.height - 2, highlight);
        draw_rect(hdc, rect.left, rect.bottom - 1, rect.width - 1, 1, highlight);
    }
}

/// Desenha um texto na posição especificada
pub fn draw_text(hdc: HDC, x: i32, y: i32, text: &str, color: u32) {
    let mut cursor_x = x;
    for ch in text.chars() {
        draw_char(hdc, cursor_x, y, ch, color);
        cursor_x += 8; // Largura fixa de 8 pixels por caractere
    }
}

/// Desenha texto centralizado em um retângulo
pub fn draw_text_centered(hdc: HDC, rect: &RECT, text: &str, color: u32) {
    let text_width = text.len() as i32 * 8; // 8 pixels por caractere
    let text_height = 8;
    let x = rect.left + (rect.right - rect.left - text_width) / 2;
    let y = rect.top + (rect.bottom - rect.top - text_height) / 2;
    text_out(hdc, x, y, text, color);
}

/// Obtém o DC principal da tela
pub fn get_main_dc() -> HDC {
    crate::gdi_global::get_main_dc()
}

/// Atualiza a tela (swap buffers se necessário)
pub fn update_screen(hdc: HDC) {
    // Para framebuffer direto, não há swap buffer
    // O desenho é direto na memória de vídeo
}

/// Desenha um gradiente vertical
pub fn draw_gradient_vertical(hdc: HDC, x: i32, y: i32, w: i32, h: i32, top_color: u32, bottom_color: u32) {
    let top_r = (top_color >> 16) & 0xFF;
    let top_g = (top_color >> 8) & 0xFF;
    let top_b = top_color & 0xFF;
    
    let bottom_r = (bottom_color >> 16) & 0xFF;
    let bottom_g = (bottom_color >> 8) & 0xFF;
    let bottom_b = bottom_color & 0xFF;
    
    for row in 0..h {
        let t = row as f32 / h as f32;
        let r = (top_r as f32 + (bottom_r as f32 - top_r as f32) * t) as u32;
        let g = (top_g as f32 + (bottom_g as f32 - top_g as f32) * t) as u32;
        let b = (top_b as f32 + (bottom_b as f32 - top_b as f32) * t) as u32;
        let color = (r << 16) | (g << 8) | b;
        
        hline(hdc, x, x + w - 1, y + row as i32, color);
    }
}

/// Verifica se um ponto está dentro de um retângulo
pub fn rect_contains(rect: &RECT, x: i32, y: i32) -> bool {
    x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
}

/// Verifica se dois retângulos se sobrepõem
pub fn rects_overlap(r1: &RECT, r2: &RECT) -> bool {
    r1.left < r2.right && r1.right > r2.left && r1.top < r2.bottom && r1.bottom > r2.top
}

/// Calcula a interseção de dois retângulos
pub fn rect_intersection(r1: &RECT, r2: &RECT) -> Option<RECT> {
    let left = r1.left.max(r2.left);
    let top = r1.top.max(r2.top);
    let right = r1.right.min(r2.right);
    let bottom = r1.bottom.min(r2.bottom);
    
    if left < right && top < bottom {
        Some(RECT { left, top, right, bottom })
    } else {
        None
    }
}

// ============================================================================
// Módulo interno de gerenciamento do estado global do GDI
// ============================================================================

pub mod gdi_global {
    use core::sync::atomic::AtomicPtr;
    use core::sync::atomic::Ordering;
    use crate::gdi::HDC;

    static MAIN_DC: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(core::ptr::null_mut());

    pub fn set_main_dc(hdc: HDC) {
        MAIN_DC.store(hdc.0, Ordering::Relaxed);
    }

    pub fn get_main_dc() -> HDC {
        HDC(MAIN_DC.load(Ordering::Relaxed))
    }
}