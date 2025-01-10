use std::{
    fs, result,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use anyhow::{Ok, Result};
use esp_idf_hal::{gpio::PinDriver, io::Write};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{ledc, prelude::Peripherals},
    http::{self, Method},
    nvs::EspDefaultNvsPartition,
};
use esp_idf_sys::{
    esp_spiffs_check, esp_spiffs_info, esp_vfs_spiffs_conf_t, esp_vfs_spiffs_register, ESP_OK,
};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};

mod esp32;
mod pwm;
mod wifi;

// FIXME
// can not extract spiffs-related code to a separate module,
// otherwise the error `ESP_ERR_INVALID_ARG` will be raised during spiffs registration.

const INDEX_HTML_GZ: &[u8] = include_bytes!("./index.html.gz");

const FS_BASE_PATH: &str = "/spiffs";

/**
 * Diagram
 * interval_u64, pwm_i32 * n
 */
const CONFIG_FILE_NAME: &str = "/spiffs/config.bin";

#[toml_cfg::toml_config]
pub struct Config {
    #[default("espressif")]
    device_name: &'static str,
    #[default("")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_psk: &'static str,
}

#[derive(Serialize, Deserialize, Debug)]
struct PwmConfig {
    steps: Vec<i32>,
    interval: u64,
}

fn read_from_file() -> Result<Option<PwmConfig>> {
    if !fs::exists(CONFIG_FILE_NAME)? {
        return Ok(None);
    }

    let config = fs::read(CONFIG_FILE_NAME)?;
    if config.len() < 8 || (config.len() - 8) % 4 != 0 {
        fs::remove_file(CONFIG_FILE_NAME)?;
        return Ok(None);
    }

    let interval = u64::from_be_bytes(config[0..8].try_into().unwrap());
    let mut steps = vec![];

    let mut index = 8;
    loop {
        if index >= config.len() {
            break;
        }

        steps.push(i32::from_be_bytes(
            config[index..index + 4].try_into().unwrap(),
        ));

        index += 4;
    }

    Ok(Some(PwmConfig { steps, interval }))
}

fn write_to_file(config: &PwmConfig) -> Result<()> {
    let mut buffer = vec![];
    buffer.extend_from_slice(&config.interval.to_be_bytes());

    for step in &config.steps {
        buffer.extend_from_slice(&step.to_be_bytes());
    }

    fs::write(CONFIG_FILE_NAME, &buffer)?;

    Ok(())
}

fn new_spiffs_config() -> esp_vfs_spiffs_conf_t {
    let mut spiffs_config = esp_vfs_spiffs_conf_t::default();
    spiffs_config.base_path = FS_BASE_PATH.as_ptr() as *const i8;
    // spiffs_config.partition_label = "spiffs".as_ptr() as *const i8;
    spiffs_config.max_files = 2;
    spiffs_config.format_if_mount_failed = true;
    spiffs_config
}

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let steps: Arc<Mutex<Vec<i32>>> = Arc::new(Mutex::new(vec![]));
    let interval: Arc<Mutex<u64>> = Arc::new(Mutex::new(100));

    // setup spiffs
    {
        let spiffs_config = new_spiffs_config();

        unsafe {
            let res = esp_vfs_spiffs_register(&spiffs_config);
            if res == ESP_OK {
                info!("SPIFFS mounted");
            } else {
                error!(
                    "Failed to initialize SPIFFS: {}",
                    esp32::esp_err_to_str(res)
                );
                return Ok(());
            }

            let mut total_bytes: usize = 0;
            let mut used_bytes: usize = 0;
            let res = esp_spiffs_info(
                spiffs_config.partition_label,
                &mut total_bytes,
                &mut used_bytes,
            );
            if res == ESP_OK {
                info!("SPIFFS: total: {}, used: {}", total_bytes, used_bytes);
            } else {
                error!("Failed to get SPIFFS info: {}", esp32::esp_err_to_str(res));
                return Ok(());
            }

            if used_bytes > total_bytes {
                warn!(
                    "Number of used bytes cannot be larger than total. Performing SPIFFS_check()."
                );
                let res = esp_spiffs_check(spiffs_config.partition_label);
                if res != ESP_OK {
                    error!("Failed to check SPIFFS: {}", esp32::esp_err_to_str(res));
                }
            }

            info!("SPIFFS initialized");
        };
    }

    // read saved config
    if let Some(pwm_config) = read_from_file()? {
        info!("read pwm config: {:?}", pwm_config);
        let mut steps = steps.lock().unwrap();
        *steps = pwm_config.steps.clone();
        let mut interval = interval.lock().unwrap();
        *interval = pwm_config.interval;
    } else {
        info!("no pwm config found");
    }

    let w = wifi::setup(
        peripherals.modem,
        sysloop,
        nvs,
        &CONFIG.device_name,
        &CONFIG.wifi_ssid,
        &CONFIG.wifi_psk,
    )?;
    wifi::guard(w, Duration::from_secs(10));

    let mut reverse = PinDriver::output(peripherals.pins.gpio5).unwrap();
    let mut led = pwm::new_driver(
        unsafe { ledc::TIMER0::new() },
        unsafe { ledc::CHANNEL0::new() },
        // peripherals.pins.gpio8,
        peripherals.pins.gpio3,
    )?;
    let mut output = pwm::new_driver(
        unsafe { ledc::TIMER0::new() },
        unsafe { ledc::CHANNEL0::new() },
        // peripherals.pins.gpio0,
        peripherals.pins.gpio4,
    )?;

    // http server
    let mut server = http::server::EspHttpServer::new(&http::server::Configuration::default())?;
    server.fn_handler(
        "/",
        Method::Get,
        move |req| -> result::Result<(), anyhow::Error> {
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
            result::Result::Ok(())
        },
    )?;

    let cloned_steps = Arc::clone(&steps);
    let cloned_interval = Arc::clone(&interval);
    server.fn_handler(
        "/pwm",
        Method::Post,
        move |mut req| -> result::Result<(), anyhow::Error> {
            let mut buffer = Vec::new();
            let mut temp_buffer = [0u8; 1024];

            loop {
                let bytes_read = req.read(&mut temp_buffer)?;
                if bytes_read == 0 {
                    break;
                }
                buffer.extend_from_slice(&temp_buffer[..bytes_read]);
            }

            let config: PwmConfig = serde_json::from_slice(&buffer.as_slice())?;

            info!("steps: {:?}", config.steps.clone());
            info!("interval: {:?}", config.interval.clone());

            let mut steps = cloned_steps.lock().unwrap();
            *steps = config.steps.clone();

            let mut interval = cloned_interval.lock().unwrap();
            *interval = config.interval;

            match write_to_file(&config) {
                result::Result::Ok(_) => {
                    info!("config saved");
                }
                Err(e) => {
                    info!("config save error: {:?}", e);
                }
            }

            req.into_response(200, None, &[("Content-type", "text/plain; charset=UTF-8")])?
                .write_all("ok".as_bytes())?;

            result::Result::Ok(())
        },
    )?;

    println!("ESP Started!");

    let mut index = 0;
    let mut interval_: u64;
    let mut duty: i32 = 0;

    let max_duty = led.get_max_duty();
    info!("max duty: {:?}", max_duty);

    loop {
        {
            interval_ = *interval.lock().unwrap();

            let steps_ = steps.lock().unwrap();

            if steps_.len() == 1 {
                duty = steps_[0];
            } else if steps_.len() > 1 {
                if index >= steps_.len() {
                    index = 0;
                }
                duty = steps_[index];
            }
        }

        {
            if duty < 0 {
                duty = -duty;
                if reverse.is_set_low() {
                    reverse.set_high().unwrap();
                }
            } else {
                if reverse.is_set_high() {
                    reverse.set_low().unwrap();
                }
            }

            led.set_duty(duty.try_into().unwrap()).unwrap();
            output.set_duty(duty.try_into().unwrap()).unwrap();

            // info!("duty: {:?}", duty);

            index += 1;
        }

        thread::sleep(Duration::from_millis(interval_));
    }
}
