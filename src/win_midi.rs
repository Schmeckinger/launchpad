use crate::win_midi_sys as sys;
use std::sync::mpsc;
use sys::MidiResult;
use winapi::shared::{basetsd, minwindef};
use winapi::um::mmsystem;

pub fn enumerate_midi_in() -> impl Iterator<Item = sys::MidiInCaps> {
    (0..sys::midi_in_count()).filter_map(|id| sys::midi_in_get_caps(id as _).ok())
}

impl sys::MidiInCaps {
    pub fn matches(&self, out_caps: &sys::MidiOutCaps) -> bool {
        //todo: compare more indepth
        self.driver_ver == out_caps.driver_ver
            && self.mid == out_caps.mid
            && self.name == out_caps.name
            && self.pid == out_caps.pid
    }

    pub fn open(&self) -> MidiResult<InDev> {
        InDev::new(self.id as _)
    }
}

pub fn enumerate_midi_out() -> impl Iterator<Item = sys::MidiOutCaps> {
    (0..sys::midi_out_count()).filter_map(|id| sys::midi_out_get_caps(id as _).ok())
}

impl sys::MidiOutCaps {
    pub fn open(&self) -> MidiResult<OutDev> {
        OutDev::new(self.id as _)
    }
}

#[derive(Debug)]
pub struct MidiMsg {
    pub msg: minwindef::UINT,
    pub param1: basetsd::DWORD_PTR,
    pub param2: basetsd::DWORD_PTR,
}

type BoxedMsgTx = Box<mpsc::Sender<MidiMsg>>;

extern "C" fn midi_in_cb(
    _handle: mmsystem::HMIDIIN,
    msg: minwindef::UINT,
    inst: basetsd::DWORD_PTR,
    param1: basetsd::DWORD_PTR,
    param2: basetsd::DWORD_PTR,
) {
    let opt_sender: BoxedMsgTx = unsafe { Box::from_raw(inst as _) };
    match msg {
        mmsystem::MM_MIM_OPEN => {
            Box::leak(opt_sender);
        }
        mmsystem::MM_MIM_CLOSE => {
            std::mem::drop(opt_sender);
        }
        _ => {
            opt_sender
                .send(MidiMsg {
                    msg,
                    param1,
                    param2,
                })
                .unwrap();
            Box::leak(opt_sender);
        }
    }
}

trait OptVec<T> {
    fn empty_index(&self) -> usize;
    fn get_inner(&self, index: usize) -> Option<&T>;
    fn push_empty(&mut self, item: T) -> usize;
    fn take_at(&mut self, index: usize) -> Option<T>;
}

pub struct InDev {
    handle: sys::MidiInHandle,
    msg_rx: mpsc::Receiver<MidiMsg>,
}

impl InDev {
    fn new(id: minwindef::UINT) -> MidiResult<Self> {
        let (msg_tx, msg_rx) = mpsc::channel::<MidiMsg>();
        let boxed_tx: BoxedMsgTx = Box::new(msg_tx);
        Ok(Self {
            handle: sys::midi_in_open(id, Box::into_raw(boxed_tx) as _, midi_in_cb)?,
            msg_rx,
        })
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) -> MidiResult<()> {
        Ok(sys::midi_in_reset(&mut self.handle)?)
    }

    #[allow(dead_code)]
    pub fn start(&mut self) -> MidiResult<()> {
        Ok(sys::midi_in_start(&mut self.handle)?)
    }

    #[allow(dead_code)]
    pub fn stop(&mut self) -> MidiResult<()> {
        Ok(sys::midi_in_stop(&mut self.handle)?)
    }

    pub fn msgs(&self) -> impl Iterator<Item = MidiMsg> + '_ {
        self.msg_rx.iter()
    }
}

impl Drop for InDev {
    fn drop(&mut self) {
        sys::midi_in_reset(&mut self.handle).unwrap();
        sys::midi_in_close(&mut self.handle).unwrap();
    }
}

pub struct OutDev {
    handle: sys::MidiOutHandle,
}

impl OutDev {
    fn new(id: minwindef::UINT) -> MidiResult<Self> {
        Ok(Self {
            handle: sys::midi_out_open(id)?,
        })
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) -> MidiResult<()> {
        Ok(sys::midi_out_reset(&mut self.handle)?)
    }

    pub fn send(&mut self, msg: u8, dw1: u8, dw2: u8) -> MidiResult<()> {
        let send = (msg as minwindef::DWORD)
            | ((dw1 as minwindef::DWORD) << 8)
            | ((dw2 as minwindef::DWORD) << 16);
        Ok(sys::midi_out_msg(&mut self.handle, send)?)
    }
}

impl Drop for OutDev {
    fn drop(&mut self) {
        sys::midi_out_reset(&mut self.handle).unwrap();
        sys::midi_out_close(&mut self.handle).unwrap();
    }
}
