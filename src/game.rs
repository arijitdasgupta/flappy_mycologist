use core::fmt::Write;
use core::ops::Neg;
use defmt::info;
use embedded_graphics::{
    image::Image,
    mono_font::{
        ascii::{FONT_6X9, FONT_9X15},
        iso_8859_1::FONT_5X7,
        MonoTextStyle,
    },
    pixelcolor::BinaryColor,
    prelude::{Point, Size},
    primitives::{Circle, PrimitiveStyle, Rectangle, StyledDrawable, Triangle},
    text::Text,
    Drawable,
};
use heapless::{String, Vec};
use tinybmp::Bmp;

use crate::{Draw, Tick};

const PLAYER_RADIUS_U: u32 = 5;
const PLAYER_RADIUS: i32 = 5;
const PLAYER_LEFT_PADDING: i32 = 15;

// GRAVITY BRUH!
const G: i32 = 1;
const BLOCK_WIDTH: u32 = 20;
const N_BLOCKS: i32 = 8;
const DEFAULT_BLOCK_HEIGHT: u32 = 20;
const BLOCK_OFFSET_VELOCITY: i32 = -1;

const SCREEN_HEIGHT: u32 = 64;
const SCREEN_WIDTH: u32 = 128;

pub struct Game<'a> {
    // Player
    player_position: i32,
    player_velocity: i32,
    player_acceleration: i32,

    // Blocks
    block_offset: i32,
    blocks: Vec<(u32, bool), 8>,

    // Game over
    // Once this is set, no further tick will be registered
    game_over: bool,

    // Score
    score: u32,

    // assets
    font: MonoTextStyle<'a, BinaryColor>,
    mushroom: Bmp<'a, BinaryColor>,
}

impl Game<'_> {
    pub fn new() -> Self {
        let mut block_heights: Vec<(u32, bool), 8> = Vec::new();
        for _ in 0..N_BLOCKS {
            let _ = block_heights.push((DEFAULT_BLOCK_HEIGHT, false));
        }

        let mushroom_sprite_data = include_bytes!("../assets/mushroom.bmp");
        let mushroom = Bmp::from_slice(mushroom_sprite_data).unwrap();

        Self {
            player_position: 5,
            player_velocity: 0,
            player_acceleration: 0,

            block_offset: 0,
            blocks: block_heights,

            game_over: false,
            score: 0,

            font: MonoTextStyle::new(&FONT_6X9, BinaryColor::On),
            mushroom,
        }
    }

    fn draw_player(&self, display: &mut crate::DisplayType) {
        let _ = Circle::new(
            Point::new(PLAYER_LEFT_PADDING, self.player_position),
            PLAYER_RADIUS_U * 2,
        )
        .draw_styled(&PrimitiveStyle::with_stroke(BinaryColor::On, 1), display);

        // Left eye
        let _ = Circle::new(
            Point::new(PLAYER_LEFT_PADDING + 3, self.player_position + 3),
            2,
        )
        .draw_styled(&PrimitiveStyle::with_fill(BinaryColor::On), display);

        // Right eye
        let _ = Circle::new(
            Point::new(PLAYER_LEFT_PADDING + 6, self.player_position + 3),
            2,
        )
        .draw_styled(&PrimitiveStyle::with_fill(BinaryColor::On), display);
    }

    fn draw_blocks(&self, display: &mut crate::DisplayType) {
        for (index, (block_height, is_mushroom)) in self.blocks.iter().enumerate() {
            // Could be optitmized more TODO
            let index: i32 = index as i32; // This is always less than 32
            let x = self.block_offset + index * (BLOCK_WIDTH as i32);
            let y = SCREEN_HEIGHT - block_height;
            let _ = Rectangle::new(
                Point::new(x, y as i32),
                Size::new(BLOCK_WIDTH - 2, *block_height),
            )
            .draw_styled(&PrimitiveStyle::with_fill(BinaryColor::On), display);

            if *is_mushroom {
                let _ = Image::new(&self.mushroom, Point::new(x, (y - 18) as i32)).draw(display);
            }
        }
    }

    fn draw_score(&self, display: &mut crate::DisplayType) {
        let mut score_string: String<4> = String::new();
        let _ = write!(score_string, "{:04}", self.score);
        let _ = Text::new(&score_string, Point::new(104, 9), self.font).draw(display);
    }
}

impl Tick for Game<'_> {
    fn tick(
        &mut self,
        _frame_count: u32,
        random_number: u8,
        random_bool: bool,
        booped: bool,
    ) -> bool {
        if booped {
            self.player_velocity = -4;
            self.player_acceleration = G;
        }

        // Check for mushroom collect
        let check_parameter = self.player_position + (PLAYER_RADIUS * 2);
        let check_threshold_y = SCREEN_HEIGHT - self.blocks[1].0 - 20;
        let is_mushroom = self.blocks[1].1;
        if is_mushroom && check_parameter > check_threshold_y as i32 {
            self.score += 10;
            self.blocks[1].1 = false;
        }

        // Checking for collision with blocks
        let check_parameter = self.player_position;
        let check_threshold = SCREEN_HEIGHT - self.blocks[1].0 - 10;
        if check_parameter > check_threshold as i32 {
            return true;
        }

        // Clamping how far the player can float up.
        let new_player_position = self.player_position + self.player_velocity;
        self.player_position = new_player_position.max(-100);

        // Updating velocity
        self.player_velocity += self.player_acceleration;

        // Infinite block generation method, when if reaches the end, we go around
        let new_block_offset = self.block_offset + BLOCK_OFFSET_VELOCITY;
        if new_block_offset < (BLOCK_WIDTH as i32).neg() {
            let new_block_height = random_number >> 3;
            for i in 0..self.blocks.len() - 1 {
                self.blocks[i] = self.blocks[i + 1];
            }
            let last_block_height = self.blocks.last_mut().unwrap();
            last_block_height.0 = new_block_height as u32;
            last_block_height.1 = random_bool;

            self.block_offset = new_block_offset % (BLOCK_WIDTH as i32);
        } else {
            self.block_offset = new_block_offset;
        }

        return false;
    }
}

impl Draw for Game<'_> {
    fn draw_on_display(&self, display: &mut crate::DisplayType) {
        self.draw_blocks(display);
        self.draw_player(display);
        self.draw_score(display);
    }
}
