use crate::win_midi as midi;
use crate::win_midi_sys as sys;
use thiserror::Error;
use winapi::um::mmsystem::MM_MIM_DATA as IN_DATA;

#[derive(Error, Debug)]
pub enum LaunchpadError {
    #[error(transparent)]
    MidiError(#[from] sys::MidiError),
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
    pub fn init(&self) -> LaunchpadResult<(LaunchpadIn, LaunchpadOut)> {
        Ok((
            LaunchpadIn::new(self.in_caps.open()?)?,
            LaunchpadOut::new(self.out_caps.open()?),
        ))
    }

    #[allow(dead_code)]
    pub fn name(&self) -> &str {
        &self.in_caps.name
    }
}

pub struct LaunchpadIn {
    in_dev: midi::InDev,
}

impl LaunchpadIn {
    fn new(mut in_dev: midi::InDev) -> LaunchpadResult<Self> {
        in_dev.start()?;
        Ok(Self { in_dev })
    }

    #[allow(dead_code)]
    pub fn current_msgs(&mut self) -> impl Iterator<Item = Event> + '_ {
        Self::map_midi_msgs(self.in_dev.current_msgs())
    }

    #[allow(dead_code)]
    pub fn msgs(&mut self) -> impl Iterator<Item = Event> + '_ {
        Self::map_midi_msgs(self.in_dev.msgs())
    }

    fn map_midi_msgs<'a, T>(msgs: T) -> impl Iterator<Item = Event> + 'a
    where
        T: Iterator<Item = midi::MidiMsg> + 'a,
    {
        msgs.filter_map(|msg| match (msg.msg, (msg.param1 as u32).to_le_bytes()) {
            (IN_DATA, [0x90, pos, 0x0, _]) => Some(Event::Up((pos & 0xF, pos / 16 + 1))),
            (IN_DATA, [0x90, pos, 0x7F, _]) => Some(Event::Down((pos & 0xF, pos / 16 + 1))),
            (IN_DATA, [0xB0, pos, 0x0, _]) => Some(Event::Up((pos & 0x7, 0))),
            (IN_DATA, [0xB0, pos, 0x7F, _]) => Some(Event::Down((pos & 0x7, 0))),
            _ => None,
        })
    }
}

pub struct LaunchpadOut {
    out_dev: midi::OutDev,
}

impl LaunchpadOut {
    // TODO: Add more functions
    fn new(out_dev: midi::OutDev) -> Self {
        Self { out_dev }
    }

    pub fn buf(self) -> LaunchpadOutBuf {
        LaunchpadOutBuf::new(self)
    }

    pub fn clear(&mut self) -> LaunchpadResult<()> {
        self.out_dev.send(0xb0, 0x0, 0x0).map_err(|err| err.into())
    }

    //    pub fn fast(&self, col1: LaunchpadColor, col2: LaunchpadColor) -> MidiResult<()> {
    //        self.out_dev.send(0x92, col1.into(), col2.into())
    //    }

    pub fn set_color(&mut self, pos: (u8, u8), col: Color) -> LaunchpadResult<()> {
        match pos {
            (0..=7, 0) => self
                .out_dev
                .send(0xB0, pos.0 | 0x68, col.into())
                .map_err(|err| err.into()),
            (8, 0) => Ok(()),
            (0..=8, 1..=8) => self
                .out_dev
                .send(0x90, (pos.1 - 1) * 16 + pos.0, col.into())
                .map_err(|err| err.into()),
            _ => Err(LaunchpadError::OutOfRange(pos.0, pos.1)),
        }
    }
}

pub struct LaunchpadOutBuf {
    colors: Vec<u8>,
    out_pad: LaunchpadOut,
}

impl LaunchpadOutBuf {
    fn new(out_pad: LaunchpadOut) -> Self {
        Self {
            colors: vec![0; 81],
            out_pad,
        }
    }

    pub fn clear(&mut self) -> LaunchpadResult<()> {
        self.out_pad.clear().map(|ret| {
            self.colors.iter_mut().for_each(|x| *x = 0);
            ret
        })
    }

    pub fn get_color(&self, pos: (u8, u8)) -> Color {
        self.colors[pos.0 as usize + (pos.1 as usize * 9)].into()
    }

    pub fn set_color(&mut self, pos: (u8, u8), col: Color) -> LaunchpadResult<()> {
        self.out_pad.set_color(pos, col).map(|ret| {
            self.colors[pos.0 as usize + (pos.1 as usize * 9)] = col.into();
            ret
        })
    }
}

#[derive(Debug)]
pub enum Event {
    Up((u8, u8)),
    Down((u8, u8)),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color(u8);

impl Color {
    pub fn new(val: u8) -> Self {
        Self(val & 0x33)
    }
}

impl Color {
    pub const BLACK: Self = Self(0x00);
    pub const GREEN: Self = Self(0x30);
    pub const ORANGE: Self = Self(0x33);
    pub const RED: Self = Self(0x03);
    pub const YELLOW: Self = Self(0x31);
}

impl From<u8> for Color {
    fn from(col: u8) -> Self {
        Self::new(col)
    }
}

impl From<(u8, u8)> for Color {
    fn from(col: (u8, u8)) -> Self {
        Self::new((col.0 & 0x3) | ((col.1 & 0x3) << 4))
    }
}

//impl PartialEq for Color {
//    fn eq(&self, other: &Self) -> bool {
//        u8::from(*self) == u8::from(*other)
//    }
//}

impl From<Color> for u8 {
    fn from(col: Color) -> Self {
        col.0
    }
}
