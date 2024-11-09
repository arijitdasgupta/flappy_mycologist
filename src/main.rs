//! Blinks the LED on a Pico board
//!
//! This will blink an LED attached to GP25, which is the pin the Pico uses for the on-board LED.
#![no_std]
#![no_main]

mod end_screen;
mod game;
mod pause_screen;

use core::borrow::{Borrow, BorrowMut};
use core::cell::RefCell;

use bsp::entry;
use critical_section::Mutex;
use defmt::*;
use defmt_rtt as _;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::{DrawTarget, Point, Size};
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle, StyledDrawable};
use embedded_hal::digital::{InputPin, OutputPin};
use end_screen::EndScreen;
use fugit::RateExtU32;
use game::Game;
use heapless::spsc::Queue;
use panic_probe as _;

use pause_screen::PauseScreen;
use rp2040_hal::gpio::{FunctionI2C, Pin};
use rp2040_hal::rosc::RingOscillator;
use rp2040_hal::{self as hal, gpio};
// Provide an alias for our BSP so we can switch targets quickly.
// Uncomment the BSP you included in Cargo.toml, the rest of the code does not need to change.
use rp_pico as bsp;
// use sparkfun_pro_micro_rp2040 as bsp;

use bsp::hal::{
    clocks::{init_clocks_and_plls, Clock},
    gpio::Interrupt::EdgeLow,
    pac::{self, interrupt},
    sio::Sio,
    watchdog::Watchdog,
};
use ssd1306::mode::{BufferedGraphicsMode, DisplayConfig};
use ssd1306::prelude::{DisplayRotation, I2CInterface};
use ssd1306::size::DisplaySize128x64;
use ssd1306::{I2CDisplayInterface, Ssd1306};

type LeftBt = gpio::Pin<gpio::bank0::Gpio16, gpio::FunctionSioInput, gpio::PullUp>;
type CenterBt = gpio::Pin<gpio::bank0::Gpio17, gpio::FunctionSioInput, gpio::PullUp>;
type RightBt = gpio::Pin<gpio::bank0::Gpio18, gpio::FunctionSioInput, gpio::PullUp>;
type InterruptInputButtons<'a> = CenterBt;

pub type DisplayType = Ssd1306<
    I2CInterface<
        hal::I2C<
            pac::I2C1,
            (
                Pin<gpio::bank0::Gpio2, gpio::FunctionI2c, gpio::PullUp>,
                Pin<gpio::bank0::Gpio3, gpio::FunctionI2c, gpio::PullUp>,
            ),
        >,
    >,
    DisplaySize128x64,
    BufferedGraphicsMode<DisplaySize128x64>,
>;

#[derive(Eq, PartialEq)]
pub enum GameRunState {
    Running,
    Paused,
    GameOver,
}

pub trait Tick {
    // Returns true if game is over!
    fn tick(&mut self, frame_count: u32, random_byte: u8, random_bool: bool, booped: bool) -> bool;
}

pub trait Draw {
    fn draw_on_display(&self, display: &mut DisplayType);
}

static mut INPUT_Q: Queue<bool, 100> = Queue::new();

static INPUT_IRQ_SHARED: Mutex<RefCell<Option<InterruptInputButtons>>> =
    Mutex::new(RefCell::new(None));

#[entry]
fn main() -> ! {
    let mut pac = pac::Peripherals::take().unwrap();
    let core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let sio = Sio::new(pac.SIO);

    // External high-speed crystal on the pico board is 12Mhz
    let external_xtal_freq_hz = 12_000_000u32;
    let clocks = init_clocks_and_plls(
        external_xtal_freq_hz,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let mut delay = cortex_m::delay::Delay::new(core.SYST, clocks.system_clock.freq().to_Hz());

    // Ring oscillator
    let rosc = RingOscillator::new(pac.ROSC);
    let rosc = rosc.initialize_with_freq(500.kHz());

    let pins = bsp::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // Button interrupts
    let mut bt_left = pins.gpio16.into_pull_up_input();
    let bt_center = pins.gpio17.into_pull_up_input();
    let mut bt_right = pins.gpio18.into_pull_up_input();
    bt_center.set_interrupt_enabled(EdgeLow, true);
    // bt_right.set_interrupt_enabled(EdgeLow, true);

    // Configure two pins as being I²C, not GPIO
    let sda_pin: Pin<_, FunctionI2C, _> = pins.gpio2.reconfigure();
    let scl_pin: Pin<_, FunctionI2C, _> = pins.gpio3.reconfigure();

    // Initializing shared pins between main and interrupts
    critical_section::with(|cs| {
        INPUT_IRQ_SHARED.borrow(cs).replace(Some(bt_center));
    });

    // Initializing display interface, display & text style etc.
    let i2c = hal::I2C::i2c1(
        pac.I2C1,
        sda_pin,
        scl_pin, // Try `not_an_scl_pin` here
        1.MHz(),
        &mut pac.RESETS,
        &clocks.system_clock,
    );
    let interface = I2CDisplayInterface::new(i2c);
    let mut display = Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();

    display.init().unwrap();

    let mut game = Game::new();
    let pause_screen = PauseScreen::new();
    let end_screen = EndScreen::new();

    let (_, mut rx) = unsafe { INPUT_Q.split() };

    // Enabling interrupts
    unsafe {
        pac::NVIC::unmask(pac::Interrupt::IO_IRQ_BANK0);
    }

    let mut status_pin = pins.led.into_push_pull_output();
    status_pin.set_high().unwrap();
    let mut frame_count: u32 = 0;
    let mut game_run_state = GameRunState::Running;
    info!("core0 init successful");

    loop {
        // Process interrupt queue to determine if the pause button was pressed
        // If pause button was pressed, stop ticking
        let mut pause_button_pressed = false;
        while let Some(i) = rx.dequeue() {
            pause_button_pressed = i;
        }
        if pause_button_pressed && game_run_state != GameRunState::GameOver {
            if game_run_state == GameRunState::Running {
                game_run_state = GameRunState::Paused;
            } else {
                game_run_state = GameRunState::Running;
            }
        }

        if let GameRunState::Running = game_run_state {
            // Input
            let mut booped = false;
            if bt_left.is_low().unwrap() || bt_right.is_low().unwrap() {
                booped = true;
            }

            // Generate random byte
            let mut rando: u8 = 0;
            for _ in 0..8 {
                if rosc.get_random_bit() == true {
                    rando = (rando << 1) + 1;
                } else {
                    rando = rando << 1;
                }
            }
            let random_bool = rosc.get_random_bit();

            // Update game state
            let game_result = game.tick(frame_count, rando, random_bool, booped);
            if game_result {
                game_run_state = GameRunState::GameOver;
            }
            // Draw
            display.clear_buffer();
            game.draw_on_display(&mut display);
            display.flush().unwrap();

            // Update frame count
            frame_count += 1;
        } else {
            match game_run_state {
                GameRunState::Paused => {
                    pause_screen.draw_on_display(&mut display);
                }
                GameRunState::GameOver => {
                    end_screen.draw_on_display(&mut display);
                }
                // This part is unreachable.
                GameRunState::Running => {}
            }

            display.flush().unwrap();
            cortex_m::asm::wfi();
        }

        delay.delay_ms(40);
    }
}

// This interrupt is called from core1, handles inputs, sends to queue.
#[interrupt]
fn IO_IRQ_BANK0() {
    static mut INPUTS: Option<InterruptInputButtons> = None;

    let (mut tx, _) = unsafe { INPUT_Q.split() };

    if INPUTS.is_none() {
        critical_section::with(|cs| {
            *INPUTS = INPUT_IRQ_SHARED.borrow(cs).take();
        })
    }

    if let Some(stuff) = INPUTS {
        let center = stuff;

        if center.interrupt_status(EdgeLow) {
            let _ = tx.enqueue(true);
            center.clear_interrupt(EdgeLow);
        }
    }
}
