mod launchpad;
mod win_midi;
mod win_midi_sys;

use crate::launchpad::{Color, Event};
use anyhow::Error;
use futures::executor;
use futures::prelude::*;

fn main() -> Result<(), Error> {
    executor::block_on(async_main())
}

async fn async_main() -> Result<(), Error> {
    if let Some(unint_pad) = launchpad::enumerate_launchpads().next() {
        let (mut in_pad, out_pad) = unint_pad.init().await?;
        out_pad.clear().await?;
        let mut msgs = Box::pin(in_pad.msgs());
        while let Some(event) = msgs.next().await {
            match dbg!(event) {
                Event::Down(pos) => out_pad.set_color(pos, Color::Red).await?,
                Event::Up(pos) => out_pad.set_color(pos, Color::Green).await?,
            }
        }
    }
    else {
        print!("No Launchpad found");
    }
    Ok(())
}
