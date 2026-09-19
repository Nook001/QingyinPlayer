//! Recover leftover Qt Quick pointer grabs after Wayland drops a release.

use std::os::raw::c_char;

unsafe extern "C" {
    fn qingyin_install_pointer_guard();
    fn qingyin_drop_pointer_grabs_later();
    fn qingyin_drop_pointer_grabs(out: *mut c_char, out_len: i32) -> i32;
}

fn drop_now() -> String {
    let mut buf = [0_u8; 1024];
    unsafe {
        qingyin_drop_pointer_grabs(buf.as_mut_ptr().cast::<c_char>(), 1024);
        std::ffi::CStr::from_ptr(buf.as_ptr().cast())
            .to_string_lossy()
            .into_owned()
    }
}

pub fn install() {
    unsafe {
        qingyin_install_pointer_guard();
    }
}

pub fn drop_after_playback() {
    install();
    let report = drop_now();
    crate::pointer_trace("grabber", &report);
    unsafe {
        qingyin_drop_pointer_grabs_later();
    }
}

pub fn drop_after_gesture() {
    install();
    let report = drop_now();
    if report.starts_with("dropped") {
        crate::pointer_trace("grabber", &report);
    }
}
