use std::path::Path;
use std::time::Duration;

pub const AFTER_AN_UPDATE: Duration = Duration::from_secs(15);
pub const AT_A_START: Duration = Duration::from_secs(2);

pub enum Taken {
    Alone(Held),
    Other,
}

pub struct Held {
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
}

fn name(data_dir: &Path) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data_dir.to_string_lossy().to_lowercase().bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("Local\\Dota2ModEnjoyer-{hash:016x}")
}

#[cfg(windows)]
pub fn take(data_dir: &Path, wait: Duration) -> Taken {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};

    let name: Vec<u16> = name(data_dir).encode_utf16().chain(Some(0)).collect();
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return Taken::Alone(Held { handle });
    }
    let millis = u32::try_from(wait.as_millis()).unwrap_or(u32::MAX);
    match unsafe { WaitForSingleObject(handle, millis) } {
        WAIT_OBJECT_0 | WAIT_ABANDONED => Taken::Alone(Held { handle }),
        _ => {
            unsafe { CloseHandle(handle) };
            Taken::Other
        }
    }
}

#[cfg(not(windows))]
pub fn take(_data_dir: &Path, _wait: Duration) -> Taken {
    Taken::Alone(Held {})
}

#[cfg(windows)]
impl Drop for Held {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::ReleaseMutex;
        if !self.handle.is_null() {
            unsafe {
                ReleaseMutex(self.handle);
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(windows)]
pub fn bring_forward() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, IsIconic, SW_RESTORE, SetForegroundWindow, ShowWindow};
    let title: Vec<u16> = crate::app::TITLE.encode_utf16().chain(Some(0)).collect();
    unsafe {
        let window = FindWindowW(std::ptr::null(), title.as_ptr());
        if window.is_null() {
            return;
        }
        if IsIconic(window) != 0 {
            ShowWindow(window, SW_RESTORE);
        }
        SetForegroundWindow(window);
    }
}

#[cfg(not(windows))]
pub fn bring_forward() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_data_folder_has_a_name_of_its_own_whatever_its_case() {
        let real = name(Path::new(r"D:\People\someone\AppData\Local\Dota2ModEnjoyer"));
        assert_eq!(real, name(Path::new(r"d:\people\SOMEONE\appdata\local\dota2modenjoyer")));
        assert_ne!(real, name(Path::new(r"C:\scratch\data")));
        assert!(real.starts_with("Local\\Dota2ModEnjoyer-") && real.len() == "Local\\Dota2ModEnjoyer-".len() + 16);
        assert_eq!(name(Path::new("Data")), "Local\\Dota2ModEnjoyer-855b556730a34a05");
    }

    #[cfg(windows)]
    #[test]
    fn a_second_copy_finds_the_first_and_takes_over_once_it_has_closed() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().to_path_buf();
        let Taken::Alone(first) = take(&data, AT_A_START) else { panic!("nobody had it") };
        let second = {
            let data = data.clone();
            std::thread::spawn(move || matches!(take(&data, Duration::from_millis(100)), Taken::Other))
        };
        assert!(second.join().unwrap(), "the first copy has it");
        let elsewhere = tempfile::tempdir().unwrap();
        let path = elsewhere.path().to_path_buf();
        assert!(std::thread::spawn(move || matches!(take(&path, Duration::ZERO), Taken::Alone(_))).join().unwrap());
        let waiting = {
            let data = data.clone();
            std::thread::spawn(move || matches!(take(&data, Duration::from_secs(5)), Taken::Alone(_)))
        };
        std::thread::sleep(Duration::from_millis(200));
        drop(first);
        assert!(waiting.join().unwrap(), "it took over");
    }
}
