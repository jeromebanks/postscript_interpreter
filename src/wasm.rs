//! The wasm32 export surface (Stage 16) — pscat in a browser tab.
//!
//! A hand-rolled C ABI over byte buffers, no wasm-bindgen: the caller
//! (web/pscat.js) copies the program into linear memory, drives the
//! interpreter with the same `begin_source`/`step_n` budget the winit
//! window uses, and blits the live RGBA pixmap into a `<canvas>` via
//! putImageData. One interpreter per module instance (wasm is
//! single-threaded; the state is a thread_local).
//!
//! Contract:
//! - `ps_alloc`/`ps_dealloc` — scratch buffers for passing sources in;
//! - `ps_begin(src, len, w, h)` — fresh page, program queued; 0 = ok,
//!   -1 = unusable page size;
//! - `ps_step(n)` — run up to n objects: 1 = more work, 0 = done,
//!   -1 = a PostScript error (report via `ps_error_*`; the machine is
//!   unwound but page and operand stack survive, REPL-style),
//!   -2 = no program begun;
//! - `ps_pixels`/`ps_pixels_len`/`ps_width`/`ps_height` — the live
//!   canvas, premultiplied RGBA (opaque page, so plain RGBA);
//! - `ps_run(src, len, w, h)` — begin + drive to completion.

use std::cell::RefCell;

use crate::Interp;

thread_local! {
    static INTERP: RefCell<Option<Interp>> = const { RefCell::new(None) };
    static ERROR: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// # Safety
/// `ptr` must come from `ps_alloc(len)` and not be freed twice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_dealloc(ptr: *mut u8, len: usize) {
    unsafe {
        drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
    }
}

/// # Safety
/// `src..src+len` must be readable (a `ps_alloc` buffer the caller
/// filled).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_begin(src: *const u8, len: usize, w: u32, h: u32) -> i32 {
    let source = unsafe { std::slice::from_raw_parts(src, len) }.to_vec();
    let Some(mut it) = Interp::with_page(w, h) else {
        return -1;
    };
    it.begin_source(&source);
    INTERP.with(|s| *s.borrow_mut() = Some(it));
    ERROR.with(|e| e.borrow_mut().clear());
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_step(budget: i32) -> i32 {
    INTERP.with(|s| {
        let mut slot = s.borrow_mut();
        let Some(it) = slot.as_mut() else {
            return -2;
        };
        match it.step_n(budget.max(0) as usize) {
            Ok(true) => 1,
            Ok(false) => 0,
            Err(e) => {
                let report = it.error_report(&e);
                ERROR.with(|er| *er.borrow_mut() = report.into_bytes());
                -1
            }
        }
    })
}

/// # Safety
/// Same contract as [`ps_begin`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ps_run(src: *const u8, len: usize, w: u32, h: u32) -> i32 {
    let rc = unsafe { ps_begin(src, len, w, h) };
    if rc != 0 {
        return rc;
    }
    loop {
        match ps_step(1 << 16) {
            1 => {}
            status => return status,
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_pixels() -> *const u8 {
    INTERP.with(|s| {
        s.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |it| it.gfx().pixmap.data().as_ptr())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_pixels_len() -> usize {
    INTERP.with(|s| {
        s.borrow()
            .as_ref()
            .map_or(0, |it| it.gfx().pixmap.data().len())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_width() -> u32 {
    INTERP.with(|s| s.borrow().as_ref().map_or(0, |it| it.gfx().pixmap.width()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_height() -> u32 {
    INTERP.with(|s| s.borrow().as_ref().map_or(0, |it| it.gfx().pixmap.height()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_error_ptr() -> *const u8 {
    ERROR.with(|e| e.borrow().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn ps_error_len() -> usize {
    ERROR.with(|e| e.borrow().len())
}
