use crate::bsdiff::BsDiff;
use crate::cli::{Compress, Preset, Storage};
use crate::console::{write_console, ConsoleType};
use crate::manifest::{ImageInfo, Operations, PatchManifest};
use crate::utils::{compare_directories, human_bytes, replace_xml_field, writeEmbedFile, DiffType};
use crate::wimgapi::{
    WimMountInfoLevel1, Wimgapi, WIM_COMPRESS_LZX, WIM_COMPRESS_NONE,
    WIM_COMPRESS_XPRESS, WIM_CREATE_ALWAYS, WIM_FLAG_MOUNT_READONLY, WIM_GENERIC_MOUNT,
    WIM_GENERIC_READ, WIM_GENERIC_WRITE, WIM_MOUNT_FLAG_INVALID, WIM_MSG_PROCESS,
    WIM_MSG_PROGRESS, WIM_OPEN_EXISTING,
};
use crate::zstdiff::ZstdDiff;
use crate::{IS_TTY, TEMP_PATH};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Local};
use indicatif::MultiProgress;
use indicatif::{ProgressBar, ProgressStyle};
use rust_i18n::t;
use std::path::{Path, PathBuf};
use std::string::String;
use std::time::Duration;
use std::{fs, ptr};

pub struct WimPatch {
    wimgapi: Wimgapi,
}

impl WimPatch {
    /// 初始化 WimPatch 实例
    pub fn new() -> Result<Self> {
        if !TEMP_PATH.exists() {
            fs::create_dir_all(&*TEMP_PATH).with_context(|| t!("create_temp_dir.failed"))?;
        }

        let path = PathBuf::from(&TEMP_PATH.join("wimgapi.dll"));
        if !path.exists() {
            writeEmbedFile("wimgapi.dll", &path)
                .with_context(|| format!("Failed to write wimgapi.dll to {}", path.display()))?;
        }
        let wimgapi = Wimgapi::new(Some(path.clone()))
            .with_context(|| format!("Failed to load wimgapi.dll from {}", path.display()))?;
        Ok(Self { wimgapi })
    }

    /// 解析补丁包的清单信息
    ///
    /// # 参数
    ///
    /// * `image_info` - 包含补丁包镜像信息的字符串
    ///
    /// # 返回值
    ///
    /// * `Ok(PatchManifest)` - 解析成功，返回补丁清单
    /// * `Err(anyhow::Error)` - 解析失败，返回错误信息
    pub fn parse_patch_info(&self, image_info: &str) -> Result<PatchManifest> {
        // 解析PatchManifest
        if let (Some(start), Some(end)) = (
            image_info.find("<PatchManifest>"),
            image_info.find("</PatchManifest>"),
        ) {
            let manifest_xml = &image_info[start..end + "</PatchManifest>".len()];
            match PatchManifest::from_xml(manifest_xml) {
                Ok(manifest) => Ok(manifest),
                Err(e) => Err(anyhow!("{}: {:?}", t!("parse_patch.failed"), e)),
            }
        } else {
            Err(anyhow!("{}", t!("parse_patch.not_found_manifest")))
        }
    }

    /// 获取补丁包的清单信息并打印
    ///
    /// # 参数
    ///
    /// * `patch` - 补丁包文件路径
    /// * `out_xml` - 是否输出 XML 格式的清单信息
    ///
    /// # 返回值
    ///
    /// * `Ok(String)` - 成功，返回清单信息字符串
    /// * `Err(anyhow::Error)` - 失败，返回错误信息
    pub fn get_patch_info(&self, patch: &Path, out_xml: bool) -> Result<String> {
        // 打开补丁包
        let patch_handle = self
            .wimgapi
            .open(
                patch,
                WIM_GENERIC_READ,
                WIM_OPEN_EXISTING,
                WIM_COMPRESS_NONE,
            )
            .with_context(|| format!("Open patch image {} failed", patch.display()))?;

        self.wimgapi
            .set_temp_path(patch_handle, &TEMP_PATH)
            .with_context(|| "Set temp path failed")?;

        let mut result = String::new();
        for index in 1..=self.wimgapi.get_image_count(patch_handle) {
            let image_handle = self
                .wimgapi
                .load_image(patch_handle, index)
                .with_context(|| format!("Load image from patch image failed, index: {}", index))?;

            // 获取补丁包的镜像信息
            let image_info = self
                .wimgapi
                .get_image_info(image_handle)
                .with_context(|| "Get image info from patch image failed".to_string())?;

            self.wimgapi
                .close(image_handle)
                .with_context(|| "Close patch image failed".to_string())?;

            // 解析PatchManifest
            let manifest = self.parse_patch_info(&image_info)?;

            if out_xml {
                result.push_str(&manifest.to_xml().unwrap());
                result.push('\n');
                continue;
            }
            let label_w = 18;
            let total_w = label_w + patch.display().to_string().len() + 1;
            result.push_str("Patch Summary:\n");
            result.push_str(&format!("{:-^total_w$}\n", "-"));
            result.push_str(&format!("{:<label_w$} {}\n", "File:", patch.display()));
            result.push_str(&format!("{:<label_w$} {}\n", "Index:", index));
            result.push_str(&format!("{:<label_w$} {}\n", "UUID:", manifest.id));
            result.push_str(&format!(
                "{:<label_w$} {}\n",
                "Size:",
                human_bytes(patch.metadata().unwrap().len())
            ));
            result.push_str(&format!(
                "{:<label_w$} {}\n",
                "Version:", manifest.patch_version
            ));
            result.push_str(&format!("{:<label_w$} {}\n", "Name:", manifest.name));
            result.push_str(&format!("{:<label_w$} {}\n", "Author:", manifest.author));
            result.push_str(&format!(
                "{:<label_w$} {}\n",
                "Description:", manifest.description
            ));
            result.push_str(&format!(
                "{:<label_w$} {}\n",
                "Tool Version:", manifest.tool_version
            ));
            if let Ok(utc_time) = DateTime::parse_from_rfc3339(&manifest.timestamp) {
                // 转换为本地时间
                let local_time = utc_time.with_timezone(&Local);
                result.push_str(&format!(
                    "{:<label_w$} {}\n",
                    "created:",
                    local_time.format("%Y-%m-%d %H:%M:%S")
                ));
            } else {
                // 如果解析失败，回退到原始格式
                result.push_str(&format!(
                    "{:<label_w$} {}\n",
                    "created:", manifest.timestamp
                ));
            }

            // 显示操作统计
            let total = manifest.operations.adds.len()
                + manifest.operations.modifies.len()
                + manifest.operations.deletes.len();
            result.push_str(&format!(
                "{:<label_w$} +{} / ~{} / -{} (total: {})\n",
                "Operations:",
                manifest.operations.adds.len(),
                manifest.operations.modifies.len(),
                manifest.operations.deletes.len(),
                total
            ));

            // 显示基础镜像信息
            result.push_str("\nBase Image Information:\n");
            result.push_str(&format!("{:-^total_w$}\n", "-"));
            result.push_str(&format!(
                "{:<label_w$} {}\n",
                "Index:", manifest.base_image_info.index
            ));

            if let Some(name) = &manifest.base_image_info.name {
                result.push_str(&format!("{:<label_w$} {}\n", "Name:", name));
            }
            if let Some(display_name) = &manifest.base_image_info.display_name {
                result.push_str(&format!("{:<label_w$} {}\n", "Display Name:", display_name));
            }
            if let Some(flags) = &manifest.base_image_info.flags {
                result.push_str(&format!("{:<label_w$} {}\n", "Flags:", flags));
            }
            result.push_str(&format!(
                "{:<label_w$} {}\n",
                "Dir Count:", manifest.base_image_info.dir_count
            ));
            result.push_str(&format!(
                "{:<label_w$} {}\n",
                "File Count:", manifest.base_image_info.file_count
            ));
            result.push_str(&format!(
                "{:<label_w$} {}\n",
                "Hard Link Bytes:", manifest.base_image_info.hard_link_bytes
            ));
            result.push_str(&format!(
                "{:<label_w$} {}\n",
                "Total Bytes:",
                human_bytes(manifest.base_image_info.total_bytes)
            ));

            // 显示更新镜像信息
            result.push_str("\nTarget Image Information:\n");
            result.push_str(&format!("{:-^total_w$}\n", "-"));
            result.push_str(&format!(
                "{:<label_w$} {}\n",
                "Index:", manifest.target_image_info.index
            ));
            if let Some(name) = &manifest.target_image_info.name {
                result.push_str(&format!("{:<label_w$} {}\n", "Name:", name));
            }
            if let Some(display_name) = &manifest.target_image_info.display_name {
                result.push_str(&format!("{:<label_w$} {}\n", "Display Name:", display_name));
            }
            if let Some(flags) = &manifest.target_image_info.flags {
                result.push_str(&format!("{:<label_w$} {}\n", "Flags:", flags));
            }
            result.push_str(&format!(
                "{:<label_w$} {}\n",
                "Dir Count:", manifest.target_image_info.dir_count
            ));
            result.push_str(&format!(
                "{:<label_w$} {}\n",
                "File Count:", manifest.target_image_info.file_count
            ));
            result.push_str(&format!(
                "{:<label_w$} {}\n",
                "Total Bytes:",
                human_bytes(manifest.target_image_info.total_bytes)
            ));

            result.push('\n');
        }
        self.wimgapi
            .close(patch_handle)
            .with_context(|| "Close patch failed".to_string())?;
        Ok(result)
    }

    /// 创建补丁包
    pub fn create_patch(
        &self,
        base_image: &Path,
        base_index: u32,
        updated_image: &Path,
        updated_index: u32,
        patch_image: &Path,
        storage: &Storage,
        preset: &Preset,
        version: &str,
        author: &str,
        name: &str,
        description: &str,
        exclude: Option<&[String]>,
        compress: Compress,
    ) -> Result<()> {
        let base_mount = TEMP_PATH.join("base");
        fs::create_dir_all(&base_mount)?;

        let update_mount = TEMP_PATH.join("updated");
        fs::create_dir_all(&update_mount)?;

        let patch_dir = TEMP_PATH.join("patch");
        fs::create_dir_all(&patch_dir)?;

        // 创建进度条管理器
        let multi_pb = MultiProgress::new();

        // 创建主进度条
        let main_pb = multi_pb.add(ProgressBar::new(6));
        main_pb.set_style(
            ProgressStyle::with_template(
                "{prefix:.bold.dim} [{elapsed_precise}] [{bar}] {pos}/{len}: {msg}",
            )
            .unwrap()
            .progress_chars("=> "),
        );
        main_pb.enable_steady_tick(Duration::from_millis(80));

        // 挂载原镜像文件
        main_pb.inc(1);
        main_pb.set_message(t!("create_patch.mount_base"));
        // 非TTY环境下输出子进度条消息
        if !*IS_TTY {
            println!("{}", t!("create_patch.mount_base"));
        }
        let base_handle = self.wimgapi.open(
            base_image,
            WIM_GENERIC_READ | WIM_GENERIC_MOUNT,
            WIM_OPEN_EXISTING,
            WIM_COMPRESS_NONE,
        )?;

        self.wimgapi.set_temp_path(base_handle, &TEMP_PATH)?;
        let base_image_handle = self.wimgapi.load_image(base_handle, base_index)?;

        if let Err(e) =
            self.wimgapi
                .mount_image_handle(base_image_handle, &base_mount, WIM_FLAG_MOUNT_READONLY)
        {
            return Err(anyhow!("Mount base image error: {:?}", e));
        }

        // 挂载更新镜像文件
        main_pb.inc(1);
        main_pb.set_message(t!("create_patch.mount_updated"));
        // 非TTY环境下输出子进度条消息
        if !*IS_TTY {
            println!("{}", t!("create_patch.mount_updated"));
        }

        let update_handle = self.wimgapi.open(
            updated_image,
            WIM_GENERIC_READ | WIM_GENERIC_MOUNT,
            WIM_OPEN_EXISTING,
            WIM_COMPRESS_NONE,
        )?;
        self.wimgapi.set_temp_path(update_handle, &TEMP_PATH)?;
        let update_image_handle = self.wimgapi.load_image(update_handle, updated_index)?;
        if let Err(e) = self.wimgapi.mount_image_handle(
            update_image_handle,
            &update_mount,
            WIM_FLAG_MOUNT_READONLY,
        ) {
            return Err(anyhow!("Mount update image error: {:?}", e));
        }

        let mut config = PatchManifest::new(name, description, author, version);

        // 获取基础镜像的卷信息
        let base_image_manifest = self.wimgapi.get_image_info(base_image_handle)?;
        let base_image_info = ImageInfo::from_str(&base_image_manifest)
            .with_context(|| "Failed to parse base image info")?;
        config.base_image_info = base_image_info;

        // 获取更新镜像的卷信息
        let update_image_manifest = self.wimgapi.get_image_info(update_image_handle)?;
        let update_image_info = ImageInfo::from_str(&update_image_manifest)
            .with_context(|| "Failed to parse update image info")?;
        config.target_image_info = update_image_info;

        // 比较差异
        main_pb.inc(1);
        main_pb.set_message(t!("create_patch.compare_diff"));
        // 非TTY环境下输出子进度条消息
        if !*IS_TTY {
            println!("{}", t!("create_patch.compare_diff"));
        }

        // 创建子进度条（用于显示具体操作进度）
        let sub_pb = multi_pb.add(ProgressBar::new(100));
        sub_pb.set_style(
            ProgressStyle::with_template("{prefix:.bold.dim} {spinner} {wide_msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        sub_pb.enable_steady_tick(Duration::from_millis(80));

        // 比较目录差异
        compare_directories(&base_mount, &update_mount, |diff_type, old, new, path| {
            // 检查是否需要排除
            if let Some(exclude) = exclude {
                for item in exclude {
                    if path
                        .to_ascii_lowercase()
                        .contains(&item.to_ascii_lowercase())
                    {
                        sub_pb.set_message(format!("{} \\{}", t!("create_patch.exclude"), path));
                        return true;
                    }
                }
            }

            // 更新进度条消息
            let message = match diff_type {
                DiffType::Added => format!("{} \\{}", t!("create_patch.Add"), path),
                DiffType::Removed => format!("{} \\{}", t!("create_patch.Delete"), path),
                DiffType::Modified => format!("{} \\{}", t!("create_patch.Modify"), path),
            };
            sub_pb.set_message(message.clone());

            // 非TTY环境下输出动态消息
            if !*IS_TTY {
                println!("{}", message);
            }

            // 构造补丁
            match diff_type {
                DiffType::Added => {
                    if let Some(new_path) = new {
                        config
                            .operations
                            .add_add(path.to_string(), new_path.metadata().unwrap().len());

                        // 确保patch目录存在
                        let target_path = patch_dir.join(path);
                        if new_path.is_dir() {
                            if let Err(e) = fs::create_dir_all(&target_path) {
                                eprintln!("Create directory Failed: {:?}", e);
                            }
                            return true;
                        }
                        // 创建父目录
                        if let Some(parent) = target_path.parent()
                            && !parent.exists()
                            && let Err(e) = fs::create_dir_all(parent)
                        {
                            eprintln!("Create directory Failed: {:?}", e);
                        }
                        // 复制新增的文件到patch目录
                        if let Err(e) = fs::copy(new_path, &target_path) {
                            eprintln!("Copy file Failed: {:?}", e);
                        }
                    }
                }
                DiffType::Removed => {
                    config.operations.add_delete(path.to_string());
                }
                DiffType::Modified => {
                    // 确保patch目录存在
                    if let Some(old_path) = old
                        && let Some(new_path) = new
                    {
                        // 创建父目录
                        if let Some(parent) = patch_dir.join(path).parent()
                            && !parent.exists()
                            && let Err(e) = fs::create_dir_all(parent)
                        {
                            eprintln!("Create directory Failed: {:?}", e);
                        }
                        match storage {
                            Storage::Full => {
                                // 复制修改前的文件到patch目录
                                if let Err(e) = fs::copy(old_path, patch_dir.join(path)) {
                                    eprintln!("Copy file Failed: {:?}", e);
                                }

                                config.operations.add_modify(
                                    path.to_string(),
                                    old_path.metadata().unwrap().len(),
                                    "full".to_string(),
                                );
                            }
                            Storage::Zstd => {
                                // 生成zstd差异文件
                                if let Err(e) = ZstdDiff::file_diff(
                                    old_path,
                                    new_path,
                                    patch_dir.join(format!("{}.diff", path)),
                                    match preset {
                                        Preset::Fast => 3,
                                        Preset::Medium => 9,
                                        Preset::Best => 19,
                                        Preset::Extreme => 22,
                                    },
                                ) {
                                    eprintln!("Create diff file Failed: {:?}", e);
                                }
                                config.operations.add_modify(
                                    path.to_string(),
                                    old_path.metadata().unwrap().len(),
                                    "zstd".to_string(),
                                );
                            }
                            Storage::Bsdiff => {
                                // 生成bsdiff差异文件
                                if let Err(e) = BsDiff::file_diff(
                                    old_path,
                                    new_path,
                                    patch_dir.join(format!("{}.diff", path)),
                                ) {
                                    eprintln!("Create diff file Failed: {:?}", e);
                                }

                                config.operations.add_modify(
                                    path.to_string(),
                                    old_path.metadata().unwrap().len(),
                                    "bsdiff".to_string(),
                                );
                            }
                        }
                    }
                }
            }
            true
        })?;

        // 完成子进度条
        sub_pb.finish_and_clear();

        main_pb.inc(1);
        main_pb.set_message(t!("create_patch.create_patch"));
        // 非TTY环境下输出子进度条消息
        if !*IS_TTY {
            println!("{}", t!("create_patch.create_patch"));
        }

        let handle = match self.wimgapi.open(
            patch_image,
            WIM_GENERIC_WRITE,
            WIM_CREATE_ALWAYS,
            match compress {
                Compress::None => WIM_COMPRESS_NONE,
                Compress::Xpress => WIM_COMPRESS_XPRESS,
                Compress::Lzx => WIM_COMPRESS_LZX,
            },
        ) {
            Ok(h) => h,
            Err(e) => {
                self.wimgapi.unmount_image_handle(base_image_handle)?;
                self.wimgapi.close(base_image_handle)?;
                self.wimgapi.close(base_handle)?;

                self.wimgapi.unmount_image_handle(update_image_handle)?;
                self.wimgapi.close(update_image_handle)?;
                self.wimgapi.close(update_handle)?;

                return Err(anyhow!("Open patch file error ({})", e));
            }
        };

        // 创建补丁文件回调函数
        extern "system" fn CreatePatchCallback(
            dwMessageId: u32,
            wParam: usize,
            lParam: isize,
            _pvUserData: *mut std::ffi::c_void,
        ) -> u32 {
            match dwMessageId {
                // 进度回调
                WIM_MSG_PROGRESS => {
                    // println!("进度: {}, 剩余: {}秒", wParam, lParam / 1000);
                }
                // 处理回调
                WIM_MSG_PROCESS => {
                    if wParam != 0 {
                        let path_ptr = wParam as *mut u16;
                        let path_str = unsafe {
                            let mut len = 0;
                            while *path_ptr.offset(len) != 0 {
                                len += 1;
                            }
                            String::from_utf16_lossy(std::slice::from_raw_parts(
                                path_ptr,
                                len as usize,
                            ))
                        };

                        let exclude_paths = [
                            "$ntfs.log",
                            "hiberfil.sys",
                            "pagefile.sys",
                            "swapfile.sys",
                            "System Volume Information",
                            "RECYCLER",
                            "Windows\\CSC",
                        ];

                        for exclude_path in &exclude_paths {
                            if path_str
                                .to_ascii_lowercase()
                                .contains(&exclude_path.to_ascii_lowercase())
                            {
                                let p_bool = lParam as *mut i32;
                                if !p_bool.is_null() {
                                    unsafe {
                                        ptr::write(p_bool, 0);
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            // 返回0表示继续处理
            0
        }

        // 注册消息回调函数
        self.wimgapi
            .register_message_callback(handle, CreatePatchCallback);
        // 捕获镜像
        let hImage = self
            .wimgapi
            .capture(handle, &patch_dir, 0)
            .with_context(|| "Capture patch image error")?;

        // 注销消息回调函数
        self.wimgapi
            .unregister_message_callback(handle, CreatePatchCallback);

        // 在</IMAGE>标签前添加基本字段信息
        let image_info = self
            .wimgapi
            .get_image_info(hImage)
            .with_context(|| "Get image info error")?;
        let updated_image_info = if let Some(pos) = image_info.rfind("</IMAGE>") {
            let prefix = &image_info[..pos];
            let suffix = &image_info[pos..];
            format!(
                "{}<NAME>{}</NAME>\
                <DESCRIPTION>{}</DESCRIPTION>\
                <DISPLAYNAME>{}</DISPLAYNAME>\
                <DISPLAYDESCRIPTION>{}</DISPLAYDESCRIPTION>\
                <FLAGS></FLAGS>{}{}",
                prefix,
                config.name,
                config.description,
                config.name,
                config.description,
                config.to_xml()?,
                suffix
            )
        } else {
            // 错误: 没找到</IMAGE>标签
            return Err(anyhow!("<IMAGE> tag not found"));
        };

        // 将更新后的XML信息设置回映像
        self.wimgapi
            .set_image_info(hImage, &updated_image_info)
            .with_context(|| "Set image info error")?;

        self.wimgapi
            .close(hImage)
            .with_context(|| "Close image error")?;
        self.wimgapi
            .close(handle)
            .with_context(|| "Close handle error")?;

        // 卸载镜像文件
        main_pb.inc(1);
        main_pb.set_message(t!("create_patch.unmount_base"));
        // 非TTY环境下输出子进度条消息
        if !*IS_TTY {
            println!("{}", t!("create_patch.unmount_base"));
        }
        self.wimgapi
            .unmount_image_handle(base_image_handle)
            .with_context(|| "Unmount src image error")?;
        self.wimgapi
            .close(base_image_handle)
            .with_context(|| "Close base image handle error")?;
        self.wimgapi
            .close(base_handle)
            .with_context(|| "Close base handle error")?;

        self.wimgapi
            .unmount_image_handle(update_image_handle)
            .with_context(|| "Unmount update image error")?;

        self.wimgapi
            .close(update_image_handle)
            .with_context(|| "Close update image handle error")?;
        self.wimgapi
            .close(update_handle)
            .with_context(|| "Close update handle error")?;

        main_pb.finish_and_clear();

        Ok(())
    }

    /// 应用补丁包
    pub fn apply_patch(
        &self,
        base_image: &Path,
        base_index: u32,
        patch_image: &Path,
        target_image: &Path,
        exclude: Option<&[String]>,
        force: bool,
    ) -> Result<()> {
        // 创建进度条管理器
        let multi_pb = MultiProgress::new();

        // 创建主进度条
        let main_pb = multi_pb.add(ProgressBar::new(6));
        main_pb.set_style(
            ProgressStyle::with_template(
                "{prefix:.bold.dim} [{elapsed_precise}] [{bar}] {pos}/{len}: {msg}",
            )
            .unwrap()
            .progress_chars("=> "),
        );
        main_pb.enable_steady_tick(Duration::from_millis(80));

        main_pb.set_message(t!("apply_patch.checking_patch"));
        if !*IS_TTY {
            write_console(ConsoleType::Info, &t!("apply_patch.checking_patch"));
        }

        // 打开补丁包
        let patch_handle = self
            .wimgapi
            .open(
                patch_image,
                WIM_GENERIC_READ | WIM_GENERIC_MOUNT,
                WIM_OPEN_EXISTING,
                WIM_COMPRESS_NONE,
            )
            .with_context(|| "Open patch image error")?;
        self.wimgapi
            .set_temp_path(patch_handle, &TEMP_PATH)
            .with_context(|| "Set temp path error")?;

        // 读取补丁包中的补丁信息
        let mut manifest: Vec<PatchManifest> = Vec::new();
        for index in 1..self.wimgapi.get_image_count(patch_handle) + 1 {
            let patch_image_handle = self
                .wimgapi
                .load_image(patch_handle, index)
                .with_context(|| "Load image error")?;

            // 读取补丁包卷信息
            let patch_image_info = self
                .wimgapi
                .get_image_info(patch_image_handle)
                .with_context(|| "Get image info error")?;
            let patch_manifest = self.parse_patch_info(&patch_image_info)?;
            self.wimgapi.close(patch_image_handle)?;
            manifest.push(patch_manifest);
        }

        // 打开基础镜像
        let base_handle = self
            .wimgapi
            .open(
                base_image,
                WIM_GENERIC_READ,
                WIM_OPEN_EXISTING,
                WIM_COMPRESS_NONE,
            )
            .with_context(|| "Open base image error")?;

        self.wimgapi
            .set_temp_path(base_handle, &TEMP_PATH)
            .with_context(|| "Set temp path error")?;
        let base_image_handle = self
            .wimgapi
            .load_image(base_handle, base_index)
            .with_context(|| "Load base image error")?;

        // 读取基础镜像卷信息
        let mut base_image_volumes = self
            .wimgapi
            .get_image_info(base_image_handle)
            .with_context(|| "Get image info error")?;
        let mut base_image_info = ImageInfo::from_str(&base_image_volumes)
            .with_context(|| "Parse base image info error")?;

        // 关闭基础镜像句柄（后续复制基础镜像后需要重新打开）
        self.wimgapi.close(base_image_handle)?;
        self.wimgapi.close(base_handle)?;

        // 校验基础镜像
        if !force {
            // 判断基础镜像是否与补丁包内信息是否一致
            if !manifest
                .iter()
                .any(|m| m.base_image_info == base_image_info)
            {
                self.wimgapi.close(patch_handle)?;

                // 判断是否已应用补丁
                if manifest
                    .iter()
                    .any(|m| m.target_image_info == base_image_info)
                {
                    return Err(anyhow!(t!("apply_patch.already_applied")));
                }
                return Err(anyhow!(t!("apply_patch.not_match")));
            }
        }

        // 挂载基础镜像
        main_pb.inc(1);
        main_pb.set_message(t!("create_patch.mount_base"));
        if !*IS_TTY {
            write_console(ConsoleType::Info, &t!("create_patch.mount_base"));
        }

        // 复制源镜像到临时目录
        fs::copy(base_image, TEMP_PATH.join(base_image.file_name().unwrap()))
            .with_context(|| "Copy base image error")?;
        let base_image = TEMP_PATH.join(base_image.file_name().unwrap());

        // 打开复制后的基础镜像
        let base_handle = self.wimgapi.open(
            &base_image,
            WIM_GENERIC_READ | WIM_GENERIC_WRITE | WIM_GENERIC_MOUNT,
            WIM_OPEN_EXISTING,
            WIM_COMPRESS_NONE,
        )?;

        self.wimgapi
            .set_temp_path(base_handle, &TEMP_PATH)
            .with_context(|| "Set temp path error")?;
        let base_image_handle = self
            .wimgapi
            .load_image(base_handle, base_index)
            .with_context(|| "Load base image error")?;

        let base_mount = TEMP_PATH.join("base");
        fs::create_dir_all(&base_mount)?;
        if let Err(e) = self
            .wimgapi
            .mount_image_handle(base_image_handle, &base_mount, 0)
        {
            self.wimgapi.close(base_image_handle)?;
            self.wimgapi.close(base_handle)?;
            self.wimgapi.close(patch_handle)?;
            return Err(anyhow!("Mount base image error: {:?}", e));
        }

        main_pb.inc(1);
        main_pb.set_message(t!("apply_patch.apply_patch"));
        if !*IS_TTY {
            write_console(ConsoleType::Info, &t!("apply_patch.apply_patch"));
        }

        for index in 1..manifest.len() as u32 + 1 {
            if let Some(manifest) = &manifest
                .iter()
                .find(|m| m.base_image_info == base_image_info)
            {
                let patch_image_handle = self
                    .wimgapi
                    .load_image(patch_handle, index)
                    .with_context(|| "Load image error")?;

                // 创建补丁包挂载目录
                let patch_mount = TEMP_PATH.join("patch");
                if patch_mount.exists() {
                    fs::remove_dir_all(&patch_mount).with_context(|| "Remove patch mount error")?;
                }
                fs::create_dir_all(&patch_mount).with_context(|| "Create patch mount error")?;

                // 挂载补丁包
                if let Err(e) = self.wimgapi.mount_image_handle(
                    patch_image_handle,
                    &patch_mount,
                    WIM_FLAG_MOUNT_READONLY,
                ) {
                    self.wimgapi.close(patch_image_handle)?;
                    self.wimgapi.close(patch_handle)?;
                    return Err(anyhow!("Mount patch image error: {:?}", e));
                }

                // 创建子进度条（用于显示具体操作进度）
                let sub_pb = multi_pb.add(ProgressBar::new(100));
                sub_pb.set_style(
                    ProgressStyle::with_template("{prefix:.bold.dim} {spinner} {wide_msg}")
                        .unwrap()
                        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
                );
                sub_pb.enable_steady_tick(Duration::from_millis(80));

                // 应用文件操作
                if let Err(e) = self.apply_operations(
                    &base_mount,
                    &patch_mount,
                    &manifest.operations,
                    exclude,
                    &sub_pb,
                ) {
                    self.wimgapi.unmount_image_handle(base_image_handle)?;
                    self.wimgapi.close(base_image_handle)?;
                    self.wimgapi.close(base_handle)?;
                    self.wimgapi.unmount_image_handle(patch_image_handle)?;
                    self.wimgapi.close(patch_image_handle)?;
                    self.wimgapi.close(patch_handle)?;

                    return Err(anyhow!("Apply operations error: {:?}", e));
                }

                // 提交更改
                if let Err(e) = self.wimgapi.commit(base_image_handle, 0) {
                    self.wimgapi.unmount_image_handle(base_image_handle)?;
                    self.wimgapi.close(base_image_handle)?;
                    self.wimgapi.close(base_handle)?;
                    self.wimgapi.unmount_image_handle(patch_image_handle)?;
                    self.wimgapi.close(patch_image_handle)?;
                    self.wimgapi.close(patch_handle)?;

                    return Err(anyhow!("Commit image error: {:?}", e));
                }

                // 更新镜像信息（需在提交更改后）
                base_image_volumes = self
                    .wimgapi
                    .get_image_info(base_image_handle)
                    .with_context(|| "Get image info error")?;
                if let Some(name) = &manifest.target_image_info.name {
                    base_image_volumes = replace_xml_field(&base_image_volumes, "NAME", name);
                }
                if let Some(display_name) = &&manifest.target_image_info.display_name {
                    base_image_volumes =
                        replace_xml_field(&base_image_volumes, "DISPLAYNAME", display_name);
                }
                if let Some(flags) = &&manifest.target_image_info.flags {
                    base_image_volumes = replace_xml_field(&base_image_volumes, "FLAGS", flags);
                }
                if let Some(description) = &&manifest.target_image_info.description {
                    base_image_volumes =
                        replace_xml_field(&base_image_volumes, "DESCRIPTION", description);
                }
                if let Some(display_description) = &&manifest.target_image_info.display_description
                {
                    base_image_volumes = replace_xml_field(
                        &base_image_volumes,
                        "DISPLAYDESCRIPTION",
                        display_description,
                    );
                }

                base_image_info = manifest.target_image_info.clone();

                // 卸载补丁包
                self.wimgapi
                    .unmount_image_handle(patch_image_handle)
                    .with_context(|| "Unmount patch image error")?;
                self.wimgapi
                    .close(patch_image_handle)
                    .with_context(|| "Close patch image handle error")?;
            }
        }

        self.wimgapi
            .set_image_info(base_image_handle, &base_image_volumes)
            .with_context(|| "Set image info error")?;

        // 导出更新镜像
        main_pb.inc(1);
        main_pb.set_message(t!("apply_patch.export_updated"));
        if !*IS_TTY {
            write_console(ConsoleType::Info, &t!("apply_patch.export_updated"));
        }

        let target_handle = self.wimgapi.open(
            target_image,
            WIM_GENERIC_WRITE,
            WIM_CREATE_ALWAYS,
            WIM_COMPRESS_LZX,
        )?;
        self.wimgapi.set_temp_path(target_handle, &TEMP_PATH)?;

        let image_count = self.wimgapi.get_image_count(base_handle);
        for index in 1..=image_count {
            if index == base_index {
                // 导出更新镜像
                if let Err(e) = self
                    .wimgapi
                    .export_image(base_image_handle, target_handle, 0)
                {
                    return Err(anyhow!("Export image error: {:?}", e));
                }
            } else {
                // 导出原始镜像
                let image_handle = self.wimgapi.load_image(base_handle, index)?;
                if let Err(e) = self.wimgapi.export_image(image_handle, target_handle, 0) {
                    return Err(anyhow!("Export image error: {:?}", e));
                }
                self.wimgapi.close(image_handle)?;
            }
        }

        self.wimgapi
            .close(target_handle)
            .with_context(|| "Close target handle error")?;

        // 卸载基础镜像
        main_pb.inc(1);
        main_pb.set_message(t!("create_patch.unmount_base"));
        if !*IS_TTY {
            write_console(ConsoleType::Info, &t!("create_patch.unmount_base"));
        }
        self.wimgapi
            .unmount_image_handle(base_image_handle)
            .with_context(|| "Unmount base image error")?;
        self.wimgapi
            .close(base_image_handle)
            .with_context(|| "Close base image handle error")?;
        self.wimgapi
            .close(base_handle)
            .with_context(|| "Close base handle error")?;

        // 卸载补丁包
        main_pb.inc(1);
        main_pb.set_message(t!("apply_patch.unmount_patch"));
        if !*IS_TTY {
            write_console(ConsoleType::Info, &t!("apply_patch.unmount_patch"));
        }

        self.wimgapi
            .close(patch_handle)
            .with_context(|| "Close patch handle error")?;

        main_pb.finish_and_clear();
        Ok(())
    }

    /// 根据操作配置对基础镜像执行文件操作
    fn apply_operations(
        &self,
        base_mount: &Path,
        patch_mount: &Path,
        operations: &Operations,
        exclude: Option<&[String]>,
        multi_pb: &ProgressBar,
    ) -> Result<()> {
        // 应用删除操作
        for delete in &operations.deletes {
            // 判断是否需要排除
            if let Some(exclude) = exclude
                && exclude.iter().any(|exclude_item| {
                    delete
                        .path
                        .to_ascii_lowercase()
                        .contains(&exclude_item.to_ascii_lowercase())
                })
            {
                multi_pb.set_message(format!("{} \\{}", t!("create_patch.exclude"), &delete.path));
                if !*IS_TTY {
                    write_console(
                        ConsoleType::Info,
                        &format!("{} \\{}", t!("create_patch.exclude"), &delete.path),
                    );
                }
                continue;
            }

            let target_path = base_mount.join(&delete.path);
            multi_pb.set_message(format!("{} \\{}", t!("create_patch.Delete"), &delete.path));
            if !*IS_TTY {
                write_console(
                    ConsoleType::Info,
                    &format!("{} \\{}", t!("create_patch.Delete"), &delete.path),
                );
            }
            if target_path.exists() {
                if target_path.is_dir() {
                    fs::remove_dir_all(&target_path)
                        .with_context(|| format!("Delete directory Failed: {}", &delete.path))?;
                } else {
                    fs::remove_file(&target_path)
                        .with_context(|| format!("Delete file Failed: {}", &delete.path))?;
                }
            }
        }

        // 应用新增操作
        for add in &operations.adds {
            // 判断是否需要排除
            if let Some(exclude) = exclude
                && exclude.iter().any(|exclude_item| {
                    add.path
                        .to_ascii_lowercase()
                        .contains(&exclude_item.to_ascii_lowercase())
                })
            {
                multi_pb.set_message(format!("{} \\{}", t!("create_patch.exclude"), &add.path));
                if !*IS_TTY {
                    write_console(
                        ConsoleType::Info,
                        &format!("{} \\{}", t!("create_patch.exclude"), &add.path),
                    );
                }
                continue;
            }

            let source_path = patch_mount.join(&add.path);
            let target_path = base_mount.join(&add.path);

            if source_path.is_dir() {
                // 新建目录
                fs::create_dir_all(&target_path)?;
                continue;
            }

            multi_pb.set_message(format!("{} \\{}", t!("create_patch.Add"), &add.path));
            if !*IS_TTY {
                write_console(
                    ConsoleType::Info,
                    &format!("{} \\{}", t!("create_patch.Add"), &add.path),
                );
            }
            // 确保目标目录存在
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Create target directory Failed: {}", parent.display())
                })?;
            }

            // 复制文件
            if source_path.exists() {
                fs::copy(&source_path, &target_path).with_context(|| {
                    format!(
                        "Copy file Failed: {} -> {}",
                        source_path.display(),
                        target_path.display()
                    )
                })?;
            } else {
                write_console(
                    ConsoleType::Warning,
                    &format!("Patch file source file not exist: {}", &add.path),
                );
            }
        }

        // 应用修改操作
        for modify in &operations.modifies {
            // 判断是否需要排除
            if let Some(exclude) = exclude
                && exclude.iter().any(|exclude_item| {
                    modify
                        .path
                        .to_ascii_lowercase()
                        .contains(&exclude_item.to_ascii_lowercase())
                })
            {
                multi_pb.set_message(format!("{} \\{}", t!("create_patch.exclude"), &modify.path));
                if !*IS_TTY {
                    write_console(
                        ConsoleType::Info,
                        &format!("{} \\{}", t!("create_patch.exclude"), &modify.path),
                    );
                }
                continue;
            }

            let source_path = patch_mount.join(&modify.path);
            let target_path = base_mount.join(&modify.path);

            multi_pb.set_message(format!("{} \\{}", t!("create_patch.Modify"), &modify.path));
            if !*IS_TTY {
                write_console(
                    ConsoleType::Info,
                    &format!("{} \\{}", t!("create_patch.Modify"), &modify.path),
                );
            }
            match modify.storage.to_lowercase().as_str() {
                "full" => {
                    // 复制文件
                    if source_path.exists() {
                        fs::copy(&source_path, &target_path).with_context(|| {
                            format!(
                                "Copy file Failed: {} -> {}",
                                source_path.display(),
                                target_path.display()
                            )
                        })?;
                    } else {
                        write_console(
                            ConsoleType::Warning,
                            &format!("Patch file source file not exist: {}", &modify.path),
                        );
                    }
                }
                "zstd" => {
                    // 应用zstdiff差异文件
                    let patch_path = patch_mount.join(format!("{}.diff ", &modify.path));
                    if patch_path.exists() {
                        if let Err(e) =
                            ZstdDiff::file_patch(&target_path, &patch_path, &target_path)
                        {
                            write_console(
                                ConsoleType::Error,
                                &format!("Apply zstdiff patch file Failed: {:?}", e),
                            );
                        }
                    } else {
                        write_console(
                            ConsoleType::Warning,
                            &format!("Patch file bsdiff patch file not exist: {}", &modify.path),
                        );
                    }
                }
                "bsdiff" => {
                    // 应用bsdiff差异文件
                    let patch_path = patch_mount.join(format!("{}.diff ", &modify.path));
                    if patch_path.exists() {
                        if let Err(e) = BsDiff::file_patch(&target_path, &patch_path, &target_path)
                        {
                            write_console(
                                ConsoleType::Error,
                                &format!("Apply bsdiff patch file Failed: {:?}", e),
                            );
                        }
                    } else {
                        write_console(
                            ConsoleType::Warning,
                            &format!("Patch file bsdiff patch file not exist: {}", &modify.path),
                        );
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// 合并多个补丁包
    ///
    /// # 参数
    ///
    /// * `patches` - 补丁包文件路径列表
    /// * `out` - 输出合并后的补丁包文件路径
    ///
    /// # 返回值
    ///
    /// * `Ok(())` - 如果合并成功
    /// * `Err` - 如果发生错误
    pub fn merge_patches(&self, patches: &[PathBuf], out: &Path) -> Result<()> {
        let h_out_patch = self
            .wimgapi
            .open(out, WIM_GENERIC_WRITE, WIM_CREATE_ALWAYS, WIM_COMPRESS_NONE)
            .with_context(|| "Open out patch error ")?;

        self.wimgapi
            .set_temp_path(h_out_patch, &TEMP_PATH)
            .with_context(|| "Set temp path error ")?;

        for patch_path in patches {
            write_console(
                ConsoleType::Info,
                &format!(
                    "{}: {}",
                    t!("merge_patch.merge_patch"),
                    patch_path.display()
                ),
            );
            let h_patch = self
                .wimgapi
                .open(
                    patch_path,
                    WIM_GENERIC_READ,
                    WIM_OPEN_EXISTING,
                    WIM_COMPRESS_NONE,
                )
                .with_context(|| "Open patch error ")?;

            self.wimgapi
                .set_temp_path(h_patch, &TEMP_PATH)
                .with_context(|| "Set temp path error ")?;
            let h_patch_image = self
                .wimgapi
                .load_image(h_patch, 1)
                .with_context(|| "Load patch image error ")?;

            self.wimgapi
                .export_image(h_patch_image, h_out_patch, 0)
                .with_context(|| "Export patch image error ")?;

            self.wimgapi
                .close(h_patch_image)
                .with_context(|| "Close patch image handle error ")?;
            self.wimgapi
                .close(h_patch)
                .with_context(|| "Close patch handle error ")?;
        }

        self.wimgapi
            .close(h_out_patch)
            .with_context(|| "Close out patch error ")?;
        Ok(())
    }

    pub fn cleanup(&self) -> Result<()> {
        let mounted_images = self.wimgapi.get_mounted_image()?;
        let mounted_images: Vec<WimMountInfoLevel1> = mounted_images
            .into_iter()
            .filter(|mount_info| mount_info.mount_flags == WIM_MOUNT_FLAG_INVALID)
            .collect();
        if mounted_images.is_empty() {
            write_console(ConsoleType::Info, &t!("cleanup.not_invalid_mount"));
            return Ok(());
        }

        for mount_info in mounted_images {
            match self.wimgapi.unmount_image(
                Path::new(&mount_info.mount_path),
                mount_info.wim_path.as_ref(),
                mount_info.image_index,
                false,
            ) {
                Ok(_) => {
                    write_console(
                        ConsoleType::Info,
                        &format!("{}: {}", t!("cleanup.unmount"), mount_info.mount_path),
                    );
                }
                Err(_e) => write_console(
                    ConsoleType::Info,
                    &format!("{}: {}", t!("cleanup.unmount"), mount_info.mount_path),
                ),
            }
        }

        Ok(())
    }

    /// 卸载指定的wim镜像
    ///
    /// # 参数
    ///
    /// * `image` - wim镜像路径
    ///
    /// # 返回值
    ///
    /// - `Ok(())` - 成功卸载
    /// - `Err(anyhow::Error)` - 失败，返回错误信息
    fn unmount_image(&self, image: &Path) -> Result<()> {
        let mounted_images = self.wimgapi.get_mounted_image()?;
        let mount = mounted_images
            .iter()
            .find(|i| i.wim_path == image.display().to_string());
        if let Some(mount) = mount {
            self.wimgapi
                .unmount_image(
                    Path::new(&mount.mount_path),
                    image,
                    mount.image_index,
                    false,
                )
                .with_context(|| "Unmount image error".to_string())?;
        }
        Ok(())
    }
}
