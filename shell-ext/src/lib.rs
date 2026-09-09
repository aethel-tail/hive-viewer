//! Windows 11 context menu handler for Hive Viewer.
//!
//! Loaded into a COM surrogate (dllhost.exe) by the sparse MSIX package
//! declared in `src-tauri/windows/AppxManifest.xml`. Registers a cascading
//! "Hive Viewer" verb with two sub-commands:
//!   打开       -> hive-viewer.exe <path>
//!   格式转换   -> hive-viewer.exe --convert <path>
//!                 多选时改为 --convert-list <临时列表文件>（每行一个路径），
//!                 避免命令行过长；写临时文件失败则退回多个 --convert 参数。
//! The exe lives next to this DLL; the app itself does the rest.

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// 与 src-tauri 的 IMAGE_EXTENSIONS 保持一致（ASCII 大小写不敏感）。
const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp", "avif"];

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::Shell::*;

// {564DF141-C207-46B0-9E42-638B78B4C1CA} — must match AppxManifest.xml.
pub const CLSID_OPEN_COMMAND: GUID = GUID::from_u128(0x564df141_c207_46b0_9e42_638b78b4c1ca);
// Distinct canonical names for the sub-commands.
const NAME_OPEN: GUID = GUID::from_u128(0x564df141_c207_46b0_9e42_638b78b4c1cb);
const NAME_CONVERT: GUID = GUID::from_u128(0x564df141_c207_46b0_9e42_638b78b4c1cc);

#[implement(IExplorerCommand)]
struct HiveViewerCommand;

#[implement(IExplorerCommand)]
struct OpenCommand;

#[implement(IExplorerCommand)]
struct ConvertCommand;

#[implement(IEnumExplorerCommand)]
struct CommandEnumerator {
    items: Vec<IExplorerCommand>,
    pos: AtomicU32,
}

#[implement(IClassFactory)]
struct HiveViewerCommandFactory;

fn sub_commands() -> Vec<IExplorerCommand> {
    vec![OpenCommand.into(), ConvertCommand.into()]
}

impl IClassFactory_Impl for HiveViewerCommandFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Ref<'_, IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> Result<()> {
        if ppvobject.is_null() {
            return Err(E_POINTER.into());
        }
        if !punkouter.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let command: IExplorerCommand = HiveViewerCommand.into();
        unsafe { command.query(riid, ppvobject).ok() }
    }

    fn LockServer(&self, _flock: BOOL) -> Result<()> {
        Ok(())
    }
}

type MenuFlags = (bool, bool, bool);

/// 前端写入的右键菜单开关（generalSettings.shellContextMenu / ...Open / ...Convert）。
/// 读 %APPDATA%\dev.hive.viewer\settings.json —— tauri-plugin-store 写成带缩进的 JSON，
/// 所以定位到键后跳过冒号后的空白再比 `false`（compact JSON 也能吃），
/// 不为此把 JSON 解析器拖进 Explorer 的代理进程。
/// 用 (mtime, len) 做缓存键：右键菜单会反复查询，避免每次都读盘。
static FLAGS_CACHE: Mutex<Option<(Option<SystemTime>, u64, MenuFlags)>> = Mutex::new(None);

fn menu_flags() -> MenuFlags {
    let path = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|p| p.join("dev.hive.viewer").join("settings.json"));
    let meta = path.as_ref().and_then(|p| std::fs::metadata(p).ok());
    let stamp = meta.as_ref().and_then(|m| m.modified().ok());
    let len = meta.as_ref().map(|m| m.len()).unwrap_or(0);

    if let Ok(cache) = FLAGS_CACHE.lock() {
        if let Some((cached_stamp, cached_len, flags)) = *cache {
            if cached_stamp == stamp && cached_len == len {
                return flags;
            }
        }
    }

    let text = path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let flags = (
        flag(&text, "shellContextMenu"),
        flag(&text, "shellContextMenuOpen"),
        flag(&text, "shellContextMenuConvert"),
    );
    if let Ok(mut cache) = FLAGS_CACHE.lock() {
        *cache = Some((stamp, len, flags));
    }
    flags
}

/// 键不存在（旧版设置文件 / 应用从未运行过）按「开」处理，与安装时自动注册的默认一致
fn flag(text: &str, key: &str) -> bool {
    let Some(i) = text.find(&format!("\"{}\"", key)) else {
        return true;
    };
    let rest = &text[i..];
    let Some(j) = rest.find(':') else {
        return true;
    };
    !rest[j + 1..].trim_start().starts_with("false")
}

fn state(enabled: bool) -> u32 {
    if enabled {
        ECS_ENABLED.0 as u32
    } else {
        ECS_HIDDEN.0 as u32
    }
}

impl IExplorerCommand_Impl for HiveViewerCommand_Impl {
    fn GetTitle(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        to_pwstr("Hive Viewer")
    }

    fn GetIcon(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        to_pwstr(&format!("{},0", exe_path()?.display()))
    }

    fn GetToolTip(&self, items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        self.GetTitle(items)
    }

    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(CLSID_OPEN_COMMAND)
    }

    fn GetState(&self, _items: Ref<'_, IShellItemArray>, _ok_to_be_slow: BOOL) -> Result<u32> {
        let (master, open, convert) = menu_flags();
        // 总开关关掉、或两个子项都关掉时，整个级联菜单一起隐藏（避免空 flyout）
        Ok(state(master && (open || convert)))
    }

    fn Invoke(&self, _items: Ref<'_, IShellItemArray>, _ctx: Ref<'_, IBindCtx>) -> Result<()> {
        Err(E_NOTIMPL.into()) // never called: we advertise sub-commands instead
    }

    fn GetFlags(&self) -> Result<u32> {
        Ok(ECF_HASSUBCOMMANDS.0 as u32)
    }

    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        Ok(CommandEnumerator { items: sub_commands(), pos: AtomicU32::new(0) }.into())
    }
}

impl IExplorerCommand_Impl for OpenCommand_Impl {
    fn GetTitle(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        to_pwstr("打开")
    }

    fn GetIcon(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        to_pwstr(&format!("{},0", exe_path()?.display()))
    }

    fn GetToolTip(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        to_pwstr("使用 Hive Viewer 查看图片")
    }

    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(NAME_OPEN)
    }

    fn GetState(&self, _items: Ref<'_, IShellItemArray>, _ok_to_be_slow: BOOL) -> Result<u32> {
        let (master, open, _) = menu_flags();
        Ok(state(master && open))
    }

    fn Invoke(&self, items: Ref<'_, IShellItemArray>, _ctx: Ref<'_, IBindCtx>) -> Result<()> {
        if let Some(path) = first_item_path(items)? {
            launch(&[path])?;
        }
        Ok(())
    }

    fn GetFlags(&self) -> Result<u32> {
        Ok(ECF_DEFAULT.0 as u32)
    }

    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        Err(E_NOTIMPL.into())
    }
}

impl IExplorerCommand_Impl for ConvertCommand_Impl {
    fn GetTitle(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        to_pwstr("格式转换")
    }

    fn GetIcon(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        to_pwstr(&format!("{},0", exe_path()?.display()))
    }

    fn GetToolTip(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        to_pwstr("使用 Hive Viewer 转换图片格式")
    }

    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(NAME_CONVERT)
    }

    fn GetState(&self, _items: Ref<'_, IShellItemArray>, _ok_to_be_slow: BOOL) -> Result<u32> {
        let (master, _, convert) = menu_flags();
        Ok(state(master && convert))
    }

    fn Invoke(&self, items: Ref<'_, IShellItemArray>, _ctx: Ref<'_, IBindCtx>) -> Result<()> {
        let paths = image_paths(items)?;
        match paths.as_slice() {
            [] => {}
            [path] => launch(&["--convert".into(), path.clone()])?,
            _ => launch_convert_batch(&paths)?,
        }
        Ok(())
    }

    fn GetFlags(&self) -> Result<u32> {
        Ok(ECF_DEFAULT.0 as u32)
    }

    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        Err(E_NOTIMPL.into())
    }
}

impl IEnumExplorerCommand_Impl for CommandEnumerator_Impl {
    fn Next(&self, celt: u32, out: *mut Option<IExplorerCommand>, fetched: *mut u32) -> HRESULT {
        if out.is_null() {
            return E_POINTER;
        }
        let mut n = 0u32;
        while n < celt {
            let pos = self.pos.load(Ordering::Relaxed) as usize;
            let Some(item) = self.items.get(pos) else { break };
            unsafe { *out.add(n as usize) = Some(item.clone()) };
            self.pos.store(pos as u32 + 1, Ordering::Relaxed);
            n += 1;
        }
        if !fetched.is_null() {
            unsafe { *fetched = n };
        }
        if n == celt { S_OK } else { S_FALSE }
    }

    fn Skip(&self, celt: u32) -> Result<()> {
        let pos = self.pos.fetch_add(celt, Ordering::Relaxed) + celt;
        if pos >= self.items.len() as u32 {
            self.pos.store(self.items.len() as u32, Ordering::Relaxed);
            return Err(S_FALSE.into());
        }
        Ok(())
    }

    fn Reset(&self) -> Result<()> {
        self.pos.store(0, Ordering::Relaxed);
        Ok(())
    }

    fn Clone(&self) -> Result<IEnumExplorerCommand> {
        Ok(CommandEnumerator {
            items: sub_commands(),
            pos: AtomicU32::new(self.pos.load(Ordering::Relaxed)),
        }
        .into())
    }
}

/// Filesystem path of the first selected item, if any.
fn first_item_path(items: Ref<'_, IShellItemArray>) -> Result<Option<String>> {
    let items = items.ok()?;
    unsafe {
        if items.GetCount()? == 0 {
            return Ok(None);
        }
        // The app scans the whole directory itself, so the first item is enough.
        let item = items.GetItemAt(0)?;
        let path = item.GetDisplayName(SIGDN_FILESYSPATH)?;
        Ok(Some(pwstr_to_string(path)))
    }
}

/// Filesystem path 是否属于支持的图片扩展名。
fn is_image_path(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| IMAGE_EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known)))
        .unwrap_or(false)
}

/// 选中项里所有支持图片的文件系统路径（顺序与资源管理器一致）。
/// 单个取不到路径的项直接跳过，不因此放弃整批。
fn image_paths(items: Ref<'_, IShellItemArray>) -> Result<Vec<String>> {
    let items = items.ok()?;
    let mut paths = Vec::new();
    unsafe {
        for i in 0..items.GetCount()? {
            let Ok(item) = items.GetItemAt(i) else { continue };
            let Ok(raw) = item.GetDisplayName(SIGDN_FILESYSPATH) else { continue };
            let path = pwstr_to_string(raw);
            if is_image_path(&path) {
                paths.push(path);
            }
        }
    }
    Ok(paths)
}

/// 多选批量转换：路径写入临时列表文件后以 --convert-list 启动。
/// Windows 文件名不含换行，因此每行一个路径是安全的；写盘失败时退回逐路径参数
/// （命令行长度上限内仍可用，后端会自行过滤非图片项）。
fn launch_convert_batch(paths: &[String]) -> Result<()> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let list = std::env::temp_dir().join(format!("hive-convert-{}-{}.txt", std::process::id(), nanos));
    if std::fs::write(&list, paths.join("\n")).is_ok() {
        let result = launch(&["--convert-list".into(), list.to_string_lossy().to_string()]);
        if result.is_err() {
            let _ = std::fs::remove_file(&list); // 启动失败：别把清单留在 temp
        }
        return result;
    }
    let mut args = vec!["--convert".to_string()];
    args.extend(paths.iter().cloned());
    launch(&args)
}

/// Launch `hive-viewer.exe` (next to this DLL) with the given arguments.
fn launch(args: &[String]) -> Result<()> {
    let mut cmd = std::process::Command::new(exe_path()?);
    cmd.args(args);
    cmd.spawn()?;
    Ok(())
}

/// Path to `hive-viewer.exe`, sitting next to this DLL in the install dir.
fn exe_path() -> Result<PathBuf> {
    unsafe {
        let mut hmodule = HMODULE::default();
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(exe_path as *const () as *const u16),
            &mut hmodule,
        )?;
        let mut buf = vec![0u16; 260];
        let len = GetModuleFileNameW(Some(hmodule), &mut buf);
        if len == 0 {
            return Err(Error::from_win32());
        }
        let dll_path = PathBuf::from(String::from_utf16_lossy(&buf[..len as usize]));
        Ok(dll_path.with_file_name("hive-viewer.exe"))
    }
}

/// Allocate a null-terminated wide string on the COM task heap (caller frees).
fn to_pwstr(s: &str) -> Result<PWSTR> {
    let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let p = CoTaskMemAlloc(wide.len() * 2) as *mut u16;
        if p.is_null() {
            return Err(E_OUTOFMEMORY.into());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
        Ok(PWSTR(p))
    }
}

/// Copy a shell-owned PWSTR into a String and free the original.
unsafe fn pwstr_to_string(p: PWSTR) -> String {
    unsafe {
        let mut len = 0;
        while *p.0.add(len) != 0 {
            len += 1;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p.0, len));
        CoTaskMemFree(Some(p.0 as *const c_void));
        s
    }
}

#[no_mangle]
extern "system" fn DllGetClassObject(rclsid: *const GUID, riid: *const GUID, ppv: *mut *mut c_void) -> HRESULT {
    if ppv.is_null() {
        return E_POINTER;
    }
    unsafe { *ppv = std::ptr::null_mut() };
    if unsafe { *rclsid } != CLSID_OPEN_COMMAND {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = HiveViewerCommandFactory.into();
    unsafe { factory.query(riid, ppv) }
}

#[no_mangle]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    // ponytail: never unload — dllhost recycles the process instead.
    // Add a real refcount only if unload latency ever matters.
    S_FALSE
}
