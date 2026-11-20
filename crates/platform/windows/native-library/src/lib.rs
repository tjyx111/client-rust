//! 从精确绝对路径加载 Windows DLL，并独占其模块句柄。

use std::ffi::{CStr, c_void};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::NonNull;

// 只搜索 DLL 自己所在目录和 System32，不搜索当前目录或 PATH。
const LOAD_FLAGS: u32 = 0x0000_0100 | 0x0000_0800;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryExW(path: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const i8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}

// 持有一个 DLL 模块句柄；离开作用域时由 Drop 调用 FreeLibrary。
pub struct NativeLibrary {
    module: *mut c_void,
}

impl NativeLibrary {
    /// 加载指定绝对路径的 DLL。
    ///
    /// # Errors
    /// 路径不绝对、规范化失败或系统加载失败时返回错误字符串。
    pub fn load_exact(path: &Path) -> Result<Self, String> {
        if !path.is_absolute() {
            return Err(format!("DLL 路径不是绝对路径: {}", path.display()));
        }
        let canonical = match path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                return Err(format!("规范化 DLL 路径失败: {}: {error}", path.display()));
            }
        };
        let mut wide: Vec<u16> = canonical.as_os_str().encode_wide().collect();
        wide.push(0);
        let module = unsafe {
            // 安全前提：wide 以 0 结尾，调用期间一直有效。
            LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), LOAD_FLAGS)
        };
        if module.is_null() {
            let error = std::io::Error::last_os_error();
            return Err(format!("加载 DLL 失败: {}: {error}", canonical.display()));
        }
        Ok(Self { module })
    }

    /// 查找 DLL 导出地址。
    ///
    /// # Safety
    /// 返回地址只能在 `self` 存活期间使用，并且必须按导出真实 ABI 调用。
    ///
    /// # Errors
    /// 导出不存在时返回错误字符串。
    pub unsafe fn required_symbol(&self, name: &CStr) -> Result<NonNull<c_void>, String> {
        let address = unsafe {
            // 安全前提：self.module 有效，name 是以 0 结尾的 C 字符串。
            GetProcAddress(self.module, name.as_ptr())
        };
        match NonNull::new(address) {
            Some(address) => Ok(address),
            None => Err(format!("DLL 缺少导出 {}", name.to_string_lossy())),
        }
    }
}

impl Drop for NativeLibrary {
    fn drop(&mut self) {
        let released = unsafe {
            // 安全前提：self 是句柄的唯一 owner，Drop 只执行一次。
            FreeLibrary(self.module)
        };
        if released == 0 {
            eprintln!("卸载 DLL 失败: {}", std::io::Error::last_os_error());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NativeLibrary;
    use std::path::Path;

    #[test]
    fn rejects_missing_and_non_dll_files() {
        let missing = Path::new(r"C:\simple-vpn-missing-4f3c\nfapi.dll");
        assert!(NativeLibrary::load_exact(missing).is_err());
        let root = std::env::var_os("SystemRoot").expect("SystemRoot 必须存在");
        let non_dll = Path::new(&root).join("System32/drivers/etc/hosts");
        assert!(NativeLibrary::load_exact(&non_dll).is_err());
    }

    #[test]
    fn loads_winhttp_and_rejects_bad_input() {
        assert!(NativeLibrary::load_exact(Path::new("nfapi.dll")).is_err());
        let root = std::env::var_os("SystemRoot").expect("SystemRoot 必须存在");
        let path = Path::new(&root).join("System32/winhttp.dll");
        let library = NativeLibrary::load_exact(&path).expect("winhttp.dll 必须能加载");
        let found = unsafe { library.required_symbol(c"WinHttpOpen") };
        assert!(found.is_ok());
        let missing = unsafe { library.required_symbol(c"simple_vpn_missing_export") };
        assert!(missing.is_err());
    }
}
