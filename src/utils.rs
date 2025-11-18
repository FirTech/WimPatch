use crate::BUFFER_SIZE;
use anyhow::{anyhow, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::{read_dir, File};
use std::io::{BufReader, Read};
use std::iter::repeat_with;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::GetCurrentProcessId;

/// 生成临时文件名
///
/// # 参数
/// - `prefix`: 前缀
/// - `suffix`: 后缀
/// - `rand_len`: 长度
///
/// # 返回
/// - `OsString` : 临时文件名
pub fn get_tmp_name(prefix: &str, suffix: &str, rand_len: usize) -> OsString {
    let capacity = prefix.len().saturating_add(suffix.len()).saturating_add(rand_len);
    let mut buf = OsString::with_capacity(capacity);
    buf.push(prefix);
    let mut char_buf = [0u8; 4];
    for c in repeat_with(fastrand::alphanumeric).take(rand_len) {
        buf.push(c.encode_utf8(&mut char_buf));
    }
    buf.push(suffix);
    buf
}

/// 将文件大小格式化为可读字节单位（MiB/KiB）
///
/// # 参数
/// - `bytes`: 字节数
///
/// # 返回值
/// - `String` : 可读的字节单位
pub fn format_bytes(bytes: u64) -> String {
    let kb = 1024f64;
    let b = bytes as f64;
    if b >= kb.powi(3) {
        format!("{:.1} GB", b / kb.powi(3))
    } else if b >= kb.powi(2) {
        format!("{:.1} MB", b / kb.powi(2))
    } else if b >= kb {
        format!("{:.1} KB", b / kb)
    } else {
        format!("{} B", bytes)
    }
}

/// 返回当前进程的父进程 PID
fn get_parent_pid(pid: u32) -> windows::core::Result<u32> {
    unsafe {
        // 全进程快照
        let h = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?;
        if h.is_invalid() {
            return Err(windows::core::Error::from_thread());
        }
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        // 枚举第一个
        Process32FirstW(h, &mut entry)?;
        loop {
            if entry.th32ProcessID == pid {
                let _ = CloseHandle(h);
                return Ok(entry.th32ParentProcessID);
            }
            if Process32NextW(h, &mut entry).is_err() {
                break;
            }
        }
        let _ = CloseHandle(h);
        Err(windows::core::Error::from_thread())
    }
}

/// 给定 PID，返回进程名（不含路径），如 "explorer.exe"
fn get_process_name(pid: u32) -> windows::core::Result<String> {
    unsafe {
        let h = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?;
        if h.is_invalid() {
            return Err(windows::core::Error::from_thread());
        }
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        Process32FirstW(h, &mut entry)?;
        loop {
            if entry.th32ProcessID == pid {
                // 找到第一个 NUL 终止符
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(MAX_PATH as usize);
                let name = OsString::from_wide(&entry.szExeFile[..len])
                    .into_string()
                    .map_err(|_| windows::core::Error::from_thread())?;
                let _ = CloseHandle(h);
                return Ok(name);
            }
            if Process32NextW(h, &mut entry).is_err() {
                break;
            }
        }
        let _ = CloseHandle(h);
        Err(windows::core::Error::from_thread())
    }
}

/// 检查父进程名是否为 explorer.exe
pub fn launched_from_explorer() -> bool {
    let self_pid = unsafe { GetCurrentProcessId() };
    if let Ok(ppid) = get_parent_pid(self_pid)
        && let Ok(name) = get_process_name(ppid)
    {
        return name.eq_ignore_ascii_case("explorer.exe");
    }
    false
}

/// 计算文件的 SHA256 哈希值
/// # 参数
/// - `path`: 文件路径
/// # 返回值
/// - `Result<String, Box<dyn std::error::Error>>`: 文件的 SHA256 哈希值，如果计算失败则返回错误
pub fn get_file_sha256(path: impl AsRef<Path>, mut callback: Option<&mut dyn FnMut(u64, u64)>) -> Result<String> {
    // 打开文件
    let file = File::open(path)?;
    let mut reader = BufReader::new(&file);

    // 创建 SHA256 哈希器
    let mut hasher = Sha256::new();

    // 创建缓冲区
    let mut buffer = vec![0u8; BUFFER_SIZE.load(Ordering::Relaxed)];
    let mut read_total: u64 = 0;

    // 逐块读取文件并更新哈希
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        read_total += bytes_read as u64;
        // 更新哈希值
        hasher.update(&buffer[..bytes_read]);
        // 调用回调函数
        if let Some(ref mut cb) = callback {
            cb(read_total, file.metadata().unwrap().len());
        }
    }

    // 计算最终哈希值
    Ok(format!("{:x}", hasher.finalize()))
}

/// 获取文件元数据（大小、修改时间等）用于快速比较
/// # 参数
/// - `path`: 文件路径
/// # 返回值
/// - `Option<(u64, u64)>`: 包含文件大小和修改时间的元组，如果获取失败则返回None
fn get_file_metadata(path: impl AsRef<Path>) -> Option<(u64, u64)> {
    if let Ok(metadata) = std::fs::metadata(path)
        && let Ok(modified) = metadata.modified()
    {
        // 将修改时间转换为纳秒时间戳
        let modified_nanos = modified.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos() as u64;
        return Some((metadata.len(), modified_nanos));
    }
    None
}

/// 判断两个文件是否相同
/// # 参数
/// - `one`: 第一个文件路径
/// - `another`: 第二个文件路径
/// # 返回值
/// - `true`: 文件相同
/// - `false`: 文件不相同
fn is_same_file(one: impl AsRef<Path>, another: impl AsRef<Path>) -> bool {
    // 先比较文件元数据（大小和修改时间）
    if let (Some((size1, mtime1)), Some((size2, mtime2))) = (get_file_metadata(&one), get_file_metadata(&another)) {
        // 如果大小或修改时间不同，直接返回false，避免二进制对比
        if size1 != size2 || mtime1 != mtime2 {
            return false;
        }
    }

    // 只有元数据匹配时，才进行二进制对比
    if let (Ok(file0), Ok(file1)) = (File::open(one), File::open(another)) {
        let mut reader0 = BufReader::new(file0);
        let mut reader1 = BufReader::new(file1);
        let mut buf0 = vec![0u8; BUFFER_SIZE.load(Ordering::Relaxed)];
        let mut buf1 = vec![0u8; BUFFER_SIZE.load(Ordering::Relaxed)];

        while let (Ok(n0), Ok(n1)) = (reader0.read(&mut buf0), reader1.read(&mut buf1)) {
            if n0 != n1 {
                return false;
            }
            if n0 == 0 {
                return true;
            }
            if buf0[..n0] != buf1[..n1] {
                return false;
            }
        }
    }
    false
}

/// 目录修改类型枚举
#[derive(Debug)]
pub enum DiffType {
    /// 新增文件或目录
    Add,
    /// 删除文件或目录
    Delete,
    /// 修改文件
    Modify,
}

/// 目录差异回调函数类型
///
/// # 参数
/// - `diff_type`: 修改类型（新增、删除、修改）
/// - `base_path`: 基准目录中的路径（删除和修改时有效）
/// - `target_path`: 目标目录中的路径（新增和修改时有效）
/// - `rel_path`: 相对于根目录的路径
///
/// # 返回值
/// - `true`: 继续比较
/// - `false`: 中断比较
pub type DiffCallback<'a> = dyn FnMut(DiffType, Option<&'a Path>, Option<&'a Path>, &'a str) -> bool;

/// 对比两个目录的差异（带回调函数）
/// # 参数
/// - `base_dir`: 基准目录路径
/// - `target_dir`: 目标目录路径
/// - `callback`: 差异回调函数，返回false可中断比较
/// # 返回值
/// - `Result<(), String>`: 比较结果，成功返回Ok(())，失败返回对应的错误信息
pub fn compare_directories<F>(base_dir: impl AsRef<Path>, target_dir: impl AsRef<Path>, mut callback: F) -> Result<()>
where
    F: FnMut(DiffType, Option<&Path>, Option<&Path>, &str) -> bool,
{
    let base_dir = base_dir.as_ref();
    let target_dir = target_dir.as_ref();

    // 检查目录是否存在
    if !base_dir.exists() {
        return Err(anyhow!("Base directory does not exist: {}", base_dir.display()));
    }
    if !target_dir.exists() {
        return Err(anyhow!("Target directory does not exist: {}", target_dir.display()));
    }

    if !base_dir.is_dir() {
        return Err(anyhow!("Base path is not a directory: {}", base_dir.display()));
    }
    if !target_dir.is_dir() {
        return Err(anyhow!("Target path is not a directory: {}", target_dir.display()));
    }

    // 构建文件映射
    let mut base_files = HashMap::new();
    if let Err(err) = build_file_map(base_dir, base_dir, &mut base_files) {
        return Err(anyhow!("Failed to read base directory: {}", err));
    }

    let mut target_files = HashMap::new();
    if let Err(err) = build_file_map(target_dir, target_dir, &mut target_files) {
        return Err(anyhow!("Failed to read target directory: {}", err));
    }

    // 检查基准目录中有但目标目录中没有的文件（删除）
    for (rel_path, base_path) in &base_files {
        if !target_files.contains_key(rel_path) {
            // 调用回调函数，如果返回false则中断比较
            if !callback(DiffType::Delete, Some(base_path), None, rel_path) {
                return Err(anyhow!("Comparison interrupted by callback"));
            }
        }
    }

    // 检查目标目录中有但基准目录中没有的文件（新增）或有变化的文件（修改）
    for (rel_path, target_path) in &target_files {
        if !base_files.contains_key(rel_path) {
            // 调用回调函数，如果返回false则中断比较
            if !callback(DiffType::Add, None, Some(target_path), rel_path) {
                return Err(anyhow!("Comparison interrupted by callback"));
            }
        } else {
            let base_path = &base_files[rel_path];
            if base_path.is_file() && target_path.is_file() && !is_same_file(base_path, target_path) {
                // 调用回调函数，如果返回false则中断比较
                if !callback(DiffType::Modify, Some(base_path), Some(target_path), rel_path) {
                    return Err(anyhow!("Comparison interrupted by callback"));
                }
            }
        }
    }

    Ok(())
}

/// 构建文件映射，键为相对于根目录的路径，值为完整路径
fn build_file_map(root_dir: &Path, current_dir: &Path, file_map: &mut HashMap<String, PathBuf>) -> std::io::Result<()> {
    for entry in read_dir(current_dir)? {
        let entry = entry?;

        let path = entry.path();
        let rel_path = path
            .strip_prefix(root_dir)
            .map_err(std::io::Error::other)?
            .to_str()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Failed to convert path to string"))?
            .to_string();

        file_map.insert(rel_path.clone(), path.clone());

        // 如果是目录，递归处理
        if entry.file_type()?.is_dir() {
            build_file_map(root_dir, &path, file_map)?;
        }
    }

    Ok(())
}

/// 替换XML中指定字段的值，不依赖字段的当前值
///
/// # 参数
/// - `xml`: 输入的XML字符串
/// - `field_name`: 要替换的字段名
/// - `value`: 新的值
///
/// # 返回值
/// - `String`: 替换后的XML字符串
pub fn replace_xml_field(xml: &str, field_name: &str, value: &str) -> String {
    let start_tag = format!("<{field_name}>");
    let end_tag = format!("</{field_name}>");

    if let Some(start_pos) = xml.find(&start_tag)
        && let Some(end_pos) = xml[start_pos + start_tag.len()..].find(&end_tag)
    {
        let total_start = start_pos + start_tag.len();
        let total_end = total_start + end_pos;

        let mut result = String::with_capacity(xml.len());
        result.push_str(&xml[..total_start]);
        result.push_str(value);
        result.push_str(&xml[total_end..]);

        return result;
    }

    // 如果没有找到字段，返回原始XML
    xml.to_string()
}
