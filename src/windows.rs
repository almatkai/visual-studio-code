//! Closing editor windows is a named user action and the editor owns unsaved-file prompts.
#[cfg(windows)]
pub fn close_other(editor: &crate::editor::Editor) -> Result<(), String> {
    use windows_sys::Win32::{Foundation::*, System::Threading::*, UI::WindowsAndMessaging::*};
    unsafe extern "system" fn collect(window: HWND, data: LPARAM) -> BOOL {
        let out = &mut *(data as *mut Vec<HWND>);
        if IsWindowVisible(window) != 0 && GetWindow(window, GW_OWNER).is_null() {
            out.push(window);
        }
        1
    }
    let expected =
        std::fs::canonicalize(&editor.executable).map_err(|_| "Editor executable unavailable")?;
    unsafe {
        let mut windows = Vec::new();
        EnumWindows(Some(collect), (&mut windows as *mut Vec<HWND>) as isize);
        for window in windows {
            let mut pid = 0;
            GetWindowThreadProcessId(window, &mut pid);
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                continue;
            }
            let mut name = vec![0u16; 32768];
            let mut size = name.len() as u32;
            let ok = QueryFullProcessImageNameW(process, 0, name.as_mut_ptr(), &mut size) != 0;
            CloseHandle(process);
            if ok {
                let path =
                    std::path::PathBuf::from(String::from_utf16_lossy(&name[..size as usize]));
                if std::fs::canonicalize(path).is_ok_and(|p| {
                    p.to_string_lossy()
                        .eq_ignore_ascii_case(&expected.to_string_lossy())
                }) {
                    PostMessageW(window, WM_CLOSE, 0, 0);
                }
            }
        }
    }
    Ok(())
}
