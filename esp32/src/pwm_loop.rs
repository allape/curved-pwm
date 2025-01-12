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

pub fn setup<ReversePin: OutputPin>(
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
