mod launchpad;
mod win_midi;
mod win_midi_sys;

use crate::launchpad::{Color, Event, LaunchpadIn, LaunchpadOutBuf};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::spawn;
use tungstenite::error::Error as WsError;
use tungstenite::{accept, Message};

trait OptVec<T> {
    fn empty_index(&self) -> usize;
    fn get_inner(&self, index: usize) -> Option<&T>;
    fn push_empty(&mut self, item: T) -> usize;
    fn take_at(&mut self, index: usize) -> Option<T>;
}

impl<T> OptVec<T> for Vec<Option<T>> {
    fn empty_index(&self) -> usize {
        self.iter()
            .position(|opt| opt.is_none())
            .unwrap_or_else(|| self.len())
    }

    fn get_inner(&self, index: usize) -> Option<&T> {
        self.get(index).and_then(|opt| opt.as_ref())
    }

    fn push_empty(&mut self, item: T) -> usize {
        if let Some((index, opt)) = self.iter_mut().enumerate().find(|opt| opt.1.is_none()) {
            opt.replace(item);
            index
        } else {
            self.push(Some(item));
            self.len() - 1
        }
    }

    fn take_at(&mut self, index: usize) -> Option<T> {
        self.get_mut(index)?.take()
    }
}

struct State {
    current: Option<u8>,
    out_pad: LaunchpadOutBuf,
    out_vec: Vec<Option<mpsc::Sender<String>>>,
}

impl State {
    fn new(out_pad: LaunchpadOutBuf) -> Self {
        Self {
            current: None,
            out_pad,
            out_vec: Vec::new(),
        }
    }
}

fn main() -> Result<(), anyhow::Error> {
    if let Some(uninit_pad) = launchpad::enumerate_launchpads().next() {
        let (in_pad, out_pad) = uninit_pad.init()?;
        let mut out_pad = out_pad.buf();
        out_pad.clear()?;
        out_pad.set_color((1, 8), Color::YELLOW)?;
        out_pad.set_color((2, 8), Color::ORANGE)?;
        out_pad.set_color((3, 8), Color::RED)?;
        out_pad.set_color((4, 8), Color::GREEN)?;

        let state = Arc::new(Mutex::new(State::new(out_pad)));

        let state_c = state.clone();
        spawn(move || pad_thread(in_pad, state_c));

        let server = TcpListener::bind("localhost:3012")?;
        for stream in server.incoming() {
            let stream = stream?;
            stream.set_read_timeout(Some(std::time::Duration::from_millis(100)))?;

            let state_c = state.clone();
            spawn(move || ws_thread(stream, state_c));
        }
    } else {
        print!("No Launchpad found");
    }
    Ok(())
}

fn ws_thread(stream: TcpStream, state_mutex: Arc<Mutex<State>>) {
    let (tx, rx) = mpsc::channel();
    let index = state_mutex.lock().unwrap().out_vec.push_empty(tx);
    let pos = index_to_pos(index as _);
    state_mutex.lock().unwrap().out_pad.set_color(pos, 0x11.into()).unwrap();

    let mut websocket = accept(stream).unwrap();
    'l: loop {
        match websocket.read_message() {
            Err(WsError::ConnectionClosed) | Err(WsError::AlreadyClosed) => break 'l,
            Ok(Message::Text(msg)) => {
                match &msg[..] {
                    "1" => {
                        let mut state = state_mutex.lock().unwrap();
                        if state.current == Some(index as _) {
                            state.out_pad.set_color(pos, Color::GREEN).unwrap();
                            state.out_pad.set_color((0, 8), Color::GREEN).unwrap();
                        } else {
                            state.out_pad.set_color(pos, 0x10.into()).unwrap();
                        }
                    }
                    "2" => {
                        let mut state = state_mutex.lock().unwrap();
                        if state.current == Some(index as _) {
                            state.out_pad.set_color(pos, Color::RED).unwrap();
                            state.out_pad.set_color((0, 8), Color::RED).unwrap();
                        } else {
                            state.out_pad.set_color(pos, 0x01.into()).unwrap();
                        }
                    }
                    _ => (),
                }
            }
            _ => (),
        }

        for string in rx.try_iter() {
            if websocket.write_message(Message::Text(string)).is_err() {
                break 'l;
            }
        }
    }

    let mut state = state_mutex.lock().unwrap();
    state.out_pad.set_color(pos, Color::BLACK).unwrap();
    state.out_vec.take_at(index);
}

fn pad_thread(mut in_pad: LaunchpadIn, state_mutex: Arc<Mutex<State>>) {
    let msgs = in_pad.msgs();
    for event in msgs {
        match event {
            Event::Down((x, y @ 1..=7)) => {
                let mut state = state_mutex.lock().unwrap();
                let index = pos_to_index((x, y));
                if state.current != Some(index) && state.out_vec.get(index as usize).is_some() {
                    state.current.map(|current| {
                        let pos = index_to_pos(current);
                        let col = (u8::from(state.out_pad.get_color(pos)) / 3).into();
                        state.out_pad.set_color(pos, col).unwrap();
                    });

                    let pos = (x, y);
                    let col = (u8::from(state.out_pad.get_color(pos)) * 3).into();
                    state.out_pad.set_color(pos, col).unwrap();
                    state.out_pad.set_color((0, 8), col).unwrap();

                    state.current = Some(index);
                }
            }
            Event::Down((x @ 0..=4, 8)) => {
                let state = state_mutex.lock().unwrap();
                if let Some(tx) = state
                    .current
                    .and_then(|c| state.out_vec.get_inner(c as usize))
                {
                    tx.send(x.to_string()).unwrap();
                }
            }
            _ => (),
        }
    }
}

fn index_to_pos(index: u8) -> (u8, u8) {
    (index % 8, (index / 8) + 1)
}

fn pos_to_index(pos: (u8, u8)) -> u8 {
    pos.0 + ((pos.1 - 1) * 8)
}