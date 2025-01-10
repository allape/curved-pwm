use anyhow::{Ok, Result};
use log::info;
use std::{
    thread::{self, JoinHandle},
    time::Duration,
};

use esp_idf_hal::peripheral;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    ipv4,
    netif::{EspNetif, NetifConfiguration},
    nvs::EspDefaultNvsPartition,
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};

pub fn setup(
    modem: impl peripheral::Peripheral<P = esp_idf_hal::modem::Modem> + 'static,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    device_name: &str,
    ssid: &str,
    psk: &str,
) -> Result<BlockingWifi<EspWifi<'static>>> {
    let wifi_sta_netif_key = format!("WIFI_STA_{:?}", device_name);
    let wifi_ap_netif_key = format!("WIFI_AP_{:?}", device_name);

    let mut inner_wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs))?;
    inner_wifi.swap_netif(
        EspNetif::new_with_conf(&NetifConfiguration {
            key: wifi_sta_netif_key.as_str().try_into().unwrap(),
            ip_configuration: Some(ipv4::Configuration::Client(
                ipv4::ClientConfiguration::DHCP(ipv4::DHCPClientSettings {
                    hostname: Some(device_name.try_into().unwrap()),
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
        ssid: ssid.try_into().unwrap(),
        bssid: None,
        auth_method: AuthMethod::WPA2Personal,
        password: psk.try_into().unwrap(),
        channel: None,
        ..Default::default()
    });

    wifi.set_configuration(&wifi_configuration)?;

    info!("Wifi configuration set with ssid: {}", ssid);

    wifi.start()?;
    info!("Wifi started");

    wifi.connect()?;
    info!("Wifi connected");

    wifi.wait_netif_up()?;
    info!("Wifi netif up");

    Ok(wifi)
}

pub fn guard(mut wifi: BlockingWifi<EspWifi<'static>>, duration: Duration) -> JoinHandle<()> {
    thread::spawn(move || loop {
        thread::sleep(duration);

        match wifi.is_connected() {
            Result::Ok(true) => {
                continue;
            }
            _ => {
                info!("WiFi not connected, trying to connect");
            }
        }

        match wifi.connect() {
            Result::Ok(_) => {
                info!("WiFi connected");
            }
            Err(e) => {
                info!("WiFi connect error: {:?}", e);
                continue;
            }
        }

        match wifi.wait_netif_up() {
            Result::Ok(_) => {
                info!("WiFi netif up");
            }
            Err(e) => {
                info!("WiFi netif up error: {:?}", e);
                continue;
            }
        }
    })
}
