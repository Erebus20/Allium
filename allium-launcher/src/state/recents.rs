use std::{cmp::min, rc::Rc};

use anyhow::Result;
use embedded_graphics::{
    image::{Image, ImageRaw},
    prelude::*,
    primitives::Rectangle,
    text::Alignment,
};
use serde::{Deserialize, Serialize};

use common::{
    constants::{
        BUTTON_DIAMETER, IMAGE_SIZE, LISTING_JUMP_SIZE, LISTING_SIZE, RECENT_GAMES_LIMIT,
        SELECTION_HEIGHT, SELECTION_MARGIN,
    },
    display::{color::Color, Display},
    platform::Key,
    stylesheet::Stylesheet,
};
use common::{
    database::Database,
    platform::{DefaultPlatform, KeyEvent, Platform},
};
use tracing::trace;

use crate::{
    command::AlliumCommand,
    cores::{CoreMapper, Game},
    state::State,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentsState {
    top: i32,
    selected: i32,
    entries: Vec<Game>,
    #[serde(skip)]
    database: Database,
    #[serde(skip)]
    core_mapper: Option<Rc<CoreMapper>>,
}

impl RecentsState {
    pub fn new() -> Self {
        Self {
            top: 0,
            selected: 0,
            entries: vec![],
            database: Default::default(),
            core_mapper: None,
        }
    }

    pub fn init(&mut self, core_mapper: Rc<CoreMapper>, database: Database) {
        self.database = database;
        self.core_mapper = Some(core_mapper);
    }

    fn select_entry(&self, game: Game) -> Result<Option<AlliumCommand>> {
        self
            .core_mapper
            .as_ref()
            .unwrap()
            .launch_game(&self.database, &game)
    }
}

impl State for RecentsState {
    fn enter(&mut self) -> Result<()> {
        self.entries = self
            .database
            .select_last_played(RECENT_GAMES_LIMIT)?
            .into_iter()
            .map(|game| {
                let extension = game
                    .path
                    .extension()
                    .and_then(|p| p.to_str())
                    .unwrap_or("")
                    .to_owned();
                let full_name = game
                    .path
                    .file_stem()
                    .and_then(|p| p.to_str())
                    .unwrap_or("")
                    .to_owned();
                Game {
                    name: game.name,
                    path: game.path,
                    image: game.image,
                    extension,
                    full_name,
                }
            })
            .collect();
        Ok(())
    }

    fn leave(&mut self) -> Result<()> {
        Ok(())
    }

    fn draw(
        &self,
        display: &mut <DefaultPlatform as Platform>::Display,
        styles: &Stylesheet,
    ) -> Result<()> {
        let Size { width, height } = display.size();

        // Draw game list
        let (x, mut y) = (24, 58);

        // Clear previous selection
        display.load(Rectangle::new(
            Point::new(x - 12, y - 4),
            Size::new(
                if styles.enable_box_art {
                    300 + 12 * 2
                } else {
                    640 - 12 * 2
                },
                LISTING_SIZE as u32 * (SELECTION_HEIGHT + SELECTION_MARGIN),
            ),
        ))?;

        for i in (self.top as usize)
            ..std::cmp::min(
                self.entries.len(),
                self.top as usize + LISTING_SIZE as usize,
            )
        {
            let entry = &self.entries[i];

            if self.selected == i as i32 {
                if styles.enable_box_art {
                    if let Some(image) = &entry.image {
                        let mut image = image::open(image)?;
                        if image.width() != IMAGE_SIZE.width || image.height() > IMAGE_SIZE.height {
                            let new_height = min(
                                IMAGE_SIZE.height,
                                IMAGE_SIZE.width * image.height() / image.width(),
                            );
                            image = image.resize_to_fill(
                                IMAGE_SIZE.width,
                                new_height,
                                image::imageops::FilterType::Triangle,
                            );
                        }
                        display.load(Rectangle::new(
                            Point::new(
                                width as i32 - IMAGE_SIZE.width as i32 - 24,
                                54 + image.height() as i32,
                            ),
                            Size::new(IMAGE_SIZE.width, IMAGE_SIZE.height - image.height()),
                        ))?;

                        let mut image = image.to_rgb8();
                        common::display::image::round(
                            &mut image,
                            styles.background_color.into(),
                            12,
                        );
                        let image: ImageRaw<Color> = ImageRaw::new(&image, IMAGE_SIZE.width);
                        let image = Image::new(
                            &image,
                            Point::new(width as i32 - IMAGE_SIZE.width as i32 - 24, 54),
                        );
                        image.draw(display)?;
                    } else {
                        display.load(Rectangle::new(
                            Point::new(width as i32 - IMAGE_SIZE.width as i32 - 24, 54),
                            IMAGE_SIZE,
                        ))?;
                    }
                }

                display.draw_entry(
                    Point { x, y },
                    &entry.name,
                    styles,
                    Alignment::Left,
                    if styles.enable_box_art { 300 } else { 592 },
                    true,
                    true,
                    0,
                )?;
            } else {
                display.draw_entry(
                    Point { x, y },
                    &entry.name,
                    styles,
                    Alignment::Left,
                    if styles.enable_box_art { 300 } else { 592 },
                    false,
                    true,
                    0,
                )?;
            }
            y += (SELECTION_HEIGHT + SELECTION_MARGIN) as i32;
        }

        // Draw button hints
        let y = height as i32 - BUTTON_DIAMETER as i32 - 8;
        let mut x = width as i32 - 12;

        x = display
            .draw_button_hint(Point::new(x, y), Key::A, "Start", styles)?
            .top_left
            .x
            - 18;
        display.draw_button_hint(Point::new(x, y), Key::B, "Back", styles)?;

        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) -> Result<(Option<AlliumCommand>, bool)> {
        Ok(match key_event {
            KeyEvent::Pressed(Key::A) => {
                let entry = self.entries.get(self.selected as usize);
                if let Some(entry) = entry {
                    (self.select_entry(entry.to_owned())?, true)
                } else {
                    (None, false)
                }
            }
            KeyEvent::Pressed(key) | KeyEvent::Autorepeat(key) => match key {
                Key::Up => {
                    let len = self.entries.len() as i32;
                    self.selected = (self.selected - 1).rem_euclid(len);
                    if self.selected < self.top {
                        self.top = self.selected;
                    }
                    if self.selected - LISTING_SIZE >= self.top {
                        self.top = len - LISTING_SIZE;
                    }
                    trace!("selected: {}, top: {}", self.selected, self.top);
                    (None, true)
                }
                Key::Down => {
                    let len = self.entries.len() as i32;
                    self.selected = (self.selected + 1).rem_euclid(len);
                    if self.selected < self.top {
                        self.top = 0;
                    }
                    if self.selected - LISTING_SIZE >= self.top {
                        self.top = self.selected - LISTING_SIZE + 1;
                    }
                    trace!("selected: {}, top: {}", self.selected, self.top);
                    (None, true)
                }
                Key::Left => {
                    let len = self.entries.len() as i32;
                    self.selected = (self.selected - LISTING_JUMP_SIZE).clamp(0, len - 1);
                    if self.selected < self.top {
                        self.top = self.selected;
                    }
                    (None, true)
                }
                Key::Right => {
                    let len = self.entries.len() as i32;
                    self.selected = (self.selected + LISTING_JUMP_SIZE).clamp(0, len - 1);
                    if self.selected - LISTING_SIZE >= self.top {
                        self.top = self.selected - LISTING_SIZE + 1;
                    }
                    (None, true)
                }
                _ => (None, false),
            },
            _ => (None, false),
        })
    }
}
