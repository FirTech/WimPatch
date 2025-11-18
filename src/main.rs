// 禁用变量命名警告
#![allow(non_snake_case)]
// 禁用未使用代码警告
#![allow(dead_code)]

use crate::cli::{App, Commands, Intrinsic, IntrinsicCommands, Language};
use crate::console::{write_console, ConsoleType};
use crate::interactive::{apply_interactive_patch, create_interactive_patch};
use crate::patch::WimPatch;
use crate::utils::{get_tmp_name, launched_from_explorer};
use anyhow::Result;
use clap::Parser;
use ::console::Term;
use rust_i18n::{set_locale, t};
use std::env::temp_dir;
use std::option::Option;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::thread::sleep;
use std::time::Duration;
use std::{fs, process};
use sys_locale::get_locale;

mod bsdiff;
mod cli;
mod console;
mod interactive;
mod manifest;
mod patch;
mod test;
mod utils;
mod wimgapi;
mod zstdiff;

rust_i18n::i18n!("locales");

static DEBUG: AtomicBool = AtomicBool::new(false);
static BUFFER_SIZE: AtomicUsize = AtomicUsize::new(65536);
static IS_TTY: OnceLock<bool> = OnceLock::new();
static TEMP_PATH: OnceLock<PathBuf> = OnceLock::new();

/// 获取临时目录路径
pub fn get_temp_path() -> &'static PathBuf {
    TEMP_PATH.get_or_init(|| temp_dir().join(get_tmp_name(".tmp", "", 6)))
}

/// 判断是否为终端
pub fn is_tty() -> bool {
    *IS_TTY.get_or_init(|| Term::stdout().features().is_attended())
}

fn main() -> Result<()> {
    // 判断是否从资源管理器启动
    if launched_from_explorer() {
        match get_locale().unwrap_or("en".into()).as_str() {
            "zh-CN" => set_locale("zh-CN"),
            "zh-TW" => set_locale("zh-TW"),
            "ja-JP" => set_locale("ja-JP"),
            _ => set_locale("en"),
        };
        println!("{}", t!("cmdline_tool_tips"));
        sleep(Duration::from_secs(5));
        return Ok(());
    }

    // 设置 Ctrl-C 信号处理
    ctrlc::set_handler(move || {
        // 删除临时目录
        fs::remove_dir_all(get_temp_path()).ok();

        // 强制退出程序
        process::exit(1);
    })
    .expect("Error setting Ctrl-C handler");

    // 处理交互模式命令行
    if let Ok(cli) = Intrinsic::try_parse() {
        set_globals(cli.debug, cli.language, cli.scratchdir, cli.buffer_size);

        // 初始化 WimPatch 实例
        let wim_patch = WimPatch::new().expect(&t!("wim_patch.new.failed"));

        let result = match cli.command {
            IntrinsicCommands::Create => match create_interactive_patch(&wim_patch) {
                Ok(()) => {
                    write_console(ConsoleType::Success, &format!("{}", t!("create_patch.success")));
                    Ok(())
                }
                Err(e) => {
                    write_console(ConsoleType::Error, &format!("{}: {:?}", t!("create_patch.failed"), e));
                    Err(e)
                }
            },
            IntrinsicCommands::Apply => match apply_interactive_patch(&wim_patch) {
                Ok(()) => {
                    write_console(ConsoleType::Success, &format!("{}", t!("apply_patch.success")));
                    Ok(())
                }
                Err(e) => {
                    write_console(ConsoleType::Error, &format!("{}: {:?}", t!("apply_patch.failed"), e));
                    Err(e)
                }
            },
        };

        // 释放WimPatch实例
        drop(wim_patch);

        // 删除临时目录
        if get_temp_path().exists()
            && let Err(e) = fs::remove_dir_all(get_temp_path())
        {
            write_console(
                ConsoleType::Warning,
                &format!("{}: {}", t!("remove_temp_dir_failed"), e),
            );
        }

        return result;
    }

    // 处理命令行
    let cli = App::parse();
    set_globals(cli.debug, cli.language, cli.scratchdir, cli.buffer_size);

    // 初始化 WimPatch 实例
    let wim_patch = WimPatch::new().expect(&t!("wim_patch.new.failed"));

    let result = match cli.command {
        // 创建补丁文件
        Commands::Create {
            base,
            index,
            mut base_index,
            target: update,
            mut target_index,
            out: patch,
            preset,
            version,
            author,
            name,
            description,
            storage,
            exclude,
            compress,
        } => {
            // 当用户指定--storage bsdiff并且还指定了--preset参数时，发出警告
            let args: Vec<String> = std::env::args().collect();
            let preset_specified = args.iter().any(|arg| arg == "--preset" || arg == "-p");
            if storage == cli::Storage::Bsdiff && preset_specified {
                write_console(ConsoleType::Warning, &format!("{}", t!("create_patch.bsdiff_preset")));
            }

            // 当用户指定--index参数时，index_base和index_updated参数等于index
            if let Some(index) = index {
                base_index = Some(index);
                target_index = Some(index);
            }

            match wim_patch.create_patch(
                &base,
                base_index,
                &update,
                target_index,
                &patch,
                &storage,
                &preset,
                &version.to_string(),
                &author,
                &name.unwrap_or(format!(
                    "{}-patch-v{}",
                    base.file_stem().unwrap().to_string_lossy(),
                    version
                )),
                &description.unwrap_or_default(),
                exclude.as_deref(),
                &compress,
            ) {
                Ok(()) => {
                    write_console(ConsoleType::Success, &format!("{}", t!("create_patch.success")));
                    Ok(())
                }
                Err(e) => {
                    write_console(ConsoleType::Error, &format!("{}: {:?}", t!("create_patch.failed"), e));
                    Err(e)
                }
            }
        }

        // 应用补丁文件
        Commands::Apply {
            base: src,
            patch,
            target,
            index,
            exclude,
            force,
        } => {
            if force {
                write_console(ConsoleType::Warning, &format!("{}", t!("apply_patch.force_warning")));
            }
            match wim_patch.apply_patch(&src, index, &patch, &target, exclude.as_deref(), force) {
                Ok(()) => {
                    write_console(ConsoleType::Success, &format!("{}", t!("apply_patch.success")));
                    Ok(())
                }
                Err(e) => {
                    write_console(ConsoleType::Error, &format!("{}: {:?}", t!("apply_patch.failed"), e));
                    Err(e)
                }
            }
        }

        // 获取补丁文件信息
        Commands::Info { patch, xml } => match wim_patch.get_patch_info(&patch, xml) {
            Ok(info) => {
                println!("{}", info);
                Ok(())
            }
            Err(e) => {
                write_console(ConsoleType::Error, &format!("{}: {:?}", t!("get_patch_info.failed"), e));
                Err(e)
            }
        },

        // 合并补丁文件
        Commands::Merge { patch, out, compress } => match wim_patch.merge_patches(&patch, &out, compress) {
            Ok(()) => {
                write_console(ConsoleType::Success, &format!("{}", t!("merge_patch.success")));
                Ok(())
            }
            Err(e) => {
                write_console(ConsoleType::Error, &format!("{}: {:?}", t!("merge_patch.failed"), e));
                Err(e)
            }
        },

        // 清理无效的挂载点
        Commands::Clean {} => match wim_patch.clean() {
            Ok(()) => {
                write_console(ConsoleType::Success, &format!("{}", t!("clean.success")));
                Ok(())
            }
            Err(e) => {
                write_console(ConsoleType::Error, &format!("{}: {:?}", t!("clean.failed"), e));
                Err(e)
            }
        },
    };

    // 释放WimPatch实例
    drop(wim_patch);

    // 删除临时目录
    if get_temp_path().exists()
        && let Err(e) = fs::remove_dir_all(get_temp_path())
    {
        write_console(
            ConsoleType::Warning,
            &format!("{}: {}", t!("remove_temp_dir_failed"), e),
        );
    }

    result
}

/// 设置全局选项
fn set_globals(debug: bool, language: Option<Language>, scratchdir: Option<PathBuf>, buffer_size: Option<usize>) {
    // 设置调试模式
    DEBUG.store(debug, Ordering::Relaxed);

    // 设置临时目录
    if let Some(path) = scratchdir {
        fs::create_dir_all(&path).unwrap();
        TEMP_PATH.get_or_init(|| path);
    }

    // 设置缓冲区大小
    if let Some(buffer_size) = buffer_size {
        BUFFER_SIZE.store(buffer_size, Ordering::Relaxed);
    }

    // 设置国际化
    if let Some(lang) = language {
        match lang {
            Language::En => set_locale("en"),
            Language::ZhCn => set_locale("zh-CN"),
            Language::ZhTw => set_locale("zh-TW"),
            Language::JaJp => set_locale("ja-JP"),
        };
    } else {
        // 获取系统语言
        let system_locale = get_locale().unwrap_or("en".into());
        match system_locale.as_str() {
            "zh-CN" => set_locale("zh-CN"),
            "zh-TW" => set_locale("zh-TW"),
            "ja-JP" => set_locale("ja-JP"),
            _ => set_locale("en"),
        };
    }
}
