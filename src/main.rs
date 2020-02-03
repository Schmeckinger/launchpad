mod launchpad;
mod win_midi;
mod win_midi_sys;

use crate::launchpad::{Color, Event};
use anyhow::Error;

fn main() -> Result<(), Error> {
    if let Some(unint_pad) = launchpad::enumerate_launchpads().next() {
        let (in_pad, mut out_pad) = unint_pad.init()?;
        out_pad.clear()?;

        for event in in_pad.msgs() {
            match dbg!(event) {
                Event::Down(pos) => out_pad.set_color(pos, Color::Red)?,
                Event::Up(pos) => out_pad.set_color(pos, Color::Green)?,
            }
        }
    }
    else {
        print!("No Launchpad found");
    }
    Ok(())
}