//! Read-only Darwin process and boot information (no Accessibility permission).
use std::ffi::CStr;
use std::collections::BTreeMap;

pub fn boot_id() -> String {
    let mut value: libc::timeval = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of_val(&value);
    let result = unsafe { libc::sysctlbyname(c"kern.boottime".as_ptr(), (&mut value as *mut libc::timeval).cast(), &mut size, std::ptr::null_mut(), 0) };
    if result == 0 { format!("{}:{}", value.tv_sec, value.tv_usec) } else { String::new() }
}
#[link(name = "proc")]
unsafe extern "C" {
    fn proc_listallpids(buffer: *mut libc::c_void, buffersize: libc::c_int) -> libc::c_int;
    fn proc_pidpath(pid: libc::c_int, buffer: *mut libc::c_void, buffersize: u32) -> libc::c_int;
}
pub fn processes() -> Vec<(String, Vec<u32>)> {
    let count = unsafe { proc_listallpids(std::ptr::null_mut(), 0) }.max(0) as usize;
    let mut pids = vec![0i32; count + 128];
    let count = unsafe { proc_listallpids(pids.as_mut_ptr().cast(), (pids.len() * 4) as i32) }.max(0) as usize;
    let mut grouped: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for pid in pids.into_iter().take(count).filter(|p| *p > 0) {
        let mut path = [0i8; 4096];
        if unsafe { proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) } > 0 {
            let path = unsafe { CStr::from_ptr(path.as_ptr()) }.to_string_lossy().into_owned();
            grouped.entry(path).or_default().push(pid as u32);
        }
    }
    grouped.into_iter().collect()
}
