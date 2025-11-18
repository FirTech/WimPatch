use clap::{Parser, Subcommand, ValueEnum};
use semver::Version;
use std::path::PathBuf;

/// Language options
#[derive(Debug, Clone, ValueEnum)]
pub enum Language {
    /// English language
    En,
    /// Simplified Chinese
    ZhCn,
    /// Traditional Chinese
    ZhTw,
    /// Japanese
    JaJp,
}

#[derive(Parser, Debug)]
#[clap(version)]
#[clap(propagate_version = false)]
#[command(disable_version_flag = false, arg_required_else_help = true)]
pub struct App {
    #[command(subcommand)]
    pub(crate) command: Commands,

    /// 缓冲区大小（单位：字节）
    #[clap(help = "Buffer size in bytes [default: 65536]")]
    #[clap(long)]
    pub(crate) buffer_size: Option<usize>,

    /// 调试模式
    #[clap(help = "Debug mode")]
    #[clap(long)]
    pub(crate) debug: bool,

    /// 临时目录路径
    #[clap(help = "Scratch directory")]
    #[clap(long)]
    pub(crate) scratchdir: Option<PathBuf>,

    /// 设置程序语言
    #[clap(help = "Set program language")]
    #[clap(long, value_enum)]
    pub(crate) language: Option<Language>,
}

#[derive(Parser, Debug)]
#[clap(version)]
#[clap(propagate_version = false)]
#[command(disable_version_flag = false, arg_required_else_help = true)]
pub struct Intrinsic {
    #[command(subcommand)]
    pub(crate) command: IntrinsicCommands,

    /// 缓冲区大小（单位：字节）
    #[clap(help = "Buffer size in bytes [default: 65536]")]
    #[clap(long)]
    pub(crate) buffer_size: Option<usize>,

    /// 调试模式
    #[clap(help = "Debug mode")]
    #[clap(long)]
    pub(crate) debug: bool,

    /// 临时目录路径
    #[clap(help = "Scratch directory")]
    #[clap(long)]
    pub(crate) scratchdir: Option<PathBuf>,

    /// 设置程序语言
    #[clap(help = "Set program language")]
    #[clap(long, value_enum)]
    pub(crate) language: Option<Language>,
}

#[derive(Subcommand, Debug)]
pub enum IntrinsicCommands {
    Create,
    Apply,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create image patch file
    Create {
        /// 源镜像文件路径
        #[clap(help = "base wim image file path")]
        #[clap(short, long, value_parser = exist_file_parser)]
        base: PathBuf,

        /// 镜像索引
        #[clap(help = "Index of the image in the wim file")]
        #[arg(short, long = "index", conflicts_with_all = ["base_index", "target_index"])]
        index: Option<u32>,

        /// 源镜像索引
        #[clap(help = "Index of the image in the base wim file")]
        #[arg(long = "base-index", requires = "target_index", conflicts_with = "index")]
        base_index: Option<u32>,

        /// 更新镜像文件路径
        #[clap(help = "Target wim image file path")]
        #[clap(short, long, value_parser = exist_file_parser)]
        target: PathBuf,

        /// 更新镜像索引
        #[clap(help = "Index of the image in the target wim file")]
        #[arg(long = "target-index", requires = "base_index", conflicts_with = "index")]
        target_index: Option<u32>,

        /// 输出补丁文件路径
        #[clap(help = "Out patch file path")]
        #[clap(short, long)]
        out: PathBuf,

        /// 压缩算法
        #[clap(help = "Compression algorithm")]
        #[clap(short, long, value_enum, default_value_t = Compress::Lzx)]
        compress: Compress,

        /// 存储类型
        #[clap(help = "Storage type of the patch file")]
        #[clap(short = 's', long,value_enum, default_value_t = Storage::Zstd)]
        storage: Storage,

        /// 压缩级别
        #[clap(help = "Compression level")]
        #[clap(short = 'p', long, value_enum, default_value_t = Preset::Medium)]
        preset: Preset,

        /// 补丁文件版本
        #[clap(help = "Version of the patch file")]
        #[clap(short, long, value_parser = parse_version)]
        version: Version,

        /// 补丁文件作者
        #[clap(help = "Author of the patch file")]
        #[clap(short, long, default_value = "unknown")]
        author: String,

        /// 补丁文件名称
        #[clap(help = "Name of the patch file")]
        #[clap(short, long)]
        name: Option<String>,

        /// 补丁文件描述
        #[clap(help = "Description of the patch file")]
        #[clap(short, long)]
        description: Option<String>,

        /// 排除文件
        #[clap(help = "Exclude files from the patch file")]
        #[clap(short, long)]
        exclude: Option<Vec<String>>,
    },

    /// Apply image patch file
    Apply {
        /// 源镜像文件路径
        #[clap(help = "Original wim image file path")]
        #[clap(short, long, value_parser = exist_file_parser)]
        base: PathBuf,

        /// 补丁文件路径
        #[clap(help = "Patch file path")]
        #[clap(short, long, value_parser = exist_file_parser)]
        patch: PathBuf,

        /// 目标镜像文件路径
        #[clap(help = "Output image path after applying patch (target image)")]
        #[clap(short, long)]
        target: PathBuf,

        /// 源镜像索引
        #[clap(help = "Index of the image in the base wim file")]
        #[clap(short, long)]
        index: Option<u32>,

        /// 排除文件
        #[clap(help = "Exclude files from the patch file")]
        #[clap(short, long)]
        exclude: Option<Vec<String>>,

        /// 强制应用补丁
        #[clap(help = "Force apply patch")]
        #[clap(short, long)]
        force: bool,
    },

    /// Merge multiple incremental patches into one merge patch
    Merge {
        /// 补丁文件路径
        #[clap(help = "Patch file path")]
        patch: Vec<PathBuf>,

        /// 输出补丁文件路径
        #[clap(help = "Out patch file path")]
        #[clap(short, long)]
        out: PathBuf,

        /// 压缩算法
        #[clap(help = "Compression algorithm")]
        #[clap(short, long, value_enum, default_value_t = Compress::Lzx)]
        compress: Compress,
    },

    /// Get patch file info
    Info {
        /// 补丁文件路径
        #[clap(help = "Patch file path")]
        patch: PathBuf,

        /// 输出XML
        #[clap(help = "Out print patch info as xml")]
        #[clap(short, long)]
        xml: bool,
    },

    /// Cleanup invalid mount
    Clean {},
}

/// Compression preset
#[derive(Debug, Clone, ValueEnum)]
pub enum Preset {
    /// Fast compression
    Fast,
    /// Medium compression
    Medium,
    /// Best compression
    Best,
    /// Extreme compression
    Extreme,
}

/// Storage type
#[derive(Debug, Clone, ValueEnum, PartialEq)]
pub enum Storage {
    /// Full storage
    Full,
    /// Zstd compressed storage
    Zstd,
    /// BSDiff differential storage
    Bsdiff,
}

/// Compression algorithm
#[derive(Debug, Clone, ValueEnum, PartialEq, Copy)]
pub enum Compress {
    /// No compression
    None,
    /// Xpress compression
    Xpress,
    /// Lzx compression
    Lzx,
}

/// 用于 clap 参数解析：验证路径必须为已存在文件。
///
/// # 参数:
/// - `s`: 命令行中传入的字符串路径。
///
/// # 返回值:
/// - `Ok(PathBuf)`: 如果字符串成功解析为 PathBuf 且该路径是已存在的文件。
/// - `Err(String)`: 如果解析失败或路径不是已存在的文件，返回错误信息。
fn exist_file_parser(s: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);

    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }

    if !path.is_file() {
        return Err(format!("Path is not a file: {}", path.display()));
    }

    Ok(path)
}

/// 用于 clap 参数解析：验证路径必须为已存在目录。
///
/// # 参数:
/// - `s`: 命令行中传入的字符串路径。
///
/// # 返回值:
/// - `Ok(PathBuf)`: 如果字符串成功解析为 PathBuf 且该路径是已存在的目录。
/// - `Err(String)`: 如果解析失败或路径不是已存在的目录，返回错误信息。
fn exist_dir_parser(s: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);

    if !path.exists() {
        return Err(format!("Dir does not exist: {}", path.display()));
    }

    if !path.is_dir() {
        return Err(format!("Path is not a dir: {}", path.display()));
    }

    Ok(path)
}

/// 用于 clap 参数解析：验证字符串是否为有效的 semver 版本号。
///
/// # 参数:
/// - `s`: 命令行中传入的字符串版本号。
///
/// # 返回值:
/// - `Ok(Version)`: 如果字符串成功解析为 semver 版本号。
/// - `Err(semver::Error)`: 如果解析失败，返回 semver 错误。
fn parse_version(s: &str) -> Result<Version, semver::Error> {
    let v = Version::parse(s)?;
    Ok(v)
}
