// 禁用变量命名警告
#![allow(non_snake_case)]
// 禁用未使用代码警告
#![allow(dead_code)]

use crate::cli::{Cli, Commands};
use crate::console::{write_console, ConsoleType};
use crate::patch::WimPatch;
use crate::utils::{get_tmp_name, launched_from_explorer};
use clap::Parser;
use ::console::Term;
use lazy_static::lazy_static;
use rust_embed::Embed;
use rust_i18n::{set_locale, t};
use std::env::temp_dir;
use std::fs;
use std::option::Option;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::sleep;
use std::time::Duration;
use sys_locale::get_locale;

mod bsdiff;
mod cli;
mod console;
mod manifest;
mod patch;
mod test;
mod utils;
mod wimgapi;
mod zstdiff;
// 设置静态资源

// x64平台
#[cfg(target_arch = "x86_64")]
#[derive(Embed)]
#[folder = "./assets-x64"]
pub struct Asset;

// x86平台
#[cfg(target_arch = "x86")]
#[derive(Embed)]
#[folder = "./assets-x86"]
pub struct Asset;

rust_i18n::i18n!("locales");

static DEBUG: AtomicBool = AtomicBool::new(false);
static BUFFER_SIZE: AtomicUsize = AtomicUsize::new(65536);

lazy_static! {
    pub static ref TEMP_PATH: PathBuf = temp_dir().join(get_tmp_name(".tmp", "", 6));
    pub static ref IS_TTY: bool = Term::stdout().features().is_attended();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 设置国际化
    let system_locale = get_locale().unwrap_or("en".into());
    match system_locale.as_str() {
        "zh-CN" => set_locale("zh-CN"),
        _ => set_locale("en"),
    };

    // 判断是否从资源管理器启动
    if launched_from_explorer() {
        println!("{}", t!("cmdline_tool_tips"));
        sleep(Duration::from_secs(5));
        return Ok(());
    }

    // 处理命令行
    let cli = Cli::parse();
    if cli.debug {
        DEBUG.store(true, Ordering::Relaxed);
    }
    if let Some(buffer_size) = cli.buffer_size {
        BUFFER_SIZE.store(buffer_size, Ordering::Relaxed);
    }

    // 初始化 WimPatch 实例
    let wim_patch = WimPatch::new().expect(&t!("wim_patch.new.failed"));

    match cli.command {
        // 创建补丁文件
        Commands::Create {
            base,
            index,
            updated: update,
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
                write_console(
                    ConsoleType::Warning,
                    &format!("{}", t!("create_patch.bsdiff_preset")),
                );
            }

            match wim_patch.create_patch(
                &base,
                index,
                &update,
                index,
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
                compress,
            ) {
                Ok(()) => {
                    write_console(
                        ConsoleType::Success,
                        &format!("{}", t!("create_patch.success")),
                    );
                }

                Err(e) => {
                    write_console(
                        ConsoleType::Error,
                        &format!("{}: {:?}", t!("create_patch.failed"), e),
                    );
                }
            }
        }

        // 应用补丁文件
        Commands::Apply {
            base: src,
            patch,
            target,
            index: src_index,
            exclude,
            force,
        } => {
            match wim_patch.apply_patch(&src, src_index, &patch, &target, exclude.as_deref(), force)
            {
                Ok(()) => {
                    write_console(
                        ConsoleType::Success,
                        &format!("{}", t!("apply_patch.success")),
                    );
                }
                Err(e) => {
                    write_console(
                        ConsoleType::Error,
                        &format!("{}: {:?}", t!("apply_patch.failed"), e),
                    );
                }
            }
        }

        // 获取补丁文件信息
        Commands::Info { patch, xml } => match wim_patch.get_patch_info(&patch, xml) {
            Ok(info) => {
                println!("{}", info);
            }
            Err(e) => {
                write_console(
                    ConsoleType::Error,
                    &format!("{}: {:?}", t!("get_patch_info.failed"), e),
                );
            }
        },

        // 合并补丁文件
        Commands::Merge { patch, out } => match wim_patch.merge_patches(&patch, &out) {
            Ok(()) => {
                write_console(
                    ConsoleType::Success,
                    &format!("{}", t!("merge_patch.success")),
                );
            }
            Err(e) => {
                write_console(
                    ConsoleType::Error,
                    &format!("{}: {:?}", t!("merge_patch.failed"), e),
                );
            }
        },
        Commands::Cleanup {} => match wim_patch.cleanup() {
            Ok(()) => {
                write_console(ConsoleType::Success, &format!("{}", t!("cleanup.success")));
            }
            Err(e) => {
                write_console(
                    ConsoleType::Error,
                    &format!("{}: {:?}", t!("cleanup.failed"), e),
                );
            }
        },
    }

    // 释放WimPatch实例
    drop(wim_patch);

    // 删除临时目录
    if TEMP_PATH.exists()
        && let Err(e) = fs::remove_dir_all(&*TEMP_PATH)
    {
        write_console(
            ConsoleType::Warning,
            &format!("{}: {}", t!("remove_temp_dir_failed"), e),
        );
    }
    Ok(())
}
