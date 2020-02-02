use crate::win_midi as midi;
use crate::win_midi_sys as sys;
use futures::prelude::*;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LaunchpadError {
    #[error(transparent)]
    MidiError(#[from] midi::MidiError),
    #[error("Position ({0}, {1}) is out of range")]
    OutOfRange(u8, u8),
}

pub type LaunchpadResult<T> = Result<T, LaunchpadError>;

pub fn enumerate_launchpads() -> impl Iterator<Item = UninitLaunchpad> {
    midi::enumerate_midi_in().filter_map(|in_caps| {
        if !in_caps.name.contains("Launchpad") {
            return None;
        }

        midi::enumerate_midi_out()
            .find(|out_caps| in_caps.matches(out_caps))
            .map(|out_caps| UninitLaunchpad { in_caps, out_caps })
    })
}

pub struct UninitLaunchpad {
    in_caps: sys::MidiInCaps,
    out_caps: sys::MidiOutCaps,
}

impl UninitLaunchpad {
    pub async fn init(&self) -> LaunchpadResult<(LaunchpadIn, LaunchpadOut)> {
        let in_dev = self.in_caps.open().await?;
        in_dev.start().await?;
        Ok((
            LaunchpadIn(in_dev),
            LaunchpadOut(self.out_caps.open().await?),
        ))
    }

    #[allow(dead_code)]
    pub fn name(&self) -> &str {
        &self.in_caps.name
    }
}

pub struct LaunchpadIn(midi::InDev);

impl LaunchpadIn {
    pub fn msgs(&mut self) -> impl Stream<Item = Event> + '_ {
        self.0.msgs().filter_map(|msg| async move {
            match (msg.msg, (msg.param1 as u32).to_le_bytes()) {
                (0x3C3, [0x90, pos, 0x0, _]) => Some(Event::Up((pos & 0xF, pos / 16 + 1))),
                (0x3C3, [0x90, pos, 0x7F, _]) => Some(Event::Down((pos & 0xF, pos / 16 + 1))),
                (0x3C3, [0xB0, pos, 0x0, _]) => Some(Event::Up((pos & 0x7, 0))),
                (0x3C3, [0xB0, pos, 0x7F, _]) => Some(Event::Down((pos & 0x7, 0))),
                _ => None,
            }
        })
    }
}

pub struct LaunchpadOut(midi::OutDev);

impl LaunchpadOut {
    // TODO: Add more functions
    pub async fn clear(&self) -> LaunchpadResult<()> {
        self.0.send(0xb0, 0x0, 0x0).await.map_err(|err| err.into())
    }

    //    pub fn fast(&self, col1: LaunchpadColor, col2: LaunchpadColor) -> MidiResult<()> {
    //        self.out_dev.send(0x92, col1.into(), col2.into())
    //    }

    pub async fn set_color(&self, pos: (u8, u8), col: Color) -> LaunchpadResult<()> {
        match pos {
            (0..=7, 0) => self
                .0
                .send(0xB0, pos.0 | 0x68, col.into())
                .await
                .map_err(|err| err.into()),
            (8, 0) => Ok(()),
            (0..=8, 1..=8) => self
                .0
                .send(0x90, (pos.1 - 1) * 16 + pos.0, col.into())
                .await
                .map_err(|err| err.into()),
            _ => Err(LaunchpadError::OutOfRange(pos.0, pos.1)),
        }
    }
}

#[derive(Debug)]
pub enum Event {
    Up((u8, u8)),
    Down((u8, u8)),
}

#[derive(Debug)]
pub enum Color {
    Black,
    Green,
    Red,
    Yellow,
    Custom(u8),
}

impl From<u8> for Color {
    fn from(col: u8) -> Self {
        Self::Custom(col)
    }
}

impl From<(u8, u8)> for Color {
    fn from(col: (u8, u8)) -> Self {
        Self::Custom((col.0 & 0x3) | (col.1 << 4))
    }
}

impl From<Color> for u8 {
    fn from(col: Color) -> Self {
        match col {
            Color::Black => 0x00,
            Color::Green => 0x30,
            Color::Red => 0x03,
            Color::Yellow => 0x33,
            Color::Custom(val) => val & 0x33,
        }
    }
}
