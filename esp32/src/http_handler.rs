use std::sync::{Arc, Mutex};

use anyhow::Result;
use esp_idf_svc::{
    hal::io::Write,
    http::server::{EspHttpConnection, Request},
    sys::{
        soc_module_clk_t_SOC_MOD_CLK_XTAL, temperature_sensor_config_t, temperature_sensor_enable,
        temperature_sensor_get_celsius, temperature_sensor_handle_t, temperature_sensor_install,
        ESP_OK,
    },
};
use log::error;

use crate::esp32;

static INDEX_HTML_GZ: &[u8] = include_bytes!("./assets/index.html.gz");
static FAVICON_PNG: &[u8] = include_bytes!("./assets/fan.png");

pub fn new_error_handler<'a>(
    message: &'a str,
) -> impl Fn(Request<&mut EspHttpConnection<'_>>) -> Result<()> + 'a {
    move |req: Request<&mut EspHttpConnection<'_>>| -> Result<()> {
        req.into_response(500, None, &[])?
            .write_all(message.as_bytes())?;
        Ok(())
    }
}

pub fn handle_index(req: Request<&mut EspHttpConnection<'_>>) -> Result<()> {
    req.into_response(
        200,
        None,
        &[
            ("Content-Encoding", "gzip"),
            ("Content-type", "text/html; charset=UTF-8"),
            ("Cache-Control", "max-age=3600"),
        ],
    )?
    .write_all(INDEX_HTML_GZ)?;
    Ok(())
}

pub fn handle_favicon(req: Request<&mut EspHttpConnection<'_>>) -> Result<()> {
    req.into_response(
        200,
        None,
        &[
            ("Content-type", "image/png"),
            ("Cache-Control", "max-age=3600"),
        ],
    )?
    .write_all(FAVICON_PNG)?;
    Ok(())
}

struct WrapperType(temperature_sensor_handle_t);

unsafe impl Send for WrapperType {}
unsafe impl Sync for WrapperType {}

pub fn new_temperature_handler() -> impl Fn(Request<&mut EspHttpConnection<'_>>) -> Result<()> {
    let mut ok = true;
    let tsensor_handler: Arc<Mutex<WrapperType>> =
        Arc::new(Mutex::new(WrapperType(std::ptr::null_mut())));
    unsafe {
        let mut tsensor_config: temperature_sensor_config_t = Default::default();
        tsensor_config.range_min = 10;
        tsensor_config.range_max = 50;
        tsensor_config.clk_src = soc_module_clk_t_SOC_MOD_CLK_XTAL;

        let res =
            temperature_sensor_install(&tsensor_config, &mut tsensor_handler.lock().unwrap().0);
        if res != ESP_OK {
            ok = false;
            error!(
                "Failed to install temperature sensor: {}",
                esp32::esp_err_to_str(res)
            );
        }
        temperature_sensor_enable(tsensor_handler.lock().unwrap().0);
    }

    move |req: Request<&mut EspHttpConnection<'_>>| -> Result<()> {
        if !ok {
            return new_error_handler("Failed to install temperature sensor")(req);
        }

        let mut sensors = 0f32;
        unsafe {
            temperature_sensor_get_celsius(tsensor_handler.lock().unwrap().0, &mut sensors);
        };
        req.into_response(
            200,
            None,
            &[("Content-type", "application/json; charset=UTF-8")],
        )?
        .write_all(sensors.to_string().as_bytes())?;
        Ok(())
    }
}
