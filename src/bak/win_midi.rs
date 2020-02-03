use crate::win_midi_sys as sys;
use futures::channel::{mpsc, oneshot};
use futures::executor;
use futures::prelude::*;
use once_cell::sync::OnceCell;
use std::sync::Mutex;
use sys::MidiInHandle;
use thiserror::Error;
use winapi::shared::{basetsd, minwindef};

static INST_TX_MUTEX: OnceCell<Mutex<mpsc::UnboundedSender<MidiInst>>> = OnceCell::new();

thread_local! {
    pub static INST_TX_LOCAL: OnceCell<mpsc::UnboundedSender<MidiInst>> = OnceCell::new();
}

#[derive(Error, Debug)]
pub enum MidiError {
    #[error(transparent)]
    MMError(#[from] sys::MMError),
}

pub type MidiResult<T> = Result<T, MidiError>;

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

    pub async fn open(&self) -> MidiResult<InDev> {
        InDev::new(self.id as _).await
    }
}

pub fn enumerate_midi_out() -> impl Iterator<Item = sys::MidiOutCaps> {
    (0..sys::midi_out_count()).filter_map(|id| sys::midi_out_get_caps(id as _).ok())
}

impl sys::MidiOutCaps {
    pub async fn open(&self) -> MidiResult<OutDev> {
        OutDev::new(self.id as _).await
    }
}

type ChanReturn<T> = oneshot::Sender<MidiResult<T>>;

trait ChanReturnTrait<T> {
    fn ret(self, ret: Result<T, impl Into<MidiError>>);
}

impl<T> ChanReturnTrait<T> for ChanReturn<T> {
    fn ret(self, ret: Result<T, impl Into<MidiError>>) {
        if let Err(err) = self.send(ret.map_err(|err| err.into())) {
            err.unwrap();
        }
    }
}

#[derive(Debug)]
pub struct MidiMsg {
    pub num: basetsd::DWORD_PTR,
    pub msg: minwindef::UINT,
    pub param1: basetsd::DWORD_PTR,
    pub param2: basetsd::DWORD_PTR,
}

#[derive(Debug)]
pub enum MidiInst {
    InClose(usize),
    InMsg(MidiMsg),
    InOpen(
        ChanReturn<(usize, mpsc::UnboundedReceiver<MidiMsg>)>,
        minwindef::UINT,
    ),
    InReset(ChanReturn<()>, usize),
    InStart(ChanReturn<()>, usize),
    InStop(ChanReturn<()>, usize),
    OutClose(usize),
    OutOpen(ChanReturn<usize>, minwindef::UINT),
    OutReset(ChanReturn<()>, usize),
    OutSend(ChanReturn<()>, usize, minwindef::DWORD),
}

extern "C" fn midi_in_cb(
    _handle: MidiInHandle,
    msg: minwindef::UINT,
    num: basetsd::DWORD_PTR,
    param1: basetsd::DWORD_PTR,
    param2: basetsd::DWORD_PTR,
) {
    INST_TX_LOCAL.with(|midi_tx| {
        let sender = midi_tx.get_or_init(|| INST_TX_MUTEX.get().unwrap().lock().unwrap().clone());
        sender
            .unbounded_send(MidiInst::InMsg(MidiMsg {
                msg,
                num,
                param1,
                param2,
            }))
            .unwrap();
    });
}

fn init_midi() -> Mutex<mpsc::UnboundedSender<MidiInst>> {
    let (inst_tx, inst_rx) = mpsc::unbounded::<MidiInst>();
    std::thread::spawn(move || executor::block_on(midi_io(inst_rx)));
    Mutex::new(inst_tx)
}

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
        self.get(index).map(|opt| opt.as_ref()).flatten()
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

async fn midi_io(mut inst_rx: mpsc::UnboundedReceiver<MidiInst>) {
    let mut in_vec = Vec::<Option<(sys::MidiInHandle, mpsc::UnboundedSender<MidiMsg>)>>::new();
    let mut out_vec = Vec::<Option<sys::MidiOutHandle>>::new();

    while let Some(inst) = inst_rx.next().await {
        use MidiInst::*;
        match inst {
            InClose(num) => {
                if let Some(item) = in_vec.take_at(num) {
                    sys::midi_in_reset(item.0).unwrap();
                    sys::midi_in_close(item.0).unwrap();
                }
            }
            InMsg(msg) => {
                if let Some(item) = in_vec.get_inner(msg.num) {
                    item.1.unbounded_send(msg).unwrap();
                }
            }
            InOpen(res_tx, id) => res_tx.ret({
                let (msg_tx, msg_rx) = mpsc::unbounded::<MidiMsg>();
                sys::midi_in_open(id, in_vec.empty_index(), midi_in_cb)
                    .map(|handle| (in_vec.push_empty((handle, msg_tx)), msg_rx))
            }),
            InReset(res_tx, num) => {
                if let Some(item) = in_vec.get_inner(num) {
                    res_tx.ret(sys::midi_in_reset(item.0));
                }
            }
            InStart(res_tx, num) => {
                if let Some(item) = in_vec.get_inner(num) {
                    res_tx.ret(sys::midi_in_start(item.0));
                }
            }
            InStop(res_tx, num) => {
                if let Some(item) = in_vec.get_inner(num) {
                    res_tx.ret(sys::midi_in_stop(item.0));
                }
            }
            OutClose(num) => {
                if let Some(item) = out_vec.take_at(num) {
                    sys::midi_out_reset(item).unwrap();
                    sys::midi_out_close(item).unwrap();
                }
            }
            OutOpen(res_tx, id) => {
                res_tx.ret(sys::midi_out_open(id).map(|handle| out_vec.push_empty(handle)))
            }
            OutReset(res_tx, num) => {
                if let Some(item) = out_vec.get_inner(num) {
                    res_tx.ret(sys::midi_out_reset(*item));
                }
            }
            OutSend(res_tx, num, msg) => {
                if let Some(item) = out_vec.get_inner(num) {
                    res_tx.ret(sys::midi_out_msg(*item, msg));
                }
            }
        }
    }
}

pub struct InDev {
    inst_tx: mpsc::UnboundedSender<MidiInst>,
    num: usize,
    msg_rx: mpsc::UnboundedReceiver<MidiMsg>,
}

impl InDev {
    async fn new(id: minwindef::UINT) -> MidiResult<Self> {
        let inst_tx = INST_TX_MUTEX.get_or_init(init_midi).lock().unwrap().clone();
        let (res_tx, res_rx) =
            oneshot::channel::<MidiResult<(usize, mpsc::UnboundedReceiver<MidiMsg>)>>();
        inst_tx
            .unbounded_send(MidiInst::InOpen(res_tx, id))
            .unwrap();
        res_rx.await.unwrap().map(|(num, msg_rx)| Self {
            inst_tx,
            num,
            msg_rx,
        })
    }

    #[allow(dead_code)]
    pub async fn reset(&self) -> MidiResult<()> {
        let (res_tx, res_rx) = oneshot::channel::<MidiResult<()>>();
        self.inst_tx
            .unbounded_send(MidiInst::InReset(res_tx, self.num))
            .unwrap();
        res_rx.await.unwrap()
    }

    #[allow(dead_code)]
    pub async fn start(&self) -> MidiResult<()> {
        let (res_tx, res_rx) = oneshot::channel::<MidiResult<()>>();
        self.inst_tx
            .unbounded_send(MidiInst::InStart(res_tx, self.num))
            .unwrap();
        res_rx.await.unwrap()
    }

    #[allow(dead_code)]
    pub async fn stop(&self) -> MidiResult<()> {
        let (res_tx, res_rx) = oneshot::channel::<MidiResult<()>>();
        self.inst_tx
            .unbounded_send(MidiInst::InStop(res_tx, self.num))
            .unwrap();
        res_rx.await.unwrap()
    }

    pub fn msgs(&mut self) -> &mut mpsc::UnboundedReceiver<MidiMsg> {
        &mut self.msg_rx
    }
}

impl Drop for InDev {
    fn drop(&mut self) {
        self.inst_tx
            .unbounded_send(MidiInst::InClose(self.num))
            .unwrap();
    }
}

pub struct OutDev {
    inst_tx: mpsc::UnboundedSender<MidiInst>,
    num: usize,
}

impl OutDev {
    async fn new(id: minwindef::UINT) -> MidiResult<Self> {
        let inst_tx = INST_TX_MUTEX.get_or_init(init_midi).lock().unwrap().clone();
        let (res_tx, res_rx) = oneshot::channel::<MidiResult<usize>>();
        inst_tx
            .unbounded_send(MidiInst::OutOpen(res_tx, id))
            .unwrap();
        res_rx.await.unwrap().map(|num| Self { inst_tx, num })
    }

    #[allow(dead_code)]
    pub async fn reset(&self) -> MidiResult<()> {
        let (res_tx, res_rx) = oneshot::channel::<MidiResult<()>>();
        self.inst_tx
            .unbounded_send(MidiInst::OutReset(res_tx, self.num))
            .unwrap();
        res_rx.await.unwrap()
    }

    pub async fn send(&self, msg: u8, dw1: u8, dw2: u8) -> MidiResult<()> {
        let (res_tx, res_rx) = oneshot::channel::<MidiResult<()>>();
        let send = (msg as minwindef::DWORD)
            | ((dw1 as minwindef::DWORD) << 8)
            | ((dw2 as minwindef::DWORD) << 16);
        self.inst_tx
            .unbounded_send(MidiInst::OutSend(res_tx, self.num, send))
            .unwrap();
        res_rx.await.unwrap()
    }
}

impl Drop for OutDev {
    fn drop(&mut self) {
        self.inst_tx
            .unbounded_send(MidiInst::OutClose(self.num))
            .unwrap();
    }
}
