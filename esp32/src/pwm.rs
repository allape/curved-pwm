use anyhow::Result;
use esp_idf_svc::hal::{
    gpio::OutputPin,
    ledc::{config::TimerConfig, LedcChannel, LedcDriver, LedcTimer, LedcTimerDriver, Resolution},
    peripheral::Peripheral,
    units::{Hertz, KiloHertz},
};

pub fn new<'a, Timer, Channel>(
    timer: impl Peripheral<P = Timer> + 'a,
    channel: impl Peripheral<P = Channel> + 'a,
    pin: impl Peripheral<P = impl OutputPin> + 'a,
    frequency: Option<Hertz>,
    resolution: Option<Resolution>,
) -> Result<LedcDriver<'a>>
where
    Timer: LedcTimer + 'a,
    Channel: LedcChannel<SpeedMode = Timer::SpeedMode>,
{
    let mut config = TimerConfig::default();
    config.frequency = frequency.unwrap_or(Hertz(1000));
    config.resolution = resolution.unwrap_or(Resolution::Bits8);

    let timer_driver = LedcTimerDriver::new(timer, &config)?;
    let ledc_driver = LedcDriver::new(channel, &timer_driver, pin)?;

    Ok(ledc_driver)
}

pub fn new_20khz<'a, Timer, Channel>(
    timer: impl Peripheral<P = Timer> + 'a,
    channel: impl Peripheral<P = Channel> + 'a,
    pin: impl Peripheral<P = impl OutputPin> + 'a,
) -> Result<LedcDriver<'a>>
where
    Timer: LedcTimer + 'a,
    Channel: LedcChannel<SpeedMode = Timer::SpeedMode>,
{
    new(timer, channel, pin, Some(KiloHertz(20).into()), None)
}
