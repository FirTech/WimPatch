use crate::cli::{Compress, Preset, Storage};
use crate::patch::WimPatch;
use anyhow::{Context, Result};
use dialoguer::{Confirm, Input, Select};
use rust_i18n::t;
use semver::Version;
use std::path::PathBuf;

/// 交互模式创建补丁
///
/// # 参数
///
/// - `wim_patch` - 用于创建补丁的 WimPatch 实例
///
/// # 返回值
///
/// - `Result<()>` - 如果创建补丁成功，则返回 Ok(())，否则返回错误信息
pub fn create_interactive_patch(wim_patch: &WimPatch) -> Result<()> {
    // 显示欢迎信息
    println!("{}", t!("interactive.welcome"));
    println!();

    // 获取基础 WIM 文件路径
    let base_image = loop {
        let path_input: String = Input::new()
            .with_prompt(t!("interactive.base_image_prompt"))
            .allow_empty(false)
            .interact_text()?;
        let path = PathBuf::from(path_input.trim_start_matches("\"").trim_end_matches("\""));
        if path.exists() && path.is_file() {
            break path;
        } else {
            println!("{}: {}", t!("interactive.file_not_exist"), path.display());
        }
    };

    // 获取更新 WIM 文件路径
    let target_image = loop {
        let path_input: String = Input::new()
            .with_prompt(t!("interactive.target_image_prompt"))
            .allow_empty(false)
            .interact_text()?;
        let path = PathBuf::from(path_input.trim_start_matches("\"").trim_end_matches("\""));
        if path.exists() && path.is_file() {
            break path;
        } else {
            println!("{}: {}", t!("interactive.file_not_exist"), path.display());
        }
    };

    // 获取镜像索引
    let base_image_count = wim_patch
        .get_image_count(&base_image)
        .with_context(|| "Failed to get base image count")?;

    let target_image_count = wim_patch
        .get_image_count(&target_image)
        .with_context(|| "Failed to get target image count")?;

    let (base_index, target_index) = {
        // 准备基础镜像索引选项，添加"自动匹配"选项
        let mut base_options: Vec<String> = (1..=base_image_count).map(|i| i.to_string()).collect();
        base_options.insert(0, t!("interactive.auto_match").to_string());

        // 选择基础镜像索引
        let base_selection = Select::new()
            .with_prompt(t!("interactive.base_index_prompt"))
            .default(0)
            .items(&base_options)
            .interact()?;
        let base_idx = if base_selection == 0 {
            None
        } else {
            Some(base_selection as u32)
        };

        // 准备目标镜像索引选项，添加"自动匹配"选项
        let mut target_options: Vec<String> = (1..=target_image_count).map(|i| i.to_string()).collect();
        target_options.insert(0, t!("interactive.auto_match").to_string());

        // 选择目标镜像索引
        let target_selection = Select::new()
            .with_prompt(t!("interactive.target_index_prompt"))
            .default(0)
            .items(&target_options)
            .interact()?;
        let target_idx = if target_selection == 0 {
            None
        } else {
            Some(target_selection as u32)
        };

        (base_idx, target_idx)
    };

    // 获取补丁文输出件路径
    let patch_image = loop {
        let path_input: String = Input::new()
            .with_prompt(t!("interactive.patch_image_prompt"))
            .allow_empty(false)
            .interact_text()?;
        let path = PathBuf::from(path_input.trim_start_matches("\"").trim_end_matches("\""));
        // 只检查目录是否存在，文件可以不存在
        if let Some(parent) = path.parent() {
            if parent.exists() || parent == PathBuf::from(".") {
                break path;
            } else {
                println!("{}: {}", t!("interactive.file_not_exist"), parent.display());
            }
        } else {
            break path;
        }
    };

    // 获取存储类型
    let storage_selection = Select::new()
        .with_prompt(t!("interactive.storage_options"))
        .default(0)
        .items(&[
            t!("interactive.storage_zstd"),
            t!("interactive.storage_bsdiff"),
            t!("interactive.storage_full"),
        ])
        .interact()?;

    let storage = match storage_selection {
        0 => Storage::Zstd,
        1 => Storage::Bsdiff,
        2 => Storage::Full,
        _ => Storage::Zstd,
    };

    // 获取预设配置
    let preset = if storage == Storage::Zstd {
        let preset_selection = Select::new()
            .with_prompt(t!("interactive.preset_options"))
            .default(1)
            .items(&[
                t!("interactive.preset_fast"),
                t!("interactive.preset_medium"),
                t!("interactive.preset_best"),
                t!("interactive.preset_extreme"),
            ])
            .interact()?;
        match preset_selection {
            0 => Preset::Fast,
            1 => Preset::Medium,
            2 => Preset::Best,
            3 => Preset::Extreme,
            _ => Preset::Medium,
        }
    } else {
        // 非 Zstd 存储类型默认 Medium 预设
        Preset::Medium
    };

    // 获取版本号（验证 SemVer 格式）
    let version = loop {
        let version_input: String = Input::new()
            .with_prompt(t!("interactive.version_prompt"))
            .default("1.0.0".to_string())
            .allow_empty(false)
            .interact_text()?;
        match Version::parse(&version_input) {
            Ok(v) => break v.to_string(),
            Err(_) => println!("{}: {}", t!("interactive.invalid_version"), version_input),
        }
    };

    // 获取作者名称
    let author: String = Input::new()
        .with_prompt(t!("interactive.author_prompt"))
        .default("Unknown".to_string())
        .allow_empty(false)
        .interact_text()?;

    // 获取补丁名称
    let name: String = Input::new()
        .with_prompt(t!("interactive.name_prompt"))
        .default(format!(
            "{}-patch-v{}",
            base_image.file_stem().unwrap().to_string_lossy(),
            version
        ))
        .allow_empty(true)
        .interact_text()?;

    // 获取补丁描述
    let description: String = Input::new()
        .with_prompt(t!("interactive.description_prompt"))
        .allow_empty(true)
        .interact_text()?;

    // 显示配置摘要
    println!("\n--- {} ---", t!("interactive.config_summary"));
    println!("{}: {}", t!("interactive.base_image"), base_image.display());
    println!("{}: {}", t!("interactive.target_image"), target_image.display());
    println!("{}: {}", t!("interactive.patch_image"), patch_image.display());

    // 显示索引信息
    if let (Some(base_idx), Some(target_idx)) = (base_index, target_index) {
        println!(
            "{}: {} -> {}: {}",
            t!("create_patch.index"),
            t!("create_patch.base"),
            t!("create_patch.target"),
            format!("{} -> {}", base_idx, target_idx)
        );
    } else {
        println!("{}: {}", t!("create_patch.index"), t!("interactive.auto_match"));
    }

    println!("{}: {:?}", t!("interactive.storage"), storage);
    if storage == Storage::Zstd {
        println!("{}: {:?}", t!("interactive.preset"), preset);
    }
    println!("{}: {}", t!("interactive.version"), version);
    println!("{}: {}", t!("interactive.author"), author);
    println!("{}: {}", t!("interactive.name"), name);
    println!("{}: {}", t!("interactive.description"), description);
    println!();

    // 确认创建补丁
    if !Confirm::new()
        .with_prompt(t!("interactive.confirm_create"))
        .default(true)
        .interact()?
    {
        println!("{}", t!("interactive.cancelled"));
        return Ok(());
    }

    // 调用创建补丁的方法
    wim_patch.create_patch(
        &base_image,
        base_index,
        &target_image,
        target_index,
        &patch_image,
        &storage,
        &preset,
        &version,
        &author,
        &name,
        &description,
        None,
        &Compress::Lzx,
    )
}

/// 交互式应用补丁
///
/// # 参数
///
/// - `wim_patch` - 用于应用补丁的 WimPatch 实例
///
/// # 返回值
///
/// - `Result<()>` - 如果应用补丁成功，则返回 Ok(())，否则返回错误信息
pub fn apply_interactive_patch(wim_patch: &WimPatch) -> Result<()> {
    // 显示欢迎信息
    println!("{}", t!("interactive.welcome"));
    println!();

    // 获取基础镜像路径
    let base_image = loop {
        let path_input: String = Input::new()
            .with_prompt(t!("interactive.base_image_prompt"))
            .allow_empty(false)
            .interact_text()?;
        let path = PathBuf::from(path_input.trim_start_matches("\"").trim_end_matches("\""));
        // 检查文件是否存在
        if path.exists() && path.is_file() {
            break path;
        } else {
            println!("{}: {}", t!("interactive.file_not_exist"), path.display());
        }
    };

    // 获取补丁文件路径
    let patch_image = loop {
        let path_input: String = Input::new()
            .with_prompt(t!("interactive.patch_image_path"))
            .allow_empty(false)
            .interact_text()?;
        let path = PathBuf::from(path_input.trim_start_matches("\"").trim_end_matches("\""));
        // 检查文件是否存在
        if path.exists() && path.is_file() {
            break path;
        } else {
            println!("{}: {}", t!("interactive.file_not_exist"), path.display());
        }
    };

    // 获取目标镜像路径
    let target_image = loop {
        let path_input: String = Input::new()
            .with_prompt(t!("interactive.target_image_prompt"))
            .allow_empty(false)
            .interact_text()?;
        let path = PathBuf::from(path_input.trim_start_matches("\"").trim_end_matches("\""));
        // 只检查目录是否存在，文件可以不存在
        if let Some(parent) = path.parent() {
            if parent.exists() || parent == PathBuf::from(".") {
                break path;
            } else {
                println!("{}: {}", t!("interactive.file_not_exist"), parent.display());
            }
        } else {
            break path;
        }
    };

    // 获取基础镜像数量
    let base_image_count = wim_patch.get_image_count(&base_image)?;

    let base_index = {
        // 准备基础镜像索引选项，添加"自动匹配"选项
        let mut base_options: Vec<String> = (1..=base_image_count).map(|i| i.to_string()).collect();
        base_options.insert(0, t!("interactive.auto_match").to_string());

        // 选择基础镜像索引
        let base_selection = Select::new()
            .with_prompt(t!("interactive.base_index_prompt"))
            .default(0)
            .items(&base_options)
            .interact()?;

        if base_selection == 0 {
            None
        } else {
            Some(base_selection as u32)
        }
    };

    // 获取是否强制应用补丁
    let force: bool = Input::new()
        .with_prompt(t!("interactive.force_apply_prompt"))
        .default(false)
        .interact()?;

    // 显示配置摘要
    println!("\n--- {} ---", t!("interactive.config_summary"));
    println!("{}: {}", t!("interactive.base_image"), base_image.display());
    if let Some(index) = base_index {
        println!("{}: {}", t!("create_patch.index"), index);
    } else {
        println!("{}: {}", t!("create_patch.index"), t!("interactive.auto_match"));
    }
    println!("{}: {}", t!("interactive.target_image"), target_image.display());
    println!("{}: {}", t!("interactive.patch_image"), patch_image.display());
    println!("{}: {}", t!("interactive.force_apply"), force);

    // 确认应用补丁
    if !Confirm::new()
        .with_prompt(t!("interactive.confirm_apply"))
        .default(true)
        .interact()?
    {
        println!("{}", t!("interactive.cancelled"));
        return Ok(());
    }

    // 调用应用补丁的方法
    wim_patch.apply_patch(&base_image, base_index, &patch_image, &target_image, None, force)
}
