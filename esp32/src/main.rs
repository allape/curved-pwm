use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Result;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{gpio::PinDriver, io::Write, prelude::*},
    http::{server, Method},
    nvs::EspDefaultNvsPartition,
};
use log::{error, info};

mod esp32;
mod http_handler;
mod pwm;
mod storage;
mod wifi;

#[toml_cfg::toml_config]
pub struct Config {
    #[default("")]
    device_name: &'static str,
    #[default("")]
    wifi_ssid: &'static str,
    #[default("")]
    wifi_psk: &'static str,
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
    storage::new()?;

    // read saved config
    if let Some(pwm_config) = storage::get_config()? {
        info!("read pwm config: {:?}", pwm_config);
        let mut steps = steps.lock().unwrap();
        *steps = pwm_config.steps.clone();
        let mut interval = interval.lock().unwrap();
        *interval = pwm_config.interval;
    } else {
        info!("no pwm config found");
    }

    #[cfg(feature = "esp-c3-32s")]
    let pinner = main_loop::Pinner {
        direction: PinDriver::output(peripherals.pins.gpio5)?, // blue led
        led: pwm::new_20khz(
            peripherals.ledc.timer0,
            peripherals.ledc.channel0,
            peripherals.pins.gpio4, // green led
        )?,
        output: pwm::new_20khz(
            peripherals.ledc.timer1,
            peripherals.ledc.channel1,
            peripherals.pins.gpio3, // red led
        )?,
    };

    #[cfg(feature = "esp32-c3-supermini")]
    let pinner = main_loop::Pinner {
        reverse: PinDriver::output(peripherals.pins.gpio0)?,
        led: pwm::new_20khz(
            peripherals.ledc.timer0,
            peripherals.ledc.channel0,
            peripherals.pins.gpio8, // built-in led
        )?,
        output: pwm::new_20khz(
            peripherals.ledc.timer1,
            peripherals.ledc.channel1,
            peripherals.pins.gpio3,
        )?,
    };

    let pwm_loop_handler = main_loop::new(pinner, Arc::clone(&interval), Arc::clone(&steps));

    let w = wifi::new(
        peripherals.modem,
        sysloop,
        nvs,
        &CONFIG.device_name,
        &CONFIG.wifi_ssid,
        &CONFIG.wifi_psk,
    )?;
    wifi::guard(w, Duration::from_secs(10));

    // http server
    let mut server = server::EspHttpServer::new(&server::Configuration::default())?;
    server.fn_handler("/", Method::Get, http_handler::handle_index)?;
    server.fn_handler("/favicon.ico", Method::Get, http_handler::handle_favicon)?;
    server.fn_handler(
        "/sensors",
        Method::Get,
        http_handler::new_temperature_handler(),
    )?;

    let cloned_steps = Arc::clone(&steps);
    let cloned_interval = Arc::clone(&interval);
    server.fn_handler("/pwm", Method::Post, move |mut req| -> Result<()> {
        let size = req
            .header("Content-Length")
            .unwrap_or("0")
            .parse::<usize>()
            .unwrap_or(0);

        let mut buffer = if size > 0 {
            Vec::with_capacity(size)
        } else {
            Vec::new()
        };

        let mut temp_buffer = [0u8; 1024];

        loop {
            let bytes_read = req.read(&mut temp_buffer)?;
            if bytes_read == 0 {
                break;
            }
            buffer.extend_from_slice(&temp_buffer[..bytes_read]);
        }

        let config: storage::PwmConfig = serde_json::from_slice(&buffer.as_slice())?;

        info!("steps: {:?}", config.steps.clone());
        info!("interval: {:?}", config.interval.clone());

        let mut steps = cloned_steps.lock().unwrap();
        *steps = config.steps.clone();

        let mut interval = cloned_interval.lock().unwrap();
        *interval = config.interval;

        match storage::save_config(&config) {
            Result::Ok(_) => {
                info!("config saved");
            }
            Err(e) => {
                error!("config save error: {:?}", e);
            }
        }

        req.into_response(200, None, &[("Content-type", "text/plain; charset=UTF-8")])?
            .write_all("ok".as_bytes())?;

        Ok(())
    })?;

    println!("ESP Started!");

    pwm_loop_handler.join().unwrap();

    Ok(())
}

mod main_loop {
    use std::{
        sync::{Arc, Mutex},
        thread::{self, JoinHandle},
        time::Duration,
    };

    use esp_idf_svc::hal::{
        gpio::{Output, OutputPin, PinDriver},
        ledc::LedcDriver,
    };
    use log::info;

    pub struct Pinner<'a, ReversePin: OutputPin> {
        pub direction: PinDriver<'a, ReversePin, Output>,
        pub led: LedcDriver<'a>,
        pub output: LedcDriver<'a>,
    }

    pub fn new<ReversePin: OutputPin>(
        mut pinner: Pinner<'static, ReversePin>,
        interval: Arc<Mutex<u64>>,
        steps: Arc<Mutex<Vec<i32>>>,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
            let mut index = 0;
            let mut interval_: u64;
            let mut duty: i32 = 0;

            let max_duty = pinner.led.get_max_duty();
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
                        if pinner.direction.is_set_low() {
                            pinner.direction.set_high().unwrap();
                        }
                    } else {
                        if pinner.direction.is_set_high() {
                            pinner.direction.set_low().unwrap();
                        }
                    }

                    pinner.led.set_duty(duty.try_into().unwrap()).unwrap();
                    pinner.output.set_duty(duty.try_into().unwrap()).unwrap();

                    // info!("duty: {:?}", duty);

                    index += 1;
                }

                thread::sleep(Duration::from_millis(interval_));
            }
        })
    }
}
