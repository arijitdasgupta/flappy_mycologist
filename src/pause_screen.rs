use embedded_graphics::{
    mono_font::{ascii::FONT_9X15, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::Point,
    text::Text,
    Drawable,
};

use crate::Draw;

pub struct PauseScreen<'a> {
    font: MonoTextStyle<'a, BinaryColor>,
}

impl PauseScreen<'_> {
    pub fn new() -> Self {
        return Self {
            font: MonoTextStyle::new(&FONT_9X15, BinaryColor::On),
        };
    }
}
impl Draw for PauseScreen<'_> {
    fn draw_on_display(&self, display: &mut crate::DisplayType) {
        let _ = Text::new("PAUSED", Point::new(30, 10), self.font).draw(display);
    }
}
