use std::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU8, Ordering},
};
use windows_sys::Win32::{
    System::{Power::*, RemoteDesktop::*, SystemServices::GUID_CONSOLE_DISPLAY_STATE},
    UI::WindowsAndMessaging::*,
};

pub(crate) fn locked() -> Option<bool> {
    let mut buffer = ptr::null_mut();
    let mut len = 0;
    // SAFETY: the successful query allocates the buffer. Check size and
    // discriminator before reading its union; always free it afterwards.
    unsafe {
        if WTSQuerySessionInformationW(
            ptr::null_mut(),
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &mut buffer,
            &mut len,
        ) == 0
            || buffer.is_null()
        {
            return None;
        }
        let result = if len as usize >= size_of::<WTSINFOEXW>() {
            let info = &*buffer.cast::<WTSINFOEXW>();
            if info.Level == 1 {
                match info.Data.WTSInfoExLevel1.SessionFlags as u32 {
                    WTS_SESSIONSTATE_LOCK => Some(true),
                    WTS_SESSIONSTATE_UNLOCK => Some(false),
                    _ => None,
                }
            } else {
                None
            }
        } else {
            None
        };
        WTSFreeMemory(buffer.cast());
        result
    }
}

static CONSOLE: AtomicU8 = AtomicU8::new(0);
pub(crate) fn console_off() -> Option<bool> {
    match CONSOLE.load(Ordering::Acquire) {
        1 => Some(false),
        2 => Some(true),
        _ => None,
    }
}
unsafe extern "system" fn notify(_: *const c_void, event: u32, setting: *const c_void) -> u32 {
    match event {
        PBT_APMSUSPEND => super::suspend::transition(true),
        PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND | PBT_APMRESUMECRITICAL => {
            super::suspend::transition(false)
        }
        PBT_POWERSETTINGCHANGE if !setting.is_null() => {
            // SAFETY: registered for just GUID_CONSOLE_DISPLAY_STATE. Windows
            // supplies a variable-length record with one DWORD for that GUID.
            let s = unsafe { &*setting.cast::<POWERBROADCAST_SETTING>() };
            if s.DataLength == 4 {
                let state = unsafe {
                    ptr::read_unaligned(
                        setting
                            .byte_add(std::mem::offset_of!(POWERBROADCAST_SETTING, Data))
                            .cast::<u32>(),
                    )
                };
                CONSOLE.store(
                    match state {
                        0 => 2,
                        1 | 2 => 1,
                        _ => 0,
                    },
                    Ordering::Release,
                );
            }
        }
        _ => {}
    }
    0
}

struct Registrations {
    sleep: isize,
    display: isize,
}
impl Registrations {
    fn open() -> Self {
        let mut callbacks = DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
            Callback: Some(notify),
            Context: ptr::null_mut(),
        };
        let recipient = (&mut callbacks as *mut DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS).cast();
        let (mut sleep, mut display) = (ptr::null_mut(), ptr::null_mut());
        // SAFETY: registration copies the callback and context; the callback
        // uses only process-lifetime atomics. Handles belong to this guard.
        unsafe {
            if PowerRegisterSuspendResumeNotification(DEVICE_NOTIFY_CALLBACK, recipient, &mut sleep)
                != 0
            {
                sleep = ptr::null_mut();
            }
            if PowerSettingRegisterNotification(
                &GUID_CONSOLE_DISPLAY_STATE,
                DEVICE_NOTIFY_CALLBACK,
                recipient,
                &mut display,
            ) != 0
            {
                display = ptr::null_mut();
            }
        }
        Self {
            sleep: sleep as isize,
            display: display as isize,
        }
    }
}
impl Drop for Registrations {
    fn drop(&mut self) {
        unsafe {
            if self.sleep != 0 {
                PowerUnregisterSuspendResumeNotification(self.sleep);
            }
            if self.display != 0 {
                PowerSettingUnregisterNotification(self.display);
            }
        }
        CONSOLE.store(0, Ordering::Release);
    }
}
pub(crate) async fn notifications() {
    loop {
        let regs = Registrations::open();
        if regs.sleep != 0 && regs.display != 0 {
            std::future::pending::<()>().await;
        }
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        drop(regs);
    }
}
