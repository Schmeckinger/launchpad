use std::os::windows::ffi::OsStringExt;
use std::sync::atomic::AtomicPtr;
use std::{ffi, mem};
use thiserror::Error;
use winapi::shared::{basetsd, minwindef, ntdef};
use winapi::um::{mmeapi, mmsystem};

//TODO: Text
#[derive(Error, Debug)]
#[error("MidiError({0})")]
pub struct MidiError(mmsystem::MMRESULT);

pub type MidiResult<T> = Result<T, MidiError>;

pub fn midi_in_count() -> minwindef::UINT {
    unsafe { mmeapi::midiInGetNumDevs() }
}

#[derive(Debug)]
pub struct MidiInCaps {
    pub driver_ver: mmsystem::MMVERSION,
    pub id: basetsd::UINT_PTR,
    pub mid: minwindef::WORD,
    pub name: String,
    pub pid: minwindef::WORD,
}

pub fn midi_in_get_caps(id: basetsd::UINT_PTR) -> MidiResult<MidiInCaps> {
    let mut caps = mem::MaybeUninit::<mmsystem::MIDIINCAPSW>::zeroed();

    map_mmresult(
        unsafe {
            mmeapi::midiInGetDevCapsW(
                id,
                caps.as_mut_ptr(),
                mem::size_of::<mmsystem::MIDIINCAPSW>() as _,
            )
        },
        || {
            let caps = unsafe { caps.assume_init() };
            MidiInCaps {
                driver_ver: caps.vDriverVersion,
                id,
                mid: caps.wMid,
                name: wchar_to_string(unsafe { &caps.szPname }),
                pid: caps.wPid,
            }
        },
    )
}

pub type MidiInHandle = AtomicPtr<mmsystem::HMIDIIN__>;

pub type MidiInCb = extern "C" fn(
    mmsystem::HMIDIIN,
    minwindef::UINT,
    basetsd::DWORD_PTR,
    basetsd::DWORD_PTR,
    basetsd::DWORD_PTR,
);

pub fn midi_in_open(
    id: minwindef::UINT,
    num: basetsd::DWORD_PTR,
    midi_in_cb: MidiInCb,
) -> MidiResult<MidiInHandle> {
    let mut dev = mem::MaybeUninit::<mmsystem::HMIDIIN>::zeroed();

    map_mmresult(
        unsafe {
            mmeapi::midiInOpen(
                dev.as_mut_ptr() as _,
                id,
                midi_in_cb as _,
                num,
                mmsystem::CALLBACK_FUNCTION,
            )
        },
        || MidiInHandle::new(unsafe { dev.assume_init() }),
    )
}

pub fn midi_in_close(handle: &mut MidiInHandle) -> MidiResult<()> {
    mmresult(unsafe { mmeapi::midiInClose(*(handle.get_mut())) })
}

pub fn midi_in_reset(handle: &mut MidiInHandle) -> MidiResult<()> {
    mmresult(unsafe { mmeapi::midiInReset(*(handle.get_mut())) })
}

pub fn midi_in_start(handle: &mut MidiInHandle) -> MidiResult<()> {
    mmresult(unsafe { mmeapi::midiInStart(*(handle.get_mut())) })
}

pub fn midi_in_stop(handle: &mut MidiInHandle) -> MidiResult<()> {
    mmresult(unsafe { mmeapi::midiInStop(*(handle.get_mut())) })
}

pub fn midi_out_count() -> minwindef::UINT {
    unsafe { mmeapi::midiOutGetNumDevs() }
}

#[derive(Debug)]
pub struct MidiOutCaps {
    pub chan_mask: minwindef::WORD,
    pub driver_ver: mmsystem::MMVERSION,
    pub id: basetsd::UINT_PTR,
    pub mid: minwindef::WORD,
    pub name: String,
    pub notes: minwindef::WORD,
    pub pid: minwindef::WORD,
    pub support: minwindef::DWORD,
    pub tech: minwindef::WORD,
    pub voices: minwindef::WORD,
}

pub fn midi_out_get_caps(id: basetsd::UINT_PTR) -> MidiResult<MidiOutCaps> {
    let mut caps = mem::MaybeUninit::<mmsystem::MIDIOUTCAPSW>::zeroed();

    map_mmresult(
        unsafe {
            mmeapi::midiOutGetDevCapsW(
                id,
                caps.as_mut_ptr(),
                mem::size_of::<mmsystem::MIDIOUTCAPSW>() as _,
            )
        },
        || {
            let caps = unsafe { caps.assume_init() };
            MidiOutCaps {
                chan_mask: caps.wChannelMask,
                driver_ver: caps.vDriverVersion,
                id,
                mid: caps.wMid,
                name: wchar_to_string(unsafe { &caps.szPname }),
                notes: caps.wNotes,
                pid: caps.wPid,
                support: caps.dwSupport,
                tech: caps.wTechnology,
                voices: caps.wVoices,
            }
        },
    )
}

pub type MidiOutHandle = AtomicPtr<mmsystem::HMIDIOUT__>;

pub fn midi_out_open(id: minwindef::UINT) -> MidiResult<MidiOutHandle> {
    let mut dev = mem::MaybeUninit::<mmsystem::HMIDIOUT>::zeroed();

    map_mmresult(
        unsafe { mmeapi::midiOutOpen(dev.as_mut_ptr() as _, id, 0, 0, mmsystem::CALLBACK_NULL) },
        || MidiOutHandle::new(unsafe { dev.assume_init() }),
    )
}

pub fn midi_out_close(handle: &mut MidiOutHandle) -> MidiResult<()> {
    mmresult(unsafe { mmeapi::midiOutClose(*(handle.get_mut())) })
}

pub fn midi_out_reset(handle: &mut MidiOutHandle) -> MidiResult<()> {
    mmresult(unsafe { mmeapi::midiOutReset(*(handle.get_mut())) })
}

pub fn midi_out_msg(handle: &mut MidiOutHandle, msg: minwindef::DWORD) -> MidiResult<()> {
    mmresult(unsafe { mmeapi::midiOutShortMsg(*(handle.get_mut()), msg) })
}

fn mmresult(mmresult: mmsystem::MMRESULT) -> MidiResult<()> {
    match mmresult {
        mmsystem::MMSYSERR_NOERROR => Ok(()),
        err => Err(MidiError(err)),
    }
}

fn map_mmresult<F, T>(mmresult: mmsystem::MMRESULT, succ: F) -> MidiResult<T>
where
    F: Fn() -> T,
{
    match mmresult {
        mmsystem::MMSYSERR_NOERROR => Ok(succ()),
        err => Err(MidiError(err)),
    }
}

fn wchar_to_string(wchar: &[ntdef::WCHAR]) -> String {
    wchar
        .to_vec()
        .split(|&wchar| wchar == 0)
        .next()
        .map(|wchars| String::from(ffi::OsString::from_wide(wchars).to_string_lossy()))
        .unwrap()
}
