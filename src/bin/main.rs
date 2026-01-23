#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use core::cell::RefCell;
use core::ops::{BitAndAssign, BitOrAssign, DerefMut};

use critical_section::{Mutex};
use esp_hal::ledc::channel::ChannelIFace;
use esp_hal::ledc::timer::TimerIFace;
use esp_hal::ledc::timer::config::Duty;
use esp_hal::ledc::{Ledc, LowSpeed, channel, timer};
use esp_hal::{Blocking, time};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{DriveMode, Level, Output, OutputConfig};
use esp_hal::time::Duration;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{delay::Delay, handler, main, ram, rmt::Rmt, time::Rate};
use esp_hal::timer::{OneShotTimer};
use esp_hal_smartled::{SmartLedsAdapter, smart_led_buffer};
use log::info;
use smart_leds::{
    RGB8, SmartLedsWrite, brightness, gamma,
    hsv::{Hsv, hsv2rgb},
};


#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

static TIMER: Mutex<RefCell<Option<OneShotTimer<Blocking>>>> = Mutex::new(RefCell::new(None));
static LINE_SYNC: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));

#[main]
fn main() -> ! {
    // generator version: 1.0.1

    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let _peripherals = esp_hal::init(config);

    // Configure RMT (Remote Control Transceiver) peripheral globally
    // <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/peripherals/rmt.html>
    let rmt: Rmt<'_, esp_hal::Blocking> = Rmt::new(_peripherals.RMT, Rate::from_mhz(80)).expect("Failed to initialize RMT");

    // We use one of the RMT channels to instantiate a `SmartLedsAdapter` which can
    // be used directly with all `smart_led` implementations
    let rmt_channel = rmt.channel0;
    let mut rmt_buffer = smart_led_buffer!(1);

    // Each devkit uses a unique GPIO for the RGB LED, so in order to support
    // all chips we must unfortunately use `#[cfg]`s:
    let mut led = SmartLedsAdapter::new(rmt_channel, _peripherals.GPIO8, &mut rmt_buffer);

    // timing stuff
    let timg0 = TimerGroup::new(_peripherals.TIMG0);
    let mut one_shot = OneShotTimer::new(timg0.timer0);
    one_shot.set_interrupt_handler(timer_handler);
    critical_section::with(|cs| {
        TIMER.borrow_ref_mut(cs).replace(one_shot);
    });


    let mut x0_output = Output::new(_peripherals.GPIO18, Level::Low, OutputConfig::default());
    let mut y0_output = Output::new(_peripherals.GPIO19, Level::Low, OutputConfig::default());
    let example_led_output = Output::new(_peripherals.GPIO20, Level::Low, OutputConfig::default());
    let mut led_pwm = Ledc::new(_peripherals.LEDC);
    led_pwm.set_global_slow_clock(esp_hal::ledc::LSGlobalClkSource::APBClk);
    let mut led_timer = led_pwm.timer::<LowSpeed>(timer::Number::Timer1);
    let mut led_channel = led_pwm.channel::<LowSpeed>(channel::Number::Channel1, example_led_output);
    

    led_timer.configure(timer::config::Config {
        duty: Duty::Duty14Bit,
        clock_source: esp_hal::ledc::timer::LSClockSource::APBClk,
        frequency: time::Rate::from_hz(50),
    }).unwrap();

    led_channel.configure(channel::config::Config {
        timer: &led_timer,
        duty_pct: 0, // Start with 0% duty cycle (off)
        drive_mode: DriveMode::PushPull,
    }).unwrap();

    let delay = Delay::new();

    let mut color = Hsv {
        hue: 0,
        sat: 255,
        val: 255,
    };
    let mut data: RGB8;
    let level = 10;

    let triangle = [
        [0,255],
        [255,255],
        [255,0],
        [0,255]
    ];

    let num_drawn_lines = triangle.len() - 1;
    let drawing_time_ms: i32 = 2000;
    let line_time_ms = drawing_time_ms / (num_drawn_lines as i32);
    let mut line_index = 0;

    data = hsv2rgb(color);

    loop {
        info!("Traveling the triangle!");
        // Iterate over the rainbow!

        // draw the next line, this can take up to tau_s
        // if critical_section::with(|cs|{
        //     let is_line_trigger = *LINE_SYNC.borrow_ref(cs);
        //     is_line_trigger.then(||{
        //         (*LINE_SYNC.borrow_ref_mut(cs)).bitand_assign(false);
        //     });
        //     is_line_trigger
        // })
        // {
        //     x0_output.set_high();
        //     y0_output.set_high();
        //     delay.delay_nanos(3000);
        //     x0_output.set_low();
        //     y0_output.set_low();
        //     line_index += 1;
        // }

        // for line_index in 0..num_drawn_lines
        // {
        //     info!("Drawing line: {line_index}");
        //     for current_time in 0..=line_time_ms
        //     {
        //         let cur_x = triangle[line_index][0];
        //         let cur_y = triangle[line_index][1];
        //         let delta_x = triangle[line_index+1][0] - cur_x;
        //         let delta_y = triangle[line_index+1][1] - cur_y;
        //         let scalex_index = current_time * delta_x / line_time_ms;
        //         let scaley_index = current_time * delta_y / line_time_ms;
                
        //         data.r = 0u8;
        //         data.g = (cur_x + scalex_index) as u8;
        //         data.b = (cur_y + scaley_index) as u8;

        //         if current_time % 100 == 0
        //         {
        //             info!("Current RBG value {data}");
        //         }

        //         led.write(brightness(gamma([data].into_iter()), level)).unwrap();

        //         delay.delay_millis(20);
        //     }
            
        // }
    }
}

// maybe only use this as a signifier!
#[handler]
#[ram]
fn timer_handler() {
    let tau_sampling: u64 = 17;

    critical_section::with(|cs|   {
        // trigger a new frame
        (*LINE_SYNC.borrow_ref_mut(cs)).bitor_assign(true);
        let _ =TIMER.borrow_ref_mut(cs).as_mut().unwrap().schedule(Duration::from_micros(tau_sampling.div_ceil(1000)));
    });

}
