use std::fs;

use anyhow::{anyhow, Ok, Result};
use esp_idf_svc::sys::{
    esp_spiffs_check, esp_spiffs_info, esp_vfs_spiffs_conf_t, esp_vfs_spiffs_register, ESP_OK,
};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};

use crate::esp32;

static FS_BASE_PATH: &str = "/spiffs\0";

/**
 * Diagram
 * interval_u64, pwm_i32 * n
 */
static CONFIG_FILE_NAME: &str = "/spiffs/config.bin";

#[derive(Serialize, Deserialize, Debug)]
pub struct PwmConfig {
    pub steps: Vec<i32>,
    pub interval: u64,
}

pub fn get_config() -> Result<Option<PwmConfig>> {
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

pub fn save_config(config: &PwmConfig) -> Result<()> {
    let mut buffer = vec![];
    buffer.extend_from_slice(&config.interval.to_be_bytes());

    for step in &config.steps {
        buffer.extend_from_slice(&step.to_be_bytes());
    }

    fs::write(CONFIG_FILE_NAME, &buffer)?;

    Ok(())
}

pub struct SpiffsConfig(esp_vfs_spiffs_conf_t);

impl Default for SpiffsConfig {
    fn default() -> Self {
        if !FS_BASE_PATH.ends_with("\0") {
            panic!("FS_BASE_PATH must end with null character");
        }

        let mut spiffs_config = esp_vfs_spiffs_conf_t::default();
        spiffs_config.base_path = FS_BASE_PATH.as_ptr() as *const i8;
        // spiffs_config.partition_label = "spiffs".as_ptr() as *const i8;
        spiffs_config.max_files = 2;
        spiffs_config.format_if_mount_failed = true;
        
        SpiffsConfig(spiffs_config)
    }
}

pub fn new() -> Result<()> {
    unsafe {
        let spiffs_config: SpiffsConfig = Default::default();
        let config = spiffs_config.0;

        let res = esp_vfs_spiffs_register(&config);
        if res == ESP_OK {
            info!("SPIFFS mounted");
        } else {
            error!(
                "Failed to initialize SPIFFS: {}",
                esp32::esp_err_to_str(res)
            );
            return Err(anyhow!("Failed to initialize SPIFFS"));
        }

        let mut total_bytes: usize = 0;
        let mut used_bytes: usize = 0;
        let res = esp_spiffs_info(config.partition_label, &mut total_bytes, &mut used_bytes);
        if res == ESP_OK {
            info!("SPIFFS: total: {}, used: {}", total_bytes, used_bytes);
        } else {
            error!("Failed to get SPIFFS info: {}", esp32::esp_err_to_str(res));
            return Err(anyhow!("Failed to get SPIFFS info"));
        }

        if used_bytes > total_bytes {
            warn!("Number of used bytes cannot be larger than total. Performing SPIFFS_check().");
            let res = esp_spiffs_check(config.partition_label);
            if res != ESP_OK {
                error!("Failed to check SPIFFS: {}", esp32::esp_err_to_str(res));
                return Err(anyhow!("Failed to check SPIFFS"));
            }
        }

        info!("SPIFFS initialized");
    };

    Ok(())
}
