use std::env;
use std::fs;

use anyhow::Result;
use common::battery::Battery;
use common::constants::{self, BATTERY_UPDATE_INTERVAL};
use common::display::Display;
use common::platform::{DefaultPlatform, Key, KeyEvent, Platform};
use common::stylesheet::Stylesheet;
use embedded_font::FontTextStyleBuilder;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Alignment, Text};
use lazy_static::lazy_static;

use crate::state::State;

pub struct AlliumMenu<P: Platform> {
    platform: P,
    display: P::Display,
    battery: P::Battery,
    styles: Stylesheet,
    state: State,
    dirty: bool,
    name: String,
}

impl AlliumMenu<DefaultPlatform> {
    pub fn new() -> Result<AlliumMenu<DefaultPlatform>> {
        let mut platform = DefaultPlatform::new()?;
        let display = platform.display()?;
        let battery = platform.battery()?;

        lazy_static! {
            static ref ALLIUM_GAME_INFO: String = env::var("ALLIUM_GAME_INFO")
                .unwrap_or_else(|_| constants::ALLIUM_GAME_INFO.to_string());
        }
        let game_info = fs::read_to_string(&*ALLIUM_GAME_INFO)?;
        let mut split = game_info.split('\n');
        let _ = split.next();
        let _ = split.next();
        let name = split.next().unwrap_or("").to_owned();

        Ok(AlliumMenu {
            platform,
            display,
            battery,
            styles: Default::default(),
            state: State::new()?,
            dirty: true,
            name,
        })
    }

    pub async fn run_event_loop(&mut self) -> Result<()> {
        self.display.darken()?;
        self.display.save()?;

        self.state.enter()?;

        let mut last_updated_battery = std::time::Instant::now();
        self.battery.update()?;

        loop {
            let now = std::time::Instant::now();

            // Update battery every 5 seconds
            if now.duration_since(last_updated_battery) > BATTERY_UPDATE_INTERVAL {
                self.battery.update()?;
                last_updated_battery = now;
                self.dirty = true;
            }

            self.state.update()?;

            if self.dirty {
                self.draw()?;
                self.state.draw(&mut self.display, &self.styles)?;
                self.display.flush()?;
                self.dirty = false;
            }

            self.dirty = match self.platform.poll().await? {
                Some(KeyEvent::Pressed(Key::L)) => {
                    if let Some(next_state) = self.state.prev()? {
                        self.state.leave()?;
                        self.state = next_state;
                        self.state.enter()?;
                    }
                    true
                }
                Some(KeyEvent::Pressed(Key::R)) => {
                    if let Some(next_state) = self.state.next()? {
                        self.state.leave()?;
                        self.state = next_state;
                        self.state.enter()?;
                    }
                    true
                }
                Some(key_event) => self.state.handle_key_event(key_event).await?,
                None => false,
            };
        }
    }

    fn draw(&mut self) -> Result<()> {
        let Size { width, height: _ } = self.display.size();

        let text_style = FontTextStyleBuilder::new(self.styles.ui_font.clone())
            .font_size(self.styles.ui_font_size)
            .text_color(self.styles.fg_color)
            .build();

        let primary_style = FontTextStyleBuilder::new(self.styles.ui_font.clone())
            .font_size(self.styles.ui_font_size)
            .text_color(self.styles.primary)
            .build();

        // Draw battery percentage
        if self.battery.charging() {
            Text::with_alignment(
                &format!("Charging: {}%", self.battery.percentage()),
                Point {
                    x: width as i32 - 8,
                    y: 8,
                },
                text_style,
                Alignment::Right,
            )
            .draw(&mut self.display)?;
        } else {
            Text::with_alignment(
                &format!("{}%", self.battery.percentage()),
                Point {
                    x: width as i32 - 8,
                    y: 8,
                },
                text_style,
                Alignment::Right,
            )
            .draw(&mut self.display)?;
        }

        // Draw game name
        let text = Text::with_alignment(
            &self.name,
            Point { x: 12, y: 8 },
            primary_style,
            Alignment::Left,
        );
        text.draw(&mut self.display)?;

        Ok(())
    }
}
