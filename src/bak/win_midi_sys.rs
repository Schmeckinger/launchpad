use std::os::windows::ffi::OsStringExt;
use std::{ffi, mem};
use thiserror::Error;
use winapi::shared::{basetsd, minwindef, ntdef};
use winapi::um::{mmeapi, mmsystem};

#[derive(Error, Debug)]
#[error("MMError({0})")]
pub struct MMError(mmsystem::MMRESULT);

pub type MMResult<T> = Result<T, MMError>;

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

pub fn midi_in_get_caps(id: basetsd::UINT_PTR) -> MMResult<MidiInCaps> {
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

pub type MidiInHandle = mmsystem::HMIDIIN;

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
) -> MMResult<MidiInHandle> {
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
        || unsafe { dev.assume_init() },
    )
}

pub fn midi_in_close(handle: MidiInHandle) -> MMResult<()> {
    mmresult(unsafe { mmeapi::midiInClose(handle) })
}

pub fn midi_in_reset(handle: MidiInHandle) -> MMResult<()> {
    mmresult(unsafe { mmeapi::midiInReset(handle) })
}

pub fn midi_in_start(handle: MidiInHandle) -> MMResult<()> {
    mmresult(unsafe { mmeapi::midiInStart(handle) })
}

pub fn midi_in_stop(handle: MidiInHandle) -> MMResult<()> {
    mmresult(unsafe { mmeapi::midiInStop(handle) })
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

pub fn midi_out_get_caps(id: basetsd::UINT_PTR) -> MMResult<MidiOutCaps> {
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

pub type MidiOutHandle = mmsystem::HMIDIOUT;

pub fn midi_out_open(id: minwindef::UINT) -> MMResult<MidiOutHandle> {
    let mut dev = mem::MaybeUninit::<mmsystem::HMIDIOUT>::zeroed();

    map_mmresult(
        unsafe { mmeapi::midiOutOpen(dev.as_mut_ptr() as _, id, 0, 0, mmsystem::CALLBACK_NULL) },
        || unsafe { dev.assume_init() },
    )
}

pub fn midi_out_close(handle: MidiOutHandle) -> MMResult<()> {
    mmresult(unsafe { mmeapi::midiOutClose(handle) })
}

pub fn midi_out_reset(handle: MidiOutHandle) -> MMResult<()> {
    mmresult(unsafe { mmeapi::midiOutReset(handle) })
}

pub fn midi_out_msg(handle: MidiOutHandle, msg: minwindef::DWORD) -> MMResult<()> {
    mmresult(unsafe { mmeapi::midiOutShortMsg(handle, msg) })
}

fn mmresult(mmresult: mmsystem::MMRESULT) -> MMResult<()> {
    match mmresult {
        mmsystem::MMSYSERR_NOERROR => Ok(()),
        err => Err(MMError(err)),
    }
}

fn map_mmresult<F, T>(mmresult: mmsystem::MMRESULT, succ: F) -> MMResult<T>
where
    F: Fn() -> T,
{
    match mmresult {
        mmsystem::MMSYSERR_NOERROR => Ok(succ()),
        err => Err(MMError(err)),
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
