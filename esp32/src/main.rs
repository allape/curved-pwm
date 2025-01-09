use std::{
    ffi::CStr,
    fs, result,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use anyhow::{Ok, Result};
use esp_idf_hal::{gpio::PinDriver, io::Write};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{gpio, ledc, peripheral, prelude::Peripherals},
    http::{self, Method},
    ipv4,
    netif::{EspNetif, NetifConfiguration},
    nvs::EspDefaultNvsPartition,
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use esp_idf_sys::{
    esp_err_t, esp_err_to_name, esp_spiffs_check, esp_spiffs_format, esp_spiffs_info,
    esp_vfs_spiffs_conf_t, esp_vfs_spiffs_register, ESP_OK,
};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};

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

fn esp_err_to_str(err: esp_err_t) -> &'static str {
    unsafe { CStr::from_ptr(esp_err_to_name(err)).to_str().unwrap() }
}

fn connect_wifi(
    modem: impl peripheral::Peripheral<P = esp_idf_hal::modem::Modem> + 'static,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> Result<BlockingWifi<EspWifi<'static>>> {
    let wifi_sta_netif_key = format!("WIFI_STA_{:?}", CONFIG.device_name);
    let wifi_ap_netif_key = format!("WIFI_AP_{:?}", CONFIG.device_name);

    let mut inner_wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs))?;
    inner_wifi.swap_netif(
        EspNetif::new_with_conf(&NetifConfiguration {
            key: wifi_sta_netif_key.as_str().try_into().unwrap(),
            ip_configuration: Some(ipv4::Configuration::Client(
                ipv4::ClientConfiguration::DHCP(ipv4::DHCPClientSettings {
                    hostname: Some(CONFIG.device_name.try_into().unwrap()),
                }),
            )),
            ..NetifConfiguration::wifi_default_client()
        })?,
        EspNetif::new_with_conf(&NetifConfiguration {
            key: wifi_ap_netif_key.as_str().try_into().unwrap(),
            ..NetifConfiguration::wifi_default_router()
        })?,
    )?;

    let mut wifi = BlockingWifi::wrap(inner_wifi, sysloop)?;

    let wifi_configuration: Configuration = Configuration::Client(ClientConfiguration {
        ssid: CONFIG.wifi_ssid.try_into().unwrap(),
        bssid: None,
        auth_method: AuthMethod::WPA2Personal,
        password: CONFIG.wifi_psk.try_into().unwrap(),
        channel: None,
        ..Default::default()
    });

    info!("with ssid: {}", CONFIG.wifi_ssid);

    wifi.set_configuration(&wifi_configuration)?;

    info!("Wifi configuration set with ssid: {}", CONFIG.wifi_ssid);

    wifi.start()?;
    info!("Wifi started");

    wifi.connect()?;
    info!("Wifi connected");

    wifi.wait_netif_up()?;
    info!("Wifi netif up");

    Ok(wifi)
}

fn new_pwm_driver<Timer, Channel>(
    timer: impl peripheral::Peripheral<P = Timer> + 'static,
    channel: impl peripheral::Peripheral<P = Channel> + 'static,
    pin: impl peripheral::Peripheral<P = impl gpio::OutputPin> + 'static,
) -> Result<Arc<Mutex<ledc::LedcDriver<'static>>>>
where
    Timer: ledc::LedcTimer + 'static,
    Channel: ledc::LedcChannel<SpeedMode = Timer::SpeedMode>,
{
    let pwm_timer = ledc::LedcTimerDriver::new(timer, &ledc::config::TimerConfig::default())?;
    let pwm_driver = ledc::LedcDriver::new(channel, &pwm_timer, pin)?;

    Ok(Arc::new(Mutex::new(pwm_driver)))
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
                error!("Failed to initialize SPIFFS: {}", esp_err_to_str(res));
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
                error!("Failed to get SPIFFS info: {}", esp_err_to_str(res));
                return Ok(());
            }

            if used_bytes > total_bytes {
                warn!(
                    "Number of used bytes cannot be larger than total. Performing SPIFFS_check()."
                );
                let res = esp_spiffs_check(spiffs_config.partition_label);
                if res != ESP_OK {
                    error!("Failed to check SPIFFS: {}", esp_err_to_str(res));
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

    // connect to wifi
    let mut wifi = connect_wifi(peripherals.modem, sysloop, nvs)?;
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(10));

        match wifi.is_connected() {
            result::Result::Ok(true) => {
                continue;
            }
            _ => {
                info!("WiFi not connected, trying to connect");
            }
        }

        match wifi.connect() {
            result::Result::Ok(_) => {
                info!("WiFi connected");
            }
            Err(e) => {
                info!("WiFi connect error: {:?}", e);
                continue;
            }
        }

        match wifi.wait_netif_up() {
            result::Result::Ok(_) => {
                info!("WiFi netif up");
            }
            Err(e) => {
                info!("WiFi netif up error: {:?}", e);
                continue;
            }
        }
    });

    // pin drivers
    let mut reverse_pin = PinDriver::output(peripherals.pins.gpio5).unwrap();

    let led = new_pwm_driver(
        unsafe { ledc::TIMER0::new() },
        unsafe { ledc::CHANNEL0::new() },
        // peripherals.pins.gpio8,
        peripherals.pins.gpio3,
    )?;

    let pwm = new_pwm_driver(
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

    server.fn_handler(
        "/reformat",
        Method::Get,
        move |req| -> result::Result<(), anyhow::Error> {
            let res;
            unsafe {
                let config = new_spiffs_config();
                res = esp_spiffs_format(config.partition_label);
            }

            req.into_response(200, None, &[("Content-type", "text/plain; charset=UTF-8")])?
                .write_all(res.to_string().as_bytes())?;
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

    let max_duty = led.lock().unwrap().get_max_duty();
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
                if reverse_pin.is_set_low() {
                    reverse_pin.set_high().unwrap();
                }
            } else {
                if reverse_pin.is_set_high() {
                    reverse_pin.set_low().unwrap();
                }
            }

            led.lock()
                .unwrap()
                .set_duty(duty.try_into().unwrap())
                .unwrap();
            pwm.lock()
                .unwrap()
                .set_duty(duty.try_into().unwrap())
                .unwrap();

            // info!("duty: {:?}", duty);

            index += 1;
        }

        thread::sleep(Duration::from_millis(interval_));
    }
}
