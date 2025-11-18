// https://learn.microsoft.com/zh-cn/windows-hardware/manufacture/desktop/wim/dd834950(v=msdn.10)?view=windows-11

use libloading::Library;
use serde::Serialize;
use std::ffi::{c_void, OsStr};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::{mem, ptr};
use windows::core::GUID;
use windows::Win32::Foundation::{GetLastError, GENERIC_EXECUTE};

/// WIMGAPI错误类型枚举
#[derive(Debug)]
pub enum WimApiError {
    /// Win32 API错误
    Win32Error(u32),
    /// 库加载错误
    LibraryError(libloading::Error),
    /// 通用错误信息
    Message(String),
}

impl std::fmt::Display for WimApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WimApiError::Win32Error(code) => write!(f, "Win32 Error: {}", code),
            WimApiError::LibraryError(err) => write!(f, "Library Error: {}", err),
            WimApiError::Message(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for WimApiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WimApiError::LibraryError(err) => Some(err),
            _ => None,
        }
    }
}

impl From<libloading::Error> for WimApiError {
    fn from(err: libloading::Error) -> Self {
        WimApiError::LibraryError(err)
    }
}

pub const WIM_GENERIC_READ: u32 = 0x8000_0000; // GENERIC_READ
pub const WIM_GENERIC_WRITE: u32 = 0x4000_0000; // GENERIC_WRITE

pub const WIM_CREATE_NEW: u32 = 1; // CREATE_NEW
pub const WIM_CREATE_ALWAYS: u32 = 2; // CREATE_ALWAYS
pub const WIM_OPEN_EXISTING: u32 = 3; // OPEN_EXISTING
pub const WIM_OPEN_ALWAYS: u32 = 4; // OPEN_ALWAYS

pub const WIM_COMPRESS_NONE: u32 = 0;
pub const WIM_COMPRESS_XPRESS: u32 = 1;
pub const WIM_COMPRESS_LZX: u32 = 2;
pub const WIM_COMPRESS_LZMS: u32 = 3;

pub const WIM_FLAG_RESERVED: u32 = 1;
pub const WIM_FLAG_VERIFY: u32 = 2;
pub const WIM_FLAG_INDEX: u32 = 4;
pub const WIM_FLAG_NO_APPLY: u32 = 8;
pub const WIM_FLAG_NO_DIRACL: u32 = 16;
pub const WIM_FLAG_NO_FILEACL: u32 = 32;
pub const WIM_FLAG_SHARE_WRITE: u32 = 64;
pub const WIM_FLAG_FILEINFO: u32 = 128;
pub const WIM_FLAG_MOUNT_READONLY: u32 = 0x0000_0200;

pub const WIM_MOUNT_FLAG_MOUNTED: u32 = 0x00000001;
pub const WIM_MOUNT_FLAG_MOUNTING: u32 = 0x00000002;
pub const WIM_MOUNT_FLAG_REMOUNTABLE: u32 = 0x00000004;
pub const WIM_MOUNT_FLAG_INVALID: u32 = 0x00000008;
pub const WIM_MOUNT_FLAG_NO_WIM: u32 = 0x00000010;
pub const WIM_MOUNT_FLAG_NO_MOUNTDIR: u32 = 0x00000020;
pub const WIM_MOUNT_FLAG_MOUNTDIR_REPLACED: u32 = 0x00000040;
pub const WIM_MOUNT_FLAG_READWRITE: u32 = 0x00000100;

pub const WIM_MSG_PROGRESS: u32 = 38008;
pub const WIM_MSG_PROCESS: u32 = 38009;
pub const WIM_MSG_SCANNING: u32 = 38010;
pub const WIM_MSG_SETRANGE: u32 = 38011;
pub const WIM_MSG_SETPOS: u32 = 38012;
pub const WIM_MSG_STEPIT: u32 = 38013;
pub const WIM_MSG_COMPRESS: u32 = 38014;
pub const WIM_MSG_ERROR: u32 = 38015;
pub const WIM_MSG_ALIGNMENT: u32 = 38016;
pub const WIM_MSG_RETRY: u32 = 38017;
pub const WIM_MSG_SPLIT: u32 = 38018;
pub const WIM_MSG_FILEINFO: u32 = 38019;
pub const WIM_MSG_INFO: u32 = 38020;
pub const WIM_MSG_WARNING: u32 = 38021;
pub const WIM_MSG_CHK_PROCESS: u32 = 38022;
pub const WIM_MSG_WARNING_OBJECTID: u32 = 38023;
pub const WIM_MSG_STALE_MOUNT_DIR: u32 = 38024;
pub const WIM_MSG_STALE_MOUNT_FILE: u32 = 38025;
pub const WIM_MSG_MOUNT_CLEANUP_PROGRESS: u32 = 38026;
pub const WIM_MSG_CLEANUP_SCANNING_DRIVE: u32 = 38027;
pub const WIM_MSG_IMAGE_ALREADY_MOUNTED: u32 = 38028;
pub const WIM_MSG_CLEANUP_UNMOUNTING_IMAGE: u32 = 38029;
pub const WIM_MSG_QUERY_ABORT: u32 = 38030;
pub const WIM_MSG_IO_RANGE_START_REQUEST_LOOP: u32 = 38031;
pub const WIM_MSG_IO_RANGE_END_REQUEST_LOOP: u32 = 38032;
pub const WIM_MSG_IO_RANGE_REQUEST: u32 = 38033;
pub const WIM_MSG_IO_RANGE_RELEASE: u32 = 38034;
pub const WIM_MSG_VERIFY_PROGRESS: u32 = 38035;
pub const WIM_MSG_COPY_BUFFER: u32 = 38036;
pub const WIM_MSG_METADATA_EXCLUDE: u32 = 38037;
pub const WIM_MSG_GET_APPLY_ROOT: u32 = 38038;
pub const WIM_MSG_MDPAD: u32 = 38039;
pub const WIM_MSG_STEPNAME: u32 = 38040;
pub const WIM_MSG_PERFILE_COMPRESS: u32 = 38041;
pub const WIM_MSG_CHECK_CI_EA_PREREQUISITE_NOT_MET: u32 = 38042;
pub const WIM_MSG_JOURNALING_ENABLED: u32 = 38043;
pub const WIM_MSG_ABORT_IMAGE: u32 = 4294967295;
pub const WIM_GENERIC_MOUNT: u32 = GENERIC_EXECUTE.0;

pub const WIM_REFERENCE_APPEND: u32 = 0x0001_0000; // WIMSetReferenceFile flags
pub const WIM_REFERENCE_REPLACE: u32 = 0x0002_0000;

pub const WIM_COMMIT_FLAG_APPEND: u32 = 0x0000_0001; // WIMCommitImageHandle

// Windows API 定义的路径最大长度
pub const MAX_PATH: usize = 260;

// 内部使用的原始结构体，用于与 Windows API 交互
#[repr(C)]
struct WIM_MOUNT_INFO_LEVEL0_RAW {
    wim_path: [u16; MAX_PATH],
    mount_path: [u16; MAX_PATH],
    image_index: u32,
    mounted_for_rw: bool,
}
#[repr(C)]
struct WIM_MOUNT_INFO_LEVEL1_RAW {
    wim_path: [u16; MAX_PATH],
    mount_path: [u16; MAX_PATH],
    image_index: u32,
    mount_flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct WIM_INFO_RAW {
    wim_path: [u16; MAX_PATH],
    guid: GUID,
    image_count: u32,
    compression_type: u32,
    part_number: u16,
    total_parts: u16,
    boot_index: u32,
    wim_attributes: u32,
    wim_flags_and_attr: u32,
}

/// MOUNTED_IMAGE_INFO_LEVELS 枚举
pub enum MountedImageInfoLevels {
    /// 使用 WIM_MOUNT_INFO_LEVEL0 结构
    MountedImageInfoLevel0 = 0,
    /// 使用 WIM_MOUNT_INFO_LEVEL1 结构
    MountedImageInfoLevel1 = 1,
}

/// WIM_MOUNT_INFO_LEVEL0 结构体
/// 包含通过 WIMGetMountedImageInfo 函数检索的信息
#[derive(Debug, Clone)]
pub struct WimMountInfoLevel0 {
    /// 指定 .wim 文件的完整路径
    pub wim_path: String,
    /// 指定装载映像的目录的完整路径
    pub mount_path: String,
    /// 指定 WimPath 中指定的 .wim 文件中的映像索引
    pub image_index: u32,
    /// 指定装载的映像是否支持保存更改
    pub mounted_for_rw: bool,
}

/// WIM_MOUNT_INFO_LEVEL1 结构体
/// 包含通过 WIMGetMountedImageInfo 函数检索的信息
#[derive(Debug, Clone)]
pub struct WimMountInfoLevel1 {
    /// 指定 .wim 文件的完整路径
    pub wim_path: String,
    /// 指定装载映像的目录的完整路径
    pub mount_path: String,
    /// 指定 WimPath 中指定的 .wim 文件中的映像索引
    pub image_index: u32,
    /// 指定装载点的当前状态。
    /// - `WIM_MOUNT_FLAG_MOUNTED`: 映像已主动装载。
    /// - `WIM_MOUNT_FLAG_MOUNTING`: 映像正在装载过程中。
    /// - `WIM_MOUNT_FLAG_REMOUNTABLE`: 映像未装载，但可以重新装载。
    /// - `WIM_MOUNT_FLAG_INVALID`: 映像装载点不再有效。
    /// - `WIM_MOUNT_FLAG_NO_WIM`: 支持装载点的 WIM 文件丢失或无法访问。
    /// - `WIM_MOUNT_FLAG_NO_MOUNTDIR`: 映像装载点已被删除或替换。
    /// - `WIM_MOUNT_FLAG_MOUNTDIR_REPLACED`: 装载点已被其他装载的映像替换。
    /// - `WIM_MOUNT_FLAG_READWRITE`: 该映像已以读写访问方式装载。
    pub mount_flags: u32,
}

#[derive(Debug, Clone)]
pub struct WimInfo {
    /// 指定 .wim 文件的完整路径
    pub wim_path: String,
    /// 指定包含 Windows 映像 (.wim) 文件唯一标识符的 GUID 结构。
    pub guid: GUID,
    /// 指定 .wim 文件中包含的映像数量。
    pub image_count: u32,
    /// 指定用于压缩 .wim 文件中资源的压缩方法。 有关初始压缩类型，请参阅 WIMCreateFile 函数。
    pub compression_type: u32,
    /// 指定跨文件集中当前 .wim 文件的部件号。 除非 .wim 文件的数据最初是由 WIMSplitFile 函数分割的，否则此值应为 1。
    pub part_number: u16,
    /// 指定跨文件集中 .wim 文件部件的总数。 除非 .wim 文件的数据最初是通过 WIMSplitFile 函数分割的，否则此值必须为 1。
    pub total_parts: u16,
    /// 指定 .wim 文件中可启动映像的索引。 如果此值为 0，则没有可用的可启动映像。 要设置可启动映像，请调用 WIMSetBootImage 函数。
    pub boot_index: u32,
    /// 指定如何处理文件以及使用哪些功能。
    /// - `WIM_ATTRIBUTE_NORMAL`: .wim 文件没有设置任何其他属性。
    /// - `WIM_ATTRIBUTE_RESOURCE_ONLY`: .wim 文件只包含文件资源，而不包含映像或元数据。
    /// - `WIM_ATTRIBUTE_METADATA_ONLY`: .wim 文件只包含映像资源和 XML 信息。
    /// - `WIM_ATTRIBUTE_VERIFY_DATA`: .wim 文件包含可被 WIMCopyFile 或 WIMCreateFile 函数使用的完整性数据。
    /// - `WIM_ATTRIBUTE_RP_FIX`: .wim 文件包含一个或多个已启用符号链接或交叉点路径修复的映像。
    /// - `WIM_ATTRIBUTE_SPANNED`: 通过 WIMSplitFile，.wim 文件已被分割成多个部分。
    /// - `WIM_ATTRIBUTE_READONLY`: .wim 文件已被锁定，无法进行修改。
    pub wim_attributes: u32,
    /// 指定在 WIMCreateFile 函数期间使用的标志。
    pub wim_flags_and_attr: u32,
}

type Pcwstr = *const u16;
type Pwstr = *mut u16;
type Pdword = *mut u32;
type Handle = usize;

type DsofWimcreateFile = unsafe extern "system" fn(
    pszWimPath: Pcwstr,
    dwDesiredAccess: u32,
    dwCreationDisposition: u32,
    dwFlagsAndAttributes: u32,
    dwCompressionType: u32,
    pdwCreationResult: *mut u32,
) -> Handle;

type DosfWimcloseHandle = unsafe extern "system" fn(hObject: Handle) -> bool;

type DosfWimsetReferenceFile = unsafe extern "system" fn(hWim: Handle, pszPath: Pcwstr, dwFlags: u32) -> bool;

type DosfWimcaptureImage = unsafe extern "system" fn(hWim: Handle, pszPath: Pcwstr, dwCaptureFlags: u32) -> Handle;

type DosfWimcommitImageHandle =
    unsafe extern "system" fn(hImage: Handle, dwCommitFlags: u32, phNewImageHandle: *mut Handle) -> bool;

type DosfWimsetTemporaryPath = unsafe extern "system" fn(hImage: Handle, pszPath: Pcwstr) -> bool;

type DosfWimloadImage = unsafe extern "system" fn(hWim: Handle, dwImageIndex: u32) -> Handle;

type DosfWimgetImageCount = unsafe extern "system" fn(hWim: Handle) -> u32;

type DosfWIMGetAttributes =
    unsafe extern "system" fn(hWim: Handle, pWimInfo: *mut WIM_INFO_RAW, cbWimInfo: u32) -> bool;

type DosfWimgetImageInformation =
    unsafe extern "system" fn(hImage: Handle, ppvImageInfo: *mut *mut std::ffi::c_void, pcbImageInfo: *mut u32) -> bool;

type DosfWimsetImageInformation = unsafe extern "system" fn(
    hWim: Handle,
    pbImageInfo: *const u8, // 指向 UTF-16 编码 XML 缓冲
    dwImageInfoSize: u32,
) -> bool;

type DosfWimapplyImage = unsafe extern "system" fn(hWim: Handle, pszPath: Pcwstr, dwApplyFlags: u32) -> bool;

type DosfWimexportImage = unsafe extern "system" fn(hImage: Handle, pszWimFileName: Handle, dwFlags: u32) -> bool;

type DosfWimdeleteImage = unsafe extern "system" fn(hWim: Handle, dwImageIndex: u32) -> bool;

type DosfWimsetBootImage = unsafe extern "system" fn(hWim: Handle, dwImageIndex: u32) -> bool;

type DosfWimmountImage = unsafe extern "system" fn(
    pszMountPath: Pwstr,
    pszWimFileName: Pwstr,
    dwImageIndex: u32,
    pszTempPath: Pwstr,
) -> bool;

type DosfWimmountImageHandle =
    unsafe extern "system" fn(hImage: Handle, pszMountPath: Pwstr, dwMountFlags: u32) -> bool;

type DosfWimunmountImage = unsafe extern "system" fn(
    pszMountPath: Pwstr,
    pszWimFileName: Pwstr,
    dwImageIndex: u32,
    bCommitChanges: bool,
) -> bool;

type DsofWIMUnmountImageHandle = unsafe extern "system" fn(hImage: Handle, dwUnmountFlags: u32) -> bool;

type DsofWIMRemountImage = unsafe extern "system" fn(pszMountPath: Pwstr, dwFlags: u32) -> bool;

type DosfWimgetMountedImageInfo = unsafe extern "system" fn(
    fInfoLevelId: u32,
    pdwImageCount: *mut u32,
    pMountInfo: *mut c_void,
    cbMountInfoLength: u32,
    pcbReturnLength: *mut u32,
) -> bool;

type DosfWimregisterMessageCallback = unsafe extern "system" fn(
    hWim: Handle,
    fpMessageProc: extern "system" fn(u32, usize, isize, *mut c_void) -> u32,
    pvUserData: *mut c_void,
) -> u32;

type DosfWimunregisterMessageCallback = unsafe extern "system" fn(
    hWim: Handle,
    fpMessageProc: extern "system" fn(u32, usize, isize, *mut c_void) -> u32,
) -> bool;

pub struct Wimgapi {
    _lib: Library,
    WIMCreateFile: DsofWimcreateFile,
    WIMCloseHandle: DosfWimcloseHandle,
    WIMSetReferenceFile: DosfWimsetReferenceFile,
    WIMCaptureImage: DosfWimcaptureImage,
    WIMCommitImageHandle: DosfWimcommitImageHandle,
    WIMSetTemporaryPath: DosfWimsetTemporaryPath,
    WIMLoadImage: DosfWimloadImage,
    WIMGetImageCount: DosfWimgetImageCount,
    WIMGetAttributes: DosfWIMGetAttributes,
    WIMGetImageInformation: DosfWimgetImageInformation,
    WIMApplyImage: DosfWimapplyImage,
    WIMExportImage: DosfWimexportImage,
    WIMDeleteImage: DosfWimdeleteImage,
    WIMSetBootImage: DosfWimsetBootImage,
    WIMMountImage: DosfWimmountImage,
    WIMMountImageHandle: DosfWimmountImageHandle,
    WIMUnmountImage: DosfWimunmountImage,
    WIMUnmountImageHandle: DsofWIMUnmountImageHandle,
    WIMRemountImage: DsofWIMRemountImage,
    WIMGetMountedImageInfo: DosfWimgetMountedImageInfo,
    WIMSetImageInformation: DosfWimsetImageInformation,
    WIMRegisterMessageCallback: DosfWimregisterMessageCallback,
    WIMUnregisterMessageCallback: DosfWimunregisterMessageCallback,
}

/// 将 &OsStr 转成以 NUL 结尾的 UTF-16 Vec<u16>
fn to_wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

#[derive(Serialize, Debug)]
struct FileMeta {
    path: String,
    size: Option<u64>,
    mtime: Option<String>,
    attributes: Option<u32>,
    sddl: Option<String>,
}

impl Wimgapi {
    /// 加载 wimgapi.dll 并解析所需函数
    ///
    /// # 参数
    ///  - `path`: 可选的 wimgapi.dll 路径，默认值为 "wimgapi.dll"
    ///
    /// # 示例
    /// ```
    /// let wimgapi = Wimgapi::new(None).unwrap();
    /// ```
    ///
    /// # 返回值
    ///  - `Ok(Self)`: 成功加载 wimgapi.dll 并解析函数
    ///  - `Err(WimApiError)`: 加载失败或解析函数失败
    pub fn new(path: Option<PathBuf>) -> Result<Self, WimApiError> {
        let lib = { unsafe { Library::new(path.unwrap_or(PathBuf::from("wimgapi.dll"))) } }?;
        unsafe {
            Ok(Self {
                WIMCreateFile: *lib.get(b"WIMCreateFile")?,
                WIMCloseHandle: *lib.get(b"WIMCloseHandle")?,
                WIMSetReferenceFile: *lib.get(b"WIMSetReferenceFile")?,
                WIMCaptureImage: *lib.get(b"WIMCaptureImage")?,
                WIMCommitImageHandle: *lib.get(b"WIMCommitImageHandle")?,
                WIMSetTemporaryPath: *lib.get(b"WIMSetTemporaryPath")?,
                WIMLoadImage: *lib.get(b"WIMLoadImage")?,
                WIMGetImageCount: *lib.get(b"WIMGetImageCount")?,
                WIMGetAttributes: *lib.get(b"WIMGetAttributes")?,
                WIMGetImageInformation: *lib.get(b"WIMGetImageInformation")?,
                WIMSetImageInformation: *lib.get(b"WIMSetImageInformation")?,
                WIMRegisterMessageCallback: *lib.get(b"WIMRegisterMessageCallback")?,
                WIMUnregisterMessageCallback: *lib.get(b"WIMUnregisterMessageCallback")?,
                WIMApplyImage: *lib.get(b"WIMApplyImage")?,
                WIMExportImage: *lib.get(b"WIMExportImage")?,
                WIMDeleteImage: *lib.get(b"WIMDeleteImage")?,
                WIMSetBootImage: *lib.get(b"WIMSetBootImage")?,
                WIMMountImage: *lib.get(b"WIMMountImage")?,
                WIMMountImageHandle: *lib.get(b"WIMMountImageHandle")?,
                WIMUnmountImage: *lib.get(b"WIMUnmountImage")?,
                WIMUnmountImageHandle: *lib.get(b"WIMUnmountImageHandle")?,
                WIMRemountImage: *lib.get(b"WIMRemountImage")?,
                WIMGetMountedImageInfo: *lib.get(b"WIMGetMountedImageInfo")?,
                _lib: lib,
            })
        }
    }

    /// 创建新映像文件或打开现有映像文件
    ///
    /// # 参数
    ///  - `path`: 指定要创建或打开的文件名
    ///  - `access`: 指定对对象的访问类型。 应用程序可以获取读取访问权限、写入访问权限、读/写访问权限或设备查询访问权限。 对于此参数，可以使用以下数值的任意组合：
    ///     - `0`: 指定对文件的查询访问权限。 应用程序可以在不访问映像的情况下查询映像信息。
    ///     - `WIM_GENERIC_READ`: 指定对映像文件的只读访问权限。 允许从文件中应用映像。 与 WIM_GENERIC_WRITE 结合以实现读/写（追加）访问权限。
    ///     - `WIM_GENERIC_WRITE`: 指定对映像文件的写入访问权限。 允许将映像捕获到文件中。 包括 WIM_GENERIC_READ 访问权限，以便对现有映像进行应用和追加操作。
    ///     - `WIM_GENERIC_MOUNT`: 指定对映像文件的装载访问权限。 允许通过 WIMMountImageHandle 来装载映像。
    ///  - `operate`: 指定对存在的文件采取的操作，以及当文件不存在时采取的操作。 此参数必须使用下列值之一：
    ///      - `WIM_CREATE_NEW`: 创建一个新的映像文件。 如果指定的文件已经存在，则函数执行失败。
    ///      - `WIM_CREATE_ALWAYS`: 创建一个新的映像文件。 如果文件存在，则函数将覆盖该文件。
    ///      - `WIM_OPEN_EXISTING`: 打开映像文件。 如果文件不存在，则函数执行失败。
    ///      - `WIM_OPEN_ALWAYS`: 如果存在映像文件，则打开该文件。 如果文件不存在，且调用方请求 WIM_GENERIC_WRITE 访问权限，则函数会创建该文件。
    ///  - `compression_type`: 指定新创建的映像文件要使用的压缩模式。 如果文件已经存在，则此值将被忽略。 此参数必须使用下列值之一：
    ///      - `WIM_COMPRESS_NONE`: 捕获不会使用文件压缩。
    ///      - `WIM_COMPRESS_XPRESS`: 捕获会使用 XPRESS 文件压缩。
    ///      - `WIM_COMPRESS_LZX`: 捕获会使用 Lzx 文件压缩。
    ///
    /// # 示例
    /// ```
    /// let wimgapi = Wimgapi::new(None).unwrap();
    /// let handle = wimgapi.open(r"D:\base.wim", WIM_GENERIC_READ, WIM_OPEN_EXISTING, WIM_COMPRESS_NONE).unwrap();
    /// ```
    ///
    /// # 返回值
    /// - `Ok(Handle)`: 句柄
    /// - `Err(...)`：失败则返回 FALSE，则包含 Win32 错误码或说明
    pub fn open(&self, path: &Path, access: u32, operate: u32, compression_type: u32) -> Result<Handle, WimApiError> {
        let mut _creation: u32 = 0;

        let handle = unsafe {
            (self.WIMCreateFile)(
                to_wide(path.as_os_str()).as_ptr(),
                access,
                operate,
                0,
                compression_type, // 打开已存在文件时此处通常无效
                &mut _creation as *mut u32,
            )
        };

        if handle != 0 {
            Ok(handle)
        } else {
            unsafe { Err(WimApiError::Win32Error(GetLastError().0)) }
        }
    }

    /// 关闭打开的 Windows 映像 (.wim) 文件或映像句柄
    ///
    /// # 参数
    ///  - `handle`: 通过调用 `open` 返回的句柄
    ///
    /// # 示例
    /// ```
    /// let wimgapi = Wimgapi::new(None).unwrap();
    /// let handle = wimgapi.open(r"D:\base.wim", WIM_GENERIC_READ, WIM_OPEN_EXISTING, WIM_COMPRESS_NONE).unwrap();
    /// wimgapi.close(handle).unwrap();
    /// ```
    ///
    /// # 返回值
    /// - `Ok(())`: 返回成功
    /// - `Err(...)`：失败则返回包含 Win32 错误码的说明
    pub fn close(&self, handle: Handle) -> Result<(), WimApiError> {
        if !unsafe { (self.WIMCloseHandle)(handle) } {
            return Err(unsafe { WimApiError::Win32Error(GetLastError().0) });
        }

        Ok(())
    }

    /// 设置临时映像文件的存储位置。
    ///
    /// # 参数
    ///  - `handle`: 通过调用 `open` 返回的句柄
    ///  - `path`: 指明在捕获或应用过程中存储临时映像 (.wim) 文件的路径。 这是捕获或应用映像的目录。
    ///
    /// # 示例
    /// ```
    /// let wimgapi = Wimgapi::new(None).unwrap();
    /// let handle = wimgapi.open(r"D:\base.wim", WIM_GENERIC_READ, WIM_OPEN_EXISTING, WIM_COMPRESS_NONE).unwrap();
    /// wimgapi.set_temp_path(handle, r"D:\UserData\Desktop\test\WimPatch\Patch").unwrap();
    /// ```
    ///
    /// # 返回值
    /// - `Ok(())`: 返回成功
    /// - `Err(...)`：失败则返回包含 Win32 错误码的说明
    pub fn set_temp_path(&self, handle: Handle, path: &Path) -> Result<(), WimApiError> {
        let result = unsafe { (self.WIMSetTemporaryPath)(handle, to_wide(path.as_os_str()).as_ptr()) };
        if result {
            Ok(())
        } else {
            unsafe { Err(WimApiError::Win32Error(GetLastError().0)) }
        }
    }

    /// 从 Windows 映像 (.wim) 文件加载卷映像。
    ///
    /// # 参数
    ///  - `handle `: WIMCreateFile 函数返回的 .wim 文件句柄。
    ///  - `index`: 指定要加载的映像从 1 开始的索引。 一个映像文件可存储多个映像。
    ///
    /// # 注意
    /// - 在调用 WIMLoadImage 函数之前，必须首先调用 `set_temp_path` 函数，以便从临时位置提取和处理元数据。
    ///
    /// # 示例
    /// ```
    /// let wimgapi = Wimgapi::new(None).unwrap();
    /// let handle = wimgapi.open(r"D:\base.wim", WIM_GENERIC_READ, WIM_OPEN_EXISTING, WIM_COMPRESS_NONE).unwrap();
    /// wimgapi.set_temp_path(handle, r"D:\Temp").unwrap();
    /// let image_handle = wimgapi.load_image(handle, 1).unwrap();
    /// ```
    ///
    /// # 返回值
    /// - `Ok(Handle)`: 返回成功，包含卷映像的对象的句柄
    /// - `Err(...)`：失败则返回包含 Win32 错误码的说明
    pub fn load_image(&self, handle: Handle, index: u32) -> Result<Handle, WimApiError> {
        let result = unsafe { (self.WIMLoadImage)(handle, index) };

        if result != 0 {
            Ok(result)
        } else {
            Err(unsafe { WimApiError::Win32Error(GetLastError().0) })
        }
    }

    /// 返回 Windows 映像 (.wim) 文件中存储的卷映像数量。
    ///
    /// # 参数
    ///  - `handle `: WIMCreateFile 函数返回的 .wim 文件句柄。
    ///
    /// # 返回值
    /// - 映像文件中的映像数量。 如果此值为 0，则表示映像文件无效或不包含任何可应用的映像。
    ///
    /// # 示例
    /// ```
    /// let wimgapi = Wimgapi::new(None).unwrap();
    /// let handle = wimgapi.open(r"D:\base.wim", WIM_GENERIC_READ, WIM_OPEN_EXISTING, WIM_COMPRESS_NONE).unwrap();
    /// let image_count = wimgapi.get_image_count(handle).unwrap();
    /// ```
    pub fn get_image_count(&self, handle: Handle) -> u32 {
        unsafe { (self.WIMGetImageCount)(handle) }
    }

    /// 从目录路径捕获映像并将其存储到映像文件中
    ///
    /// # 参数
    ///  - `handle`: 通过调用 `open` 返回的句柄
    ///  - `src_path`: 捕获映像数据的根驱动器或目录路径。
    ///  - `flag`: 指定要在捕获过程中使用的功能。
    ///     - `0`
    ///     - `WIM_FLAG_VERIFY`: 捕获功能可逐个字节验证单实例文件。
    ///     - `WIM_FLAG_NO_RP_FIX`: 禁用交叉点和符号链接的自动路径修复。
    ///     - `WIM_FLAG_NO_DIRACL`: 禁用捕获目录的安全信息。
    ///     - `WIM_FLAG_NO_FILEACL`: 禁用捕获文件的安全信息。
    ///
    /// # 注意
    /// - 结束后需要需要调用`close`关闭卷映像的对象的句柄
    /// - 如需捕获系统请排除以下目录：
    ///   - `C:\$ntfs.log`
    ///   - `C:\hiberfil.sys`
    ///   - `C:\pagefile.sys`
    ///   - `C:\swapfile.sys`
    ///   - `C:\System Volume Information`
    ///   - `C:\RECYCLER`
    ///   - `C:\Windows\CSC`
    ///
    /// # 返回值
    /// - `Ok(Handle)`: 返回成功，包含卷映像的对象的句柄
    /// - `Err(...)`：失败则返回包含 Win32 错误码的说明
    pub fn capture(&self, handle: Handle, src_path: &Path, flags: u32) -> Result<Handle, WimApiError> {
        let h_image = unsafe { (self.WIMCaptureImage)(handle, to_wide(src_path.as_os_str()).as_ptr(), flags) };
        if h_image != 0 {
            Ok(h_image)
        } else {
            Err(unsafe { WimApiError::Win32Error(GetLastError().0) })
        }
    }
    /// 将已加载映像中的更改保存到 .wim 文件中
    ///
    /// # 参数
    ///  - `handle`: 通过 WIMLoadImage 函数打开的映像的句柄。
    ///  - `flags`: 指定要在提交过程中使用的功能。
    ///     - `0`
    ///     - `WIM_COMMIT_FLAG_APPEND`: 在 .wim 文件中添加新的映像条目。 默认值是更新装载期间指定的映像。
    ///     - `WIM_FLAG_VERIFY`: 捕获功能可逐个字节验证单实例文件。
    ///     - `WIM_FLAG_NO_RP_FIX`: 禁用交叉点和符号链接的自动路径修复。
    ///     - `WIM_FLAG_NO_DIRACL`: 禁用捕获目录的安全信息。
    ///     - `WIM_FLAG_NO_FILEACL`: 禁用捕获文件的安全信息。
    ///
    /// # 注意
    /// - 在调用 WIMCreateFile 时，必须使用 WIM_GENERIC_MOUNT 标志来打开 .wim 文件。
    /// - WIMCommitImageHandle 函数更新 .wim 文件中给定映像的内容，以反映指定挂载目录的内容。 成功完成此操作后，用户或应用程序仍可访问映射到挂载目录下的映像内容。
    /// - 使用 WIMUnmountImageHandle 函数，以使用映像句柄从挂载目录中卸载映像。
    ///
    /// # 返回值
    /// - `Ok(())`: 返回成功
    /// - `Err(...)`：失败则返回包含 Win32 错误码的说明
    pub fn commit(&self, handle: Handle, flags: u32) -> Result<(), WimApiError> {
        let mut _new_img = std::ptr::null_mut();
        let result = unsafe { (self.WIMCommitImageHandle)(handle, flags, _new_img) };
        if result {
            Ok(())
        } else {
            Err(unsafe { WimApiError::Win32Error(GetLastError().0) })
        }
    }

    /// 将映像从 Windows 映像 (.wim) 文件应用到目录路径。
    ///
    /// # 参数
    ///  - `handle`: WIMLoadImage 或 WIMCaptureImage 函数返回的卷映像的句柄。
    ///  - `path`: 应用映像数据的根驱动器或目录路径
    ///  - `dwApplyFlags`: 指定如何处理文件以及使用哪些功能。
    ///     - `0`: 默认，无处理
    ///     - `WIM_FLAG_VERIFY`: 验证文件是否与原始数据匹配。
    ///     - `WIM_FLAG_INDEX`: 指定为缓存或性能目的而按顺序读取映像。
    ///     - `WIM_FLAG_NO_APPLY`: 应用映像而不实际创建目录或文件。 可用于获取映像中的文件和目录列表。
    ///     - `WIM_FLAG_FILEINFO`: 在应用操作期间发送 WIM_MSG_FILEINFO 消息。
    ///     - `WIM_FLAG_NO_RP_FIX`: 禁用交叉点和符号链接的自动路径修复。
    ///     - `WIM_FLAG_NO_DIRACL`: 禁用还原目录的安全信息。
    ///     - `WIM_FLAG_NO_FILEACL`: 禁用还原文件的安全信息。
    ///
    /// # 注意
    /// - 要在应用映像时获取详细信息，请参阅 `WIMRegisterMessageCallback` 函数。
    /// - 要在不实际应用映像的情况下获取映像中的文件列表，请指定 `WIM_FLAG_NO_APPLY` 标志，并注册一个用于处理 `WIM_MSG_PROCESS` 消息的回调。 要从 `WIM_MSG_FILEINFO` 消息中获取其他文件信息，请指定 `WIM_FLAG_FILEINFO`。
    ///
    /// # 返回值
    /// - `Ok(())`: 返回成功
    /// - `Err(...)`：失败则返回包含 Win32 错误码的说明
    pub fn apply_image(&self, handle: Handle, path: &Path, flag: u32) -> Result<(), WimApiError> {
        let result = unsafe { (self.WIMApplyImage)(handle, to_wide(path.as_os_str()).as_ptr(), flag) };

        if result {
            Ok(())
        } else {
            unsafe { Err(WimApiError::Win32Error(GetLastError().0)) }
        }
    }

    /// 从 .wim（Windows 映像）文件中删除映像，使其无法访问。 但是，文件资源仍可供 WIMSetReferenceFile 函数使用。
    ///
    /// # 参数
    /// - `handle`: 由 WIMCreateFile 函数返回的 .wim 文件句柄。 此句柄必须具有 WIM_GENERIC_WRITE 访问权限才能删除映像。 不支持拆分的 .wim 文件，并且 .wim 文件中不能有任何打开的映像。
    /// - `index`: 要删除的映像从 1 开始的索引。 一个映像文件可以存储多个映像。
    ///
    /// # 注意
    /// - 在调用 WIMDeleteImage 函数之前，必须首先调用 WIMSetTemporaryPath 函数，以便从临时位置提取和处理映像的元数据。
    ///
    /// # 返回值
    /// - `Ok(())`: 返回成功
    /// - `Err(...)`：失败则返回包含 Win32 错误码的说明
    pub fn delete_image(&self, handle: Handle, index: u32) -> Result<(), WimApiError> {
        let result = unsafe { (self.WIMDeleteImage)(handle, index) };
        if result {
            Ok(())
        } else {
            unsafe { Err(WimApiError::Win32Error(GetLastError().0)) }
        }
    }

    /// 将带有给定映像索引的映像标记为可启动映像。
    ///
    /// # 参数
    /// - `handle`: 由 WIMCreateFile 函数返回的 Windows 映像 (.wim) 文件的句柄。
    /// - `index`: 要加载的映像从 1 开始的索引。 一个映像文件可以存储多个映像。
    ///
    /// # 注意
    /// - 如果 `index` 的输入值为 0，则 `.wim` 文件中的所有映像都不会被标记为启动。 在任何时候，一个 `.wim` 文件中只能有一个映像被设置为可启动。
    ///
    /// # 示例
    /// ```
    /// let wimgapi = Wimgapi::new(None).unwrap();
    /// let handle = wimgapi.open(r"D:\base.wim", WIM_GENERIC_READ, WIM_OPEN_EXISTING, WIM_COMPRESS_NONE).unwrap();
    /// wimgapi.set_boot_image(handle, 1).unwrap();
    /// ```
    ///
    /// # 返回值
    /// - `true`: 函数成功执行
    /// - `false`: 函数执行失败
    pub fn set_boot_image(&self, handle: Handle, index: u32) -> bool {
        unsafe { (self.WIMSetBootImage)(handle, index) }
    }

    /// 将 Windows 映像 (.wim) 文件中的映像装载到指定的目录。
    ///
    /// # 参数
    /// - `mount_path`: 映像文件被装载到的目录完整文件路径。 指定路径的长度不得超过 MAX_PATH 字符数。
    /// - `image_path`: 装载的映像文件完整文件名。
    /// - `index`: 装载的映像文件中映像的索引。
    /// - `temp_path`: 临时目录完整文件路径。在该目录中可以跟踪 .wim 文件的更改。 如果此参数为 `None`，则不会装载映像以供编辑。
    ///
    /// # 注意
    /// - `WIMMountImage` 函数会将 .wim 文件中给定映像的内容映射到指定的装载目录。 成功完成此操作后，用户或应用程序就可访问映射到装载目录下的映像内容。
    /// - 使用 `WIMUnmountImage` 函数从装载目录中卸载映像。
    ///
    /// # 返回值
    /// - `Ok(())`: 返回成功
    /// - `Err(...)`：失败则返回包含 Win32 错误码的说明
    pub fn mount_image(
        &self,
        mount_path: &Path,
        image_path: &Path,
        index: u32,
        temp_path: Option<&Path>,
    ) -> Result<(), WimApiError> {
        let result = unsafe {
            (self.WIMMountImage)(
                to_wide(mount_path.as_os_str()).as_mut_ptr(),
                to_wide(image_path.as_os_str()).as_mut_ptr(),
                index,
                match temp_path {
                    Some(path) => to_wide(path.as_os_str()).as_mut_ptr(),
                    None => null_mut(),
                },
            )
        };

        if result {
            Ok(())
        } else {
            unsafe { Err(WimApiError::Win32Error(GetLastError().0)) }
        }
    }

    /// 将 Windows® 映像 (.wim) 文件中的映像装载到指定的目录。
    ///
    /// # 参数
    /// - `handle`: WIMLoadImage 或 WIMCaptureImage 函数返回的卷映像的句柄。 在调用 WIMCreateFile 时，必须使用 WIM_GENERIC_MOUNT 标志来打开 WIM 文件。
    /// - `mount_path`: 映像文件被装载到的目录完整文件路径。 指定路径的长度不得超过 MAX_PATH 字符数。
    /// - `flags`: 指定如何处理文件以及使用哪些功能。
    ///     - `WIM_FLAG_MOUNT_READONLY`: 无论 WIM 访问级别如何，装载映像时都无法保存更改。
    ///     - `WIM_FLAG_VERIFY`: 验证文件是否与原始数据匹配。
    ///     - `WIM_FLAG_NO_RP_FIX`: 禁用交叉点和符号链接的自动路径修复。
    ///     - `WIM_FLAG_NO_DIRACL`: 禁用还原目录的安全信息。
    ///     - `WIM_FLAG_NO_FILEACL`: 禁用还原文件的安全信息。
    ///
    /// # 注意
    /// - `WIMMountImageHandle` 函数会将 .wim 文件中给定映像的内容映射到指定的装载目录。 成功完成此操作后，用户或应用程序就可访问映射到装载目录下的映像内容。
    /// - 必须使用 WIM_GENERIC_MOUNT 访问权限来打开包含映像的 WIM 文件。
    /// - 使用 WIMUnmountImageHandle 函数从装载目录中卸载映像。
    ///
    /// # 返回值
    /// - `Ok(())`: 返回成功
    /// - `Err(...)`：失败则返回包含 Win32 错误码的说明
    pub fn mount_image_handle(&self, handle: Handle, mount_path: &Path, flags: u32) -> Result<(), WimApiError> {
        let result = unsafe { (self.WIMMountImageHandle)(handle, to_wide(mount_path.as_os_str()).as_mut_ptr(), flags) };

        if result {
            Ok(())
        } else {
            unsafe { Err(WimApiError::Win32Error(GetLastError().0)) }
        }
    }

    /// 从之前使用 WIMMountImageHandle 函数装载的 Windows® 映像 (.wim) 中卸载映像。
    ///
    /// # 参数
    /// - `handle`: WIMMountImageHandle 函数返回的卷映像的句柄。
    ///
    /// # 注意
    /// - `WIMUnmountImageHandle` 函数会从指定的装载目录中删除 .wim 文件中给定映像的内容。 成功完成此操作后，用户或应用程序将无法访问之前映射到装载目录下的映像内容。
    /// - 必须使用 WIM_GENERIC_MOUNT 访问权限来打开包含映像的 WIM 文件。
    ///
    /// # 返回值
    /// - `Ok(())`: 返回成功
    pub fn unmount_image_handle(&self, handle: Handle) -> Result<(), WimApiError> {
        let result = unsafe { (self.WIMUnmountImageHandle)(handle, 0) };

        if result {
            Ok(())
        } else {
            unsafe { Err(WimApiError::Win32Error(GetLastError().0)) }
        }
    }

    /// 从指定目录下的 Windows 映像 (.wim) 文件中卸载已装载的映像。
    ///
    /// # 参数
    /// - `mount_path`: 映像文件被装载到的目录完整文件路径。 指定路径的长度不得超过 MAX_PATH 字符数。
    /// - `image_path`: 卸载的映像文件完整文件名。
    /// - `index`: 卸载的映像文件中映像的索引。
    /// - `commit`: 指明是否必须在卸载 .wim 文件前提交对 .wim 文件的更改（如有）的标志。 如果装载 .wim 文件时未启用编辑，则此标记无效。
    ///
    /// # 注意
    /// - `WIMUnmountImage` 函数会从指定的装载目录中删除 .wim 文件中给定映像的内容。 成功完成此操作后，用户或应用程序将无法访问之前映射到装载目录下的映像内容。
    ///
    /// # 返回值
    /// - `Ok(())`: 返回成功
    /// - `Err(...)`：失败则返回包含 Win32 错误码的说明
    pub fn unmount_image(
        &self,
        mount_path: &Path,
        image_path: &Path,
        index: u32,
        commit: bool,
    ) -> Result<(), WimApiError> {
        let result = unsafe {
            (self.WIMUnmountImage)(
                to_wide(mount_path.as_os_str()).as_mut_ptr(),
                to_wide(image_path.as_os_str()).as_mut_ptr(),
                index,
                commit,
            )
        };

        if result {
            Ok(())
        } else {
            unsafe { Err(WimApiError::Win32Error(GetLastError().0)) }
        }
    }

    /// 获取当前已装载的映像列表。
    ///
    /// # 返回值
    /// - `Ok(Vec<WIM_MOUNT_INFO_LEVEL0>)`: 返回成功，包含已挂载的镜像列表
    /// - `Err(...)`：失败则返回包含 Win32 错误码的说明
    ///
    /// # 示例
    /// ```
    /// let wimgapi = Wimgapi::new(None).unwrap();
    /// let mounted_images = wimgapi.get_mounted_image().unwrap();
    /// ```
    pub fn get_mounted_image(&self) -> Result<Vec<WimMountInfoLevel1>, WimApiError> {
        unsafe {
            // 首先获取已挂载的镜像数量
            let mut image_count: u32 = 0;
            let mut return_length: u32 = 0;

            let result = (self.WIMGetMountedImageInfo)(
                MountedImageInfoLevels::MountedImageInfoLevel1 as u32,
                &mut image_count,
                null_mut(),
                0,
                &mut return_length,
            );

            // 如果函数返回失败，并且不是因为缓冲区不足，则返回错误
            if !result {
                let error = GetLastError().0;
                if error != 122 {
                    // ERROR_INSUFFICIENT_BUFFER
                    return Err(WimApiError::Win32Error(error));
                }
            }

            // 根据返回的长度分配缓冲区
            let buffer_size = return_length as usize;
            let mut buffer: Vec<u8> = vec![0; buffer_size];

            // 再次调用函数获取详细信息
            let result = (self.WIMGetMountedImageInfo)(
                MountedImageInfoLevels::MountedImageInfoLevel1 as u32,
                &mut image_count,
                buffer.as_mut_ptr() as *mut c_void,
                buffer_size as u32,
                &mut return_length,
            );

            if !result {
                return Err(WimApiError::Win32Error(GetLastError().0));
            }

            // 将原始数据转换为WIM_MOUNT_INFO_LEVEL1结构体列表
            let item_size = std::mem::size_of::<WIM_MOUNT_INFO_LEVEL1_RAW>();
            let mut result_list: Vec<WimMountInfoLevel1> = Vec::new();

            for i in 0..image_count {
                let offset = i as usize * item_size;
                if offset + item_size <= buffer_size {
                    let raw_ptr = &buffer[offset] as *const u8 as *const WIM_MOUNT_INFO_LEVEL1_RAW;
                    let raw_info = &*raw_ptr;

                    // 将UTF-16字符串转换为Rust字符串
                    let wim_path = Wimgapi::utf16_ptr_to_string(&raw_info.wim_path as *const u16, MAX_PATH);
                    let mount_path = Wimgapi::utf16_ptr_to_string(&raw_info.mount_path as *const u16, MAX_PATH);

                    result_list.push(WimMountInfoLevel1 {
                        wim_path,
                        mount_path,
                        image_index: raw_info.image_index,
                        mount_flags: raw_info.mount_flags,
                    });
                }
            }

            Ok(result_list)
        }
    }

    /// 重新激活之前装载到指定目录的已装载映像。
    ///
    /// # 参数
    /// - `mount_path`: 映像文件必须被重新装载到的目录完整文件路径。指定路径的长度不得超过 MAX_PATH 字符数。
    ///
    /// # 注意
    /// - `WIMRemountImage` 函数会将 .wim 文件中给定映像的内容映射到指定的装载目录。 成功完成此操作后，用户或应用程序就可访问映射到装载目录下的映像内容。
    /// - 使用 WIMUnmountImage 函数从装载目录中卸载映像。
    ///
    /// # 返回值
    /// - `Ok(())`: 返回成功
    /// - `Err(...)`：失败则返回包含 Win32 错误码的说明
    pub fn remount_image(&self, mount_path: &Path) -> Result<(), WimApiError> {
        let result = unsafe { (self.WIMRemountImage)(to_wide(mount_path.as_os_str()).as_mut_ptr(), 0) };

        if result {
            Ok(())
        } else {
            unsafe { Err(WimApiError::Win32Error(GetLastError().0)) }
        }
    }

    /// 将映像数据从一个 Windows 映像 (.wim) 文件传输到另一个。
    ///
    /// # 参数
    /// - `hImage`: 通过 `WIMLoadImage` 函数打开的映像的句柄。
    /// - `hWim`: `WIMCreateFile` 函数返回的 .wim 文件句柄。 此句柄必须具有 `WIM_GENERIC_WRITE` 访问权限才能接受导出的映像。 不支持拆分 .wim 文件。
    /// - `flags`: 指定将映像导出到目标 .wim 文件的方式。
    ///   - `WIM_EXPORT_ALLOW_DUPLICATES`: 即使映像已存储在 .wim 文件中，它也会被导出到目标 .wim 文件中。
    ///   - `WIM_EXPORT_ONLY_RESOURCES`: 文件资源会被导出到目标 .wim 文件中，并且不包含映像资源或 XML 信息。
    ///   - `WIM_EXPORT_ONLY_METADATA`: 映像资源和 XML 信息将被导出到目标 .wim 文件中，并且不包括支持文件资源。
    ///
    /// # 注意
    /// - 在调用 `WIMExportImage` 函数之前，必须为源文件和目标 .wim 文件调用 `WIMSetTemporaryPath` 函数。
    /// - 如果 `flag` 参数传递的值为 0，且映像已存储在于目标中，则函数将返回 `FALSE`，并将 `LastError` 设置为 `ERROR_ALREADY_EXISTS`。
    ///
    /// # 返回值
    /// - `Ok(())`: 返回成功
    /// - `Err(...)`：失败则返回包含 Win32 错误码的说明
    pub fn export_image(&self, hImage: Handle, hWim: Handle, flags: u32) -> Result<(), WimApiError> {
        let result = unsafe { (self.WIMExportImage)(hImage, hWim, flags) };

        if result {
            Ok(())
        } else {
            unsafe { Err(WimApiError::Win32Error(GetLastError().0)) }
        }
    }

    /// 启用 WIMApplyImage 和 WIMCaptureImage 函数，以便将备用 .wim 文件用作文件资源。 这样可以优化在捕获到多个数据相似的映像时的存储。
    ///
    /// # 参数
    ///  - `handle`: 通过调用 `open` 返回的句柄
    ///  - `ref_path`: 要添加到引用列表中的 .wim 文件的路径
    ///  - `flag`: 指定如何将 .wim 文件添加到引用列表。 此参数必须包含以下两个值之一
    ///     - `WIM_REFERENCE_APPEND`: 将指定的 .wim 文件添加到当前列表中。
    ///     - `WIM_REFERENCE_REPLACE`: 指定的 .wim 文件将成为列表中唯一的项目。
    ///
    /// # 返回值
    /// - `Ok(())`: 返回成功
    /// - `Err(...)`：失败则返回包含 Win32 错误码的说明
    pub fn set_reference_file(&self, handle: Handle, ref_path: &Path, flag: u32) -> Result<(), WimApiError> {
        let result = unsafe { (self.WIMSetReferenceFile)(handle, to_wide(ref_path.as_os_str()).as_ptr(), flag) };

        if result {
            Ok(())
        } else {
            unsafe { Err(WimApiError::Win32Error(GetLastError().0)) }
        }
    }

    /// 将 UTF-16 编码的字符串转换为 Rust 字符串
    ///
    /// # 参数
    /// - `ptr`: 指向 UTF-16 编码字符串的指针
    /// - `units`: 字符串的长度（单位：UTF-16 代码单元）
    ///
    /// # 返回值
    /// - `String`: 转换后的 Rust 字符串
    fn utf16_ptr_to_string(ptr: *const u16, units: usize) -> String {
        if ptr.is_null() || units == 0 {
            return String::new();
        }
        // create slice (units length), trim trailing zeros
        unsafe {
            let slice = std::slice::from_raw_parts(ptr, units);
            let mut len = units;
            while len > 0 && slice[len - 1] == 0 {
                len -= 1;
            }
            String::from_utf16_lossy(&slice[..len])
        }
    }

    /// 获取卷信息
    ///
    /// # 参数
    /// - `hImage`: 由 WIMCreateFile、WIMLoadImage 或 WIMCaptureImage 函数返回的句柄
    ///
    /// # 返回值
    /// - `Ok(String)`: 包含卷映像信息的 XML 字符串
    /// - `Err(WimApiError)`: 错误信息
    pub fn get_image_info(&self, handle: Handle) -> Result<String, WimApiError> {
        let mut pv: *mut std::ffi::c_void = ptr::null_mut();
        let mut size: u32 = 0;

        let result = unsafe {
            (self.WIMGetImageInformation)(handle, &mut pv as *mut *mut std::ffi::c_void, &mut size as *mut u32)
        };
        if !result {
            return Err(WimApiError::Win32Error(unsafe { GetLastError().0 }));
        }

        // Interpret pv as UTF-16LE buffer of size bytes -> u16 units = size/2
        let xml_string = Wimgapi::utf16_ptr_to_string(pv as *const u16, (size as usize) / 2);

        Ok(xml_string)
    }

    /// 获取wim映像属性
    ///
    /// # 参数
    /// - `hWim`: 由 WIMCreateFile 函数返回的句柄
    ///
    /// # 返回值
    /// - `Ok(WIM_INFO_RAW)`: 包含卷映像属性的结构体
    /// - `Err(WimApiError)`: 错误信息
    pub fn get_attributes(&self, handle: Handle) -> Result<WimInfo, WimApiError> {
        let mut raw: WIM_INFO_RAW = unsafe { mem::zeroed() };
        let size = mem::size_of::<WIM_INFO_RAW>() as u32;

        let result = unsafe { (self.WIMGetAttributes)(handle, &mut raw as *mut _, size) };
        if !result {
            return Err(WimApiError::Win32Error(unsafe { GetLastError().0 }));
        }

        let path = Wimgapi::utf16_ptr_to_string(raw.wim_path.as_ptr(), MAX_PATH);
        Ok(WimInfo {
            wim_path: path,
            guid: raw.guid,
            image_count: raw.image_count,
            compression_type: raw.compression_type,
            part_number: raw.part_number,
            total_parts: raw.total_parts,
            boot_index: raw.boot_index,
            wim_attributes: raw.wim_attributes,
            wim_flags_and_attr: raw.wim_flags_and_attr,
        })
    }

    /// 设置卷映像信息
    ///
    /// # 参数
    /// - `hImage`: 由 WIMCreateFile、WIMLoadImage 或 WIMCaptureImage 函数返回的句柄
    /// - `xml_info`: 包含卷映像信息的 XML 字符串
    ///
    /// # 说明
    /// - 传入的 XML 数据必须是 Unicode 格式
    /// - 如果输入句柄来自 WIMCreateFile 函数，则 XML 数据必须用 `<WIM></WIM>` 标记括起来
    /// - 如果输入句柄来自 WIMLoadImage 或 WIMCaptureImage 函数，则 XML 数据必须用 `<IMAGE></IMAGE>` 标记括起来
    ///
    /// # 返回值
    /// - `Ok(())`: 设置成功
    /// - `Err(WimApiError)`: 错误信息
    pub fn set_image_info(&self, hImage: Handle, xml_info: &str) -> Result<(), WimApiError> {
        // 将 Rust 字符串转换为 UTF-16 编码的字节数组
        let utf16_chars: Vec<u16> = xml_info.encode_utf16().collect();
        let buffer_size = (utf16_chars.len() * std::mem::size_of::<u16>()) as u32;

        // 调用 WIMSetImageInformation 函数
        let result = unsafe { (self.WIMSetImageInformation)(hImage, utf16_chars.as_ptr() as *const u8, buffer_size) };

        if result {
            Ok(())
        } else {
            Err(WimApiError::Win32Error(unsafe { GetLastError().0 }))
        }
    }

    /// 注册一个要通过映像特定的数据调用的函数。
    ///
    /// # 参数
    /// - `handle`: 由 WIMCreateFile 返回的 `.wim` 文件句柄。
    /// - `callback`: 指向应用程序定义的回调函数的指针。
    ///
    /// # 返回值
    /// - 如果函数成功执行，则返回值为回调函数从 0 开始的索引。
    /// - 如果函数执行失败，则返回值为 `INVALID_CALLBACK_VALUE` (`0xFFFFFFFF`)。
    pub fn register_message_callback(
        &self,
        handle: Handle,
        callback: extern "system" fn(u32, usize, isize, *mut c_void) -> u32,
    ) -> u32 {
        unsafe { (self.WIMRegisterMessageCallback)(handle, callback, null_mut()) }
    }

    /// 取消注册使用映像特定数据调用的函数。
    ///
    /// # 参数
    /// - `handle`: 由 WIMCreateFile 返回的 `.wim` 文件句柄。
    /// - `fpMessageProc`: 一个指向要取消注册的应用程序定义的回调函数的指针。 指定 `NULL` 以取消注册所有回调函数。
    ///
    /// # 返回值
    /// - `true`: 函数成功执行
    /// - `false`: 函数执行失败
    pub fn unregister_message_callback(
        &self,
        handle: Handle,
        fpMessageProc: extern "system" fn(u32, usize, isize, *mut c_void) -> u32,
    ) -> bool {
        unsafe { (self.WIMUnregisterMessageCallback)(handle, fpMessageProc) }
    }
}