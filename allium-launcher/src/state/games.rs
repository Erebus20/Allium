use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::{cmp::min, rc::Rc};

use anyhow::{anyhow, Result};
use embedded_graphics::{
    image::{Image, ImageRaw},
    prelude::*,
    primitives::Rectangle,
    text::Alignment,
};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::trace;

use common::constants::{
    ALLIUM_GAMES_DIR, BUTTON_DIAMETER, IMAGE_SIZE, LISTING_JUMP_SIZE, LISTING_SIZE,
    SELECTION_HEIGHT, SELECTION_MARGIN,
};
use common::database::Database;
use common::display::{color::Color, Display};
use common::platform::{DefaultPlatform, Key, KeyEvent, Platform};
use common::stylesheet::Stylesheet;

use crate::{
    command::AlliumCommand,
    devices::{DeviceMapper, Game},
    state::State,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GamesState {
    #[serde(skip)]
    entries: Vec<Entry>,
    stack: Vec<View>,
    #[serde(skip)]
    core_mapper: Option<Rc<DeviceMapper>>,
    #[serde(skip)]
    database: Database,
}

impl GamesState {
    pub fn new() -> Result<GamesState> {
        let directory = Directory::default();
        Ok(GamesState {
            entries: entries(&directory)?,
            stack: vec![View::new(directory, 0, 0)],
            core_mapper: None,
            database: Default::default(),
        })
    }

    pub fn init(&mut self, core_mapper: Rc<DeviceMapper>, database: Database) {
        self.core_mapper = Some(core_mapper);
        self.database = database;
    }

    fn view(&self) -> &View {
        self.stack.last().unwrap()
    }

    fn view_mut(&mut self) -> &mut View {
        self.stack.last_mut().unwrap()
    }

    fn push_directory(&mut self, directory: Directory) -> Result<()> {
        self.stack.push(View::new(directory, 0, 0));
        self.change_directory()?;
        Ok(())
    }

    fn pop_directory(&mut self) -> Result<()> {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
        self.change_directory()?;
        Ok(())
    }

    fn change_directory(&mut self) -> Result<()> {
        self.entries = entries(&self.view().directory)?;
        Ok(())
    }

    fn select_entry(&mut self, selected: i32) -> Result<Option<AlliumCommand>> {
        let entry = &mut self.entries[selected as usize];
        Ok(match entry {
            Entry::Directory(directory) => {
                let directory = directory.to_owned();
                self.push_directory(directory)?;
                None
            }
            Entry::Game(game) => self
                .core_mapper
                .as_ref()
                .unwrap()
                .launch_game(&self.database, game)?,
        })
    }
}

impl State for GamesState {
    fn enter(&mut self) -> Result<()> {
        self.change_directory()?;
        Ok(())
    }

    fn leave(&mut self) -> Result<()> {
        Ok(())
    }

    fn draw(
        &mut self,
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
                    324 + 12 * 2
                } else {
                    640 - 12 * 2
                },
                LISTING_SIZE as u32 * (SELECTION_HEIGHT + SELECTION_MARGIN),
            ),
        ))?;

        let selected = self.view().selected;
        for i in (self.view().top as usize)
            ..std::cmp::min(
                self.entries.len(),
                self.view().top as usize + LISTING_SIZE as usize,
            )
        {
            let entry = &mut self.entries[i];

            if selected == i as i32 {
                if styles.enable_box_art {
                    if let Some(image) = entry.image() {
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
                    entry.name(),
                    styles,
                    Alignment::Left,
                    if styles.enable_box_art { 324 } else { 592 },
                    true,
                    true,
                    0,
                )?;
            } else {
                display.draw_entry(
                    Point { x, y },
                    entry.name(),
                    styles,
                    Alignment::Left,
                    if styles.enable_box_art { 324 } else { 592 },
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

        display.load(Rectangle::new(
            Point::new(360, y),
            Size::new(width - 360, BUTTON_DIAMETER),
        ))?;

        x = display
            .draw_button_hint(Point::new(x, y), Key::A, "Start", styles, Alignment::Right)?
            .top_left
            .x
            - 18;
        display.draw_button_hint(Point::new(x, y), Key::B, "Back", styles, Alignment::Right)?;

        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) -> Result<(Option<AlliumCommand>, bool)> {
        Ok(match key_event {
            KeyEvent::Pressed(Key::A) => {
                let view = self.view();
                let entry = self.entries.get(view.selected as usize);
                if entry.is_some() {
                    (self.select_entry(view.selected)?, true)
                } else {
                    (None, false)
                }
            }
            KeyEvent::Pressed(Key::B) => {
                self.pop_directory()?;
                (None, true)
            }
            KeyEvent::Pressed(key) | KeyEvent::Autorepeat(key) => match key {
                Key::Up => {
                    let len = self.entries.len() as i32;
                    let view = self.view_mut();
                    view.selected = (view.selected - 1).rem_euclid(len);
                    if view.selected < view.top {
                        view.top = view.selected;
                    }
                    if view.selected - LISTING_SIZE >= view.top {
                        view.top = len - LISTING_SIZE;
                    }
                    trace!("selected: {}, top: {}", view.selected, view.top);
                    (None, true)
                }
                Key::Down => {
                    let len = self.entries.len() as i32;
                    let view = self.view_mut();
                    view.selected = (view.selected + 1).rem_euclid(len);
                    if view.selected < view.top {
                        view.top = 0;
                    }
                    if view.selected - LISTING_SIZE >= view.top {
                        view.top = view.selected - LISTING_SIZE + 1;
                    }
                    trace!("selected: {}, top: {}", view.selected, view.top);
                    (None, true)
                }
                Key::Left => {
                    let len = self.entries.len() as i32;
                    let view = self.view_mut();
                    view.selected = (view.selected - LISTING_JUMP_SIZE).clamp(0, len - 1);
                    if view.selected < view.top {
                        view.top = view.selected;
                    }
                    (None, true)
                }
                Key::Right => {
                    let len = self.entries.len() as i32;
                    let view = self.view_mut();
                    view.selected = (view.selected + LISTING_JUMP_SIZE).clamp(0, len - 1);
                    if view.selected - LISTING_SIZE >= view.top {
                        view.top = view.selected - LISTING_SIZE + 1;
                    }
                    (None, true)
                }
                _ => (None, false),
            },
            _ => (None, false),
        })
    }
}

pub fn entries(directory: &Directory) -> Result<Vec<Entry>> {
    let mut entries: Vec<_> = std::fs::read_dir(&directory.path)
        .map_err(|e| anyhow!("Failed to open directory: {:?}, {}", &directory.path, e))?
        .flat_map(|entry| entry.ok())
        .flat_map(|entry| match Entry::new(entry.path()) {
            Ok(Some(entry)) => Some(entry),
            _ => None,
        })
        .collect();
    entries.sort_unstable();
    Ok(entries)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Entry {
    Directory(Directory),
    Game(Game),
}

impl Entry {
    pub fn name(&self) -> &str {
        match self {
            Entry::Game(game) => &game.name,
            Entry::Directory(directory) => &directory.name,
        }
    }

    pub fn image(&mut self) -> Option<&Path> {
        match self {
            Entry::Game(game) => game.image(),
            Entry::Directory(_) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Directory {
    pub name: String,
    pub full_name: String,
    pub path: PathBuf,
}

impl Ord for Directory {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.full_name.cmp(&other.full_name)
    }
}

impl PartialOrd for Directory {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Default for Directory {
    fn default() -> Self {
        Directory {
            name: "Games".to_string(),
            full_name: "Games".to_string(),
            path: ALLIUM_GAMES_DIR.to_owned(),
        }
    }
}

const EXCLUDE_EXTENSIONS: [&str; 1] = ["db"];

impl Entry {
    fn new(path: PathBuf) -> Result<Option<Entry>> {
        // Don't add hidden files starting with .
        let file_name = match path.file_name().and_then(OsStr::to_str) {
            Some(file_name) => file_name,
            None => return Ok(None),
        };
        if file_name.starts_with('.') {
            return Ok(None);
        }

        let extension = path
            .extension()
            .and_then(OsStr::to_str)
            .unwrap_or_default()
            .to_owned();

        // Don't add images
        if file_name == "Imgs" {
            return Ok(None);
        }
        if EXCLUDE_EXTENSIONS.contains(&extension.as_str()) {
            return Ok(None);
        }

        let full_name = match path.file_stem().and_then(OsStr::to_str) {
            Some(name) => name.to_owned(),
            None => return Ok(None),
        };
        let mut name = full_name.clone();

        // Remove numbers
        lazy_static! {
            static ref NUMBERS_RE: Regex = Regex::new(r"^\d+").unwrap();
        }
        name = NUMBERS_RE.replace(&name, "").to_string();

        // Remove trailing parenthesis
        lazy_static! {
            static ref PARENTHESIS_RE: Regex = Regex::new(r"[\(\[].+[\)\]]$").unwrap();
        }
        name = PARENTHESIS_RE.replace(&name, "").to_string();

        // Trim whitespaces
        name = name.trim().to_owned();

        // Directories without extensions can be navigated into
        if extension.is_empty() && path.is_dir() {
            return Ok(Some(Entry::Directory(Directory {
                name,
                full_name,
                path,
            })));
        }

        Ok(Some(Entry::Game(Game::new(name, path))))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct View {
    top: i32,
    selected: i32,
    directory: Directory,
}

impl View {
    pub fn new(directory: Directory, top: i32, selected: i32) -> View {
        View {
            top,
            selected,
            directory,
        }
    }
}
