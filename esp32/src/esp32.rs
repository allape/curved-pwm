use anyhow::{anyhow, Result};
use std::ffi::CStr;

use esp_idf_svc::sys::{esp_err_t, esp_err_to_name};

pub fn esp_err_to_str(err: esp_err_t) -> &'static str {
    cstr_to_str(unsafe { esp_err_to_name(err) }).unwrap_or("Unknown ESP Error")
}

pub fn cstr_to_str(cstr: *const i8) -> Result<&'static str> {
    if cstr.is_null() {
        return Err(anyhow!("null"));
    }
    unsafe { Ok(CStr::from_ptr(cstr).to_str()?) }
}
