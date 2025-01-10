use anyhow::{Ok, Result};
use esp_idf_hal::{
    gpio::OutputPin,
    ledc::{config::TimerConfig, LedcChannel, LedcDriver, LedcTimer, LedcTimerDriver},
    peripheral::Peripheral,
};

pub fn new_driver<Timer, Channel>(
    timer: impl Peripheral<P = Timer> + 'static,
    channel: impl Peripheral<P = Channel> + 'static,
    pin: impl Peripheral<P = impl OutputPin> + 'static,
) -> Result<LedcDriver<'static>>
where
    Timer: LedcTimer + 'static,
    Channel: LedcChannel<SpeedMode = Timer::SpeedMode>,
{
    let pwm_timer = LedcTimerDriver::new(timer, &TimerConfig::default())?;
    let pwm_driver = LedcDriver::new(channel, &pwm_timer, pin)?;

    Ok(pwm_driver)
}
