//! Gerenciador de Janelas

extern crate alloc;

use alloc::boxed::Box;
use core::ptr;
use core::sync::atomic::{AtomicU32, Ordering};
use crate::gdi::{
    HDC, HWND, HBRUSH, HPEN, HFONT, HGDIOBJ, COLORREF, RECT,
    WNDPROC, WNDCLASSA, WNDCLASSEXA, CREATESTRUCTA,
    WS_OVERLAPPED, WS_POPUP, WS_CHILD, WS_VISIBLE, WS_DISABLED,
    WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_MAXIMIZE, WS_MINIMIZE,
    WS_BORDER, WS_SYSMENU, WS_THICKFRAME, WS_CAPTION,
    WS_MINIMIZEBOX, WS_MAXIMIZEBOX,
    WS_EX_WINDOWEDGE, WS_EX_CLIENTEDGE,
    SRCCOPY, fill_rect_color, draw_rect, text_out,
};

#[repr(C)]
pub struct WND {
    pub hwnd: HWND,
    pub h_instance: usize,
    pub h_menu: usize,
    pub hwnd_parent: HWND,
    pub hwnd_next: *mut WND,
    pub hwnd_prev: *mut WND,
    pub rect: RECT,
    pub rect_client: RECT,
    pub style: u32,
    pub ex_style: u32,
    pub wnd_proc: WNDPROC,
    pub class_name: [u8; 64],
    pub window_name: [u8; 128],
    pub h_menu_internal: usize,
    pub h_cursor: usize,
    pub h_icon: usize,
    pub h_cursor_default: usize,
    pub hbr_background: HBRUSH,
    pub user_data: usize,
    pub state: WindowState,
    pub visible: bool,
    pub enabled: bool,
    pub minimized: bool,
    pub maximized: bool,
    pub active: bool,
    pub focus: bool,
    pub z_index: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowState {
    Hidden,
    Normal,
    Minimized,
    Maximized,
}

pub struct WindowList {
    pub head: *mut WND,
    pub tail: *mut WND,
    pub count: usize,
}

pub struct WindowManager {
    pub windows: WindowList,
    pub next_id: AtomicU32,
    pub active_window: HWND,
    pub focus_window: HWND,
    pub capture_window: HWND,
}

impl WindowManager {
    pub const fn new() -> WindowManager {
        WindowManager {
            windows: WindowList {
                head: ptr::null_mut(),
                tail: ptr::null_mut(),
                count: 0,
            },
            next_id: AtomicU32::new(1),
            active_window: HWND(ptr::null_mut()),
            focus_window: HWND(ptr::null_mut()),
            capture_window: HWND(ptr::null_mut()),
        }
    }

    pub fn add_window(&mut self, hwnd: HWND) {
        unsafe {
            let new_window = Box::into_raw(Box::new(WND {
                hwnd,
                h_instance: 0,
                h_menu: 0,
                hwnd_parent: HWND(ptr::null_mut()),
                hwnd_next: ptr::null_mut(),
                hwnd_prev: ptr::null_mut(),
                rect: RECT { left: 0, top: 0, right: 100, bottom: 100 },
                rect_client: RECT { left: 0, top: 0, right: 100, bottom: 100 },
                style: WS_OVERLAPPED,
                ex_style: 0,
                wnd_proc: default_wnd_proc,
                class_name: [0; 64],
                window_name: [0; 128],
                h_menu_internal: 0,
                h_cursor: 0,
                h_icon: 0,
                h_cursor_default: 0,
                hbr_background: HBRUSH(ptr::null_mut()),
                user_data: 0,
                state: WindowState::Normal,
                visible: true,
                enabled: true,
                minimized: false,
                maximized: false,
                active: false,
                focus: false,
                z_index: self.next_id.load(Ordering::Relaxed),
            }));

            if self.windows.head.is_null() {
                self.windows.head = new_window;
                self.windows.tail = new_window;
            } else {
                unsafe {
                    (*self.windows.tail).hwnd_next = new_window;
                    (*new_window).hwnd_prev = self.windows.tail;
                }
                self.windows.tail = new_window;
            }
            self.windows.count += 1;
        }
    }

    pub fn remove_window(&mut self, hwnd: HWND) {
        unsafe {
            let mut current = self.windows.head;
            while !current.is_null() {
                if (*current).hwnd == hwnd {
                    let prev = (*current).hwnd_prev;
                    let next = (*current).hwnd_next;

                    if !prev.is_null() {
                        (*prev).hwnd_next = next;
                    } else {
                        self.windows.head = next;
                    }

                    if !next.is_null() {
                        (*next).hwnd_prev = prev;
                    } else {
                        self.windows.tail = prev;
                    }

                    self.windows.count -= 1;
                    drop(Box::from_raw(current));
                    break;
                }
                current = (*current).hwnd_next;
            }
        }
    }

    pub fn find_window(&self, hwnd: HWND) -> Option<*mut WND> {
        unsafe {
            let mut current = self.windows.head;
            while !current.is_null() {
                if (*current).hwnd == hwnd {
                    return Some(current);
                }
                current = (*current).hwnd_next;
            }
            None
        }
    }
}

static WINDOW_MANAGER: core::sync::atomic::AtomicPtr<WindowManager> = core::sync::atomic::AtomicPtr::new(ptr::null_mut());

pub fn window_manager() -> *mut WindowManager {
    WINDOW_MANAGER.load(Ordering::Relaxed)
}

pub fn window_manager_init() {
    let wm = Box::into_raw(Box::new(WindowManager {
        windows: WindowList {
            head: ptr::null_mut(),
            tail: ptr::null_mut(),
            count: 0,
        },
        next_id: AtomicU32::new(1),
        active_window: HWND(ptr::null_mut()),
        focus_window: HWND(ptr::null_mut()),
        capture_window: HWND(ptr::null_mut()),
    }));

    WINDOW_MANAGER.store(wm, Ordering::Relaxed);
}

pub fn window_manager_next_id() -> u32 {
    unsafe {
        let wm = &mut *window_manager();
        wm.next_id.fetch_add(1, Ordering::Relaxed)
    }
}

pub fn register_class(_wnd_class: &WNDCLASSA) -> u16 {
    1
}

pub fn register_class_ex(_wnd_class: &WNDCLASSEXA) -> u16 {
    1
}

pub fn create_window(
    _dw_ex_style: u32,
    _lp_class_name: *const u8,
    _lp_window_name: *const u8,
    dw_style: u32,
    x: i32,
    y: i32,
    n_width: i32,
    n_height: i32,
    _h_wnd_parent: HWND,
    _h_menu: usize,
    h_instance: usize,
    _lp_param: *const core::ffi::c_void,
) -> HWND {
    let hwnd = HWND(window_manager_next_id() as *mut core::ffi::c_void);

    unsafe {
        let wm = &mut *window_manager();
        wm.add_window(hwnd);

        if let Some(window_ptr) = wm.find_window(hwnd) {
            (*window_ptr).h_instance = h_instance;
            (*window_ptr).hwnd_parent = _h_wnd_parent;
            (*window_ptr).rect = RECT { left: x, top: y, right: x + n_width, bottom: y + n_height };
            (*window_ptr).rect_client = RECT { left: 0, top: 0, right: n_width, bottom: n_height };
            (*window_ptr).style = dw_style;
            (*window_ptr).ex_style = _dw_ex_style;
            (*window_ptr).visible = (dw_style & WS_VISIBLE) != 0;
            (*window_ptr).enabled = true;
            (*window_ptr).state = WindowState::Normal;
        }
    }

    hwnd
}

pub fn destroy_window(hwnd: HWND) -> bool {
    unsafe {
        let wm = &mut *window_manager();
        if wm.find_window(hwnd).is_some() {
            wm.remove_window(hwnd);
            true
        } else {
            false
        }
    }
}

pub fn show_window(hwnd: HWND, _n_cmd_show: i32) -> bool {
    unsafe {
        let wm = &mut *window_manager();
        if let Some(window_ptr) = wm.find_window(hwnd) {
            (*window_ptr).visible = true;
            true
        } else {
            false
        }
    }
}

pub fn update_window(hwnd: HWND) -> bool {
    send_message(hwnd, 0x000F, 0, 0);
    true
}

pub fn show_window_update(hwnd: HWND, _n_cmd_show: i32) -> bool {
    show_window(hwnd, _n_cmd_show);
    update_window(hwnd)
}

pub fn send_message(hwnd: HWND, msg: u32, w_param: usize, l_param: isize) -> isize {
    unsafe {
        let wm = &mut *window_manager();
        if let Some(window_ptr) = wm.find_window(hwnd) {
            let wnd_proc = (*window_ptr).wnd_proc;
            wnd_proc(hwnd, msg, w_param, l_param)
        } else {
            0
        }
    }
}

pub fn post_message(_hwnd: HWND, _msg: u32, _w_param: usize, _l_param: isize) -> bool {
    true
}

pub fn get_message(_msg: &mut MSG, _hwnd: HWND, _msg_filter_min: u32, _msg_filter_max: u32) -> bool {
    false
}

pub fn translate_message(_msg: &MSG) -> bool {
    false
}

pub fn dispatch_message(msg: &MSG) -> isize {
    send_message(msg.hwnd, msg.message, msg.w_param, msg.l_param)
}

pub fn update_window_rect(_hwnd: HWND, _rect: &RECT) -> bool {
    true
}

pub fn get_window_dc(_hwnd: HWND) -> crate::gdi::HDC {
    crate::gdi::HDC::null()
}

pub fn release_window_dc(_hwnd: HWND, _hdc: crate::gdi::HDC) -> i32 {
    0
}

#[repr(C)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub w_param: usize,
    pub l_param: isize,
    pub time: u32,
    pub pt: crate::gdi::RECT,
}

pub fn window_subsystem_init() {
    window_manager_init();
}

pub fn create_simple_window(
    class_name: *const u8,
    window_name: *const u8,
    style: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    parent: HWND,
    menu: usize,
    instance: usize,
) -> HWND {
    create_window(
        0, class_name, window_name,
        style | WS_OVERLAPPED | WS_VISIBLE,
        x, y, width, height,
        parent, menu, instance,
        ptr::null(),
    )
}

pub fn update_all_windows(hdc: HDC) {
    unsafe {
        let wm = &*window_manager();
        let mut current = wm.windows.head;
        while !current.is_null() {
            if (*current).visible {
                draw_window_border(hdc, (*current).hwnd);
                send_message((*current).hwnd, 0x000F, 0, 0);
            }
            current = (*current).hwnd_next;
        }
    }
}

pub fn draw_window_border(hdc: HDC, hwnd: HWND) {
    unsafe {
        let wm = &mut *window_manager();
        if let Some(window_ptr) = wm.find_window(hwnd) {
            let rect = (*window_ptr).rect;
            let title_bytes = (*window_ptr).window_name;
            let title = core::str::from_utf8(&title_bytes).unwrap_or("Window");
            let is_active = (*window_ptr).active;

            let title_color = if is_active {
                COLORREF::rgb(0, 0, 128)
            } else {
                COLORREF::rgb(128, 128, 128)
            };

            let border_color = if is_active {
                COLORREF::rgb(0, 0, 0)
            } else {
                COLORREF::rgb(128, 128, 128)
            };

            let client_color = COLORREF::rgb(192, 192, 192);

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

            crate::gdi::text_out(hdc, rect.left + 6, rect.top + 4, title, COLORREF::rgb(255, 255, 255));

            draw_close_button(hdc, rect.right - 16, rect.top + 4);
        }
    }
}

fn draw_close_button(hdc: HDC, x: i32, y: i32) {
    for i in 0..8 {
        crate::gdi::draw_pixel(hdc, x + i, y + i, COLORREF::black());
        crate::gdi::draw_pixel(hdc, x + 7 - i, y + i, COLORREF::black());
    }
}

extern "C" fn default_wnd_proc(_hwnd: HWND, _msg: u32, _w_param: usize, _l_param: isize) -> isize {
    0
}
