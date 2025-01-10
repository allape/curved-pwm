use std::ffi::CStr;

use esp_idf_sys::{esp_err_t, esp_err_to_name};

pub fn esp_err_to_str(err: esp_err_t) -> &'static str {
    unsafe { CStr::from_ptr(esp_err_to_name(err)).to_str().unwrap() }
}
