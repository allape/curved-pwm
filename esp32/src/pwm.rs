use anyhow::{Ok, Result};
use esp_idf_hal::{
    gpio::OutputPin,
    ledc::{config::TimerConfig, LedcChannel, LedcDriver, LedcTimer, LedcTimerDriver},
    peripheral::Peripheral,
    units::Hertz,
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
    let mut config = TimerConfig::default();
    config.frequency = Hertz(20_000);

    let pwm_timer = LedcTimerDriver::new(timer, &config)?;
    let pwm_driver = LedcDriver::new(channel, &pwm_timer, pin)?;

    Ok(pwm_driver)
}
