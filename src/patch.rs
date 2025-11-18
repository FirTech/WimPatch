use crate::bsdiff::BsDiff;
use crate::cli::{Compress, Preset, Storage};
use crate::console::{ConsoleType, write_console};
use crate::manifest::{Action, ImageInfo, Operation, PatchManifest};
use crate::utils::{DiffType, compare_directories, format_bytes, get_tmp_name, replace_xml_field};
use crate::wimgapi::{
    WIM_COMPRESS_LZX, WIM_COMPRESS_NONE, WIM_COMPRESS_XPRESS, WIM_CREATE_ALWAYS, WIM_FLAG_MOUNT_READONLY,
    WIM_GENERIC_MOUNT, WIM_GENERIC_READ, WIM_GENERIC_WRITE, WIM_MOUNT_FLAG_INVALID, WIM_MOUNT_FLAG_NO_MOUNTDIR,
    WIM_MOUNT_FLAG_NO_WIM, WIM_MSG_PROCESS, WIM_MSG_PROGRESS, WIM_OPEN_ALWAYS, WIM_OPEN_EXISTING, WimMountInfoLevel1,
    Wimgapi,
};
use crate::zstdiff::ZstdDiff;
use crate::{get_temp_path, is_tty};
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Local};
use console::style;
use indicatif::MultiProgress;
use indicatif::{ProgressBar, ProgressStyle};
use rust_i18n::t;
use semver::Version;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::string::String;
use std::time::Duration;
use std::{fs, ptr};

pub struct WimPatch {
    multi_pb: MultiProgress,
    wimgapi: Wimgapi,
}

impl WimPatch {
    /// 初始化 WimPatch 实例
    pub fn new() -> Result<Self> {
        // 进度条管理器
        let multi_pb = MultiProgress::new();

        // 加载 wimgapi
        let wimgapi = Wimgapi::new(None).with_context(|| "Failed to load wimgapi.dll".to_string())?;

        // 创建临时目录
        if !get_temp_path().exists() {
            fs::create_dir_all(get_temp_path()).with_context(|| t!("create_temp_dir.failed"))?;
        }

        Ok(Self { wimgapi, multi_pb })
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
    fn parse_patch_info(&self, image_info: &str) -> Result<PatchManifest> {
        // 解析PatchManifest
        if let (Some(start), Some(end)) = (image_info.find("<PatchManifest>"), image_info.find("</PatchManifest>")) {
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
            .open(patch, WIM_GENERIC_READ, WIM_OPEN_EXISTING, WIM_COMPRESS_NONE)
            .with_context(|| format!("Open patch image {} failed", patch.display()))?;

        self.wimgapi
            .set_temp_path(patch_handle, get_temp_path())
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
            result.push_str(&format!("{:<label_w$} {{{}}}\n", "Id:", manifest.id));
            result.push_str(&format!(
                "{:<label_w$} {}\n",
                "Size:",
                format_bytes(patch.metadata().unwrap().len())
            ));
            result.push_str(&format!("{:<label_w$} {}\n", "Version:", manifest.patch_version));
            result.push_str(&format!("{:<label_w$} {}\n", "Name:", manifest.name));
            result.push_str(&format!("{:<label_w$} {}\n", "Author:", manifest.author));
            result.push_str(&format!("{:<label_w$} {}\n", "Description:", manifest.description));
            result.push_str(&format!("{:<label_w$} {}\n", "Tool Version:", manifest.tool_version));
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
                result.push_str(&format!("{:<label_w$} {}\n", "created:", manifest.timestamp));
            }

            // 显示操作统计
            let add_count = manifest.operations.iter().filter(|op| op.action == Action::Add).count();
            let modify_count = manifest
                .operations
                .iter()
                .filter(|op| op.action == Action::Modify)
                .count();
            let delete_count = manifest
                .operations
                .iter()
                .filter(|op| op.action == Action::Delete)
                .count();

            let total = add_count + modify_count + delete_count;
            result.push_str(&format!(
                "{:<label_w$} +{} / ~{} / -{} (total: {})\n",
                "Operations:", add_count, modify_count, delete_count, total
            ));

            // 显示基础镜像信息
            result.push_str("\nBase Image Information:\n");
            result.push_str(&format!("{:-^total_w$}\n", "-"));
            result.push_str(&format!("{:<label_w$} {{{}}}\n", "Guid:", manifest.base_image_guid));
            result.push_str(&format!("{:<label_w$} {}\n", "Index:", manifest.base_image_info.index));

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
                format_bytes(manifest.base_image_info.total_bytes)
            ));

            // 显示更新镜像信息
            result.push_str("\nTarget Image Information:\n");
            result.push_str(&format!("{:-^total_w$}\n", "-"));
            result.push_str(&format!("{:<label_w$} {{{}}}\n", "Guid:", manifest.target_image_guid));
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
                format_bytes(manifest.target_image_info.total_bytes)
            ));

            result.push('\n');
        }
        self.wimgapi
            .close(patch_handle)
            .with_context(|| "Close patch failed".to_string())?;
        Ok(result)
    }

    /// 创建补丁
    ///
    /// # 参数
    ///
    /// - `base_image` - 基础镜像路径
    /// - `index_base` - 基础镜像索引
    /// - `updated_image` - 更新镜像路径
    /// - `index_updated` - 更新镜像索引
    /// - `patch_image` - 补丁镜像路径
    /// - `storage` - 存储配置
    /// - `preset` - 预设配置
    /// - `version` - 补丁版本
    /// - `author` - 作者
    /// - `name` - 名称
    /// - `description` - 描述
    /// - `exclude` - 排除路径列表
    /// - `compress` - 压缩算法
    ///
    /// # 返回值
    ///
    /// - `Ok(())` - 成功
    /// - `Err(anyhow::Error)` - 失败
    pub fn create_patch(
        &self,
        base_image: &Path,
        base_index: Option<u32>,
        target_image: &Path,
        target_index: Option<u32>,
        patch_image: &Path,
        storage: &Storage,
        preset: &Preset,
        version: &str,
        author: &str,
        name: &str,
        description: &str,
        exclude: Option<&[String]>,
        compress: &Compress,
    ) -> Result<()> {
        // 获取基础镜像文件卷数
        let base_handle = self
            .wimgapi
            .open(
                base_image,
                WIM_GENERIC_READ | WIM_GENERIC_MOUNT,
                WIM_OPEN_EXISTING,
                WIM_COMPRESS_NONE,
            )
            .with_context(|| "Open base image failed".to_string())?;
        let base_image_count = self.wimgapi.get_image_count(base_handle);
        self.wimgapi
            .close(base_handle)
            .with_context(|| "Close base handle error")?;

        // 获取更新镜像文件卷数
        let target_handle = self
            .wimgapi
            .open(
                target_image,
                WIM_GENERIC_READ | WIM_GENERIC_MOUNT,
                WIM_OPEN_EXISTING,
                WIM_COMPRESS_NONE,
            )
            .with_context(|| "Open update image failed".to_string())?;
        let target_image_count = self.wimgapi.get_image_count(target_handle);
        self.wimgapi
            .close(target_handle)
            .with_context(|| "Close update handle error")?;

        // 选择要处理的镜像索引
        if let Some(base_index) = base_index
            && let Some(target_index) = target_index
        {
            if base_index > base_image_count || target_index > target_image_count {
                return Err(anyhow!("Index {} is out of range", base_index));
            }
            write_console(
                ConsoleType::Info,
                &format!(
                    "{}: {}({}{}) -> {}({}{})",
                    t!("create_patch.create_patch"),
                    t!("create_patch.base"),
                    t!("create_patch.index"),
                    base_index,
                    t!("create_patch.target"),
                    t!("create_patch.index"),
                    target_index
                ),
            );

            self.build_patch_image(
                base_image,
                base_index,
                target_image,
                target_index,
                patch_image,
                storage,
                preset,
                version,
                author,
                name,
                description,
                exclude,
                *compress,
            )?;
        } else {
            // 用户未指定索引，遍历所有基础镜像和更新镜像的组合(1-1、2-2、3-3等)
            for index in 1..=base_image_count.min(target_image_count) {
                write_console(
                    ConsoleType::Info,
                    &format!(
                        "{}: {}({}{}) -> {}({}{})",
                        t!("create_patch.create_patch"),
                        t!("create_patch.base"),
                        t!("create_patch.index"),
                        index,
                        t!("create_patch.target"),
                        t!("create_patch.index"),
                        index
                    ),
                );
                self.build_patch_image(
                    base_image,
                    index,
                    target_image,
                    index,
                    patch_image,
                    storage,
                    preset,
                    version,
                    author,
                    name,
                    description,
                    exclude,
                    *compress,
                )?;
            }
        }

        self.multi_pb
            .clear()
            .with_context(|| "Clear multi pb failed".to_string())?;
        Ok(())
    }

    /// 构建补丁镜像
    ///
    /// # 参数
    ///
    /// - `base_image` - 基础镜像路径
    /// - `base_index` - 基础镜像索引
    /// - `updated_image` - 更新镜像路径
    /// - `updated_index` - 更新镜像索引
    /// - `patch_image` - 输出补丁镜像路径
    /// - `storage` - 存储配置
    /// - `preset` - 预设配置
    /// - `version` - 补丁版本
    /// - `author` - 作者
    /// - `name` - 名称
    /// - `description` - 描述
    /// - `exclude` - 排除路径列表
    /// - `compress` - 压缩算法
    ///
    /// # 返回值
    ///
    /// - `Ok(())` - 成功
    /// - `Err(anyhow::Error)` - 失败
    fn build_patch_image(
        &self,
        base_image: &Path,
        base_index: u32,
        target_image: &Path,
        target_index: u32,
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
        // 创建主进度条
        let main_pb = self.multi_pb.add(ProgressBar::new(6));
        main_pb.set_style(
            ProgressStyle::with_template("{prefix:.bold.dim} [{elapsed_precise}] [{bar}] {pos}/{len}: {msg}")
                .unwrap()
                .progress_chars("=> "),
        );
        main_pb.enable_steady_tick(Duration::from_millis(80));

        main_pb.set_message(t!("create_patch.read_image_info"));
        if !is_tty() {
            println!("{}", t!("create_patch.read_image_info"));
        }

        // 打开基础镜像文件
        let base_handle = self.wimgapi.open(
            base_image,
            WIM_GENERIC_READ | WIM_GENERIC_MOUNT,
            WIM_OPEN_EXISTING,
            WIM_COMPRESS_NONE,
        )?;
        self.wimgapi
            .set_temp_path(base_handle, get_temp_path())
            .with_context(|| "Set temp path failed".to_string())?;
        let base_image_handle = self
            .wimgapi
            .load_image(base_handle, base_index)
            .with_context(|| "Load base image failed".to_string())?;

        // 读取基础镜像卷信息
        let base_image_manifest = self
            .wimgapi
            .get_image_info(base_image_handle)
            .with_context(|| "Get base image info failed".to_string())?;
        let base_image_attributes = self
            .wimgapi
            .get_attributes(base_handle)
            .with_context(|| "Get base image attributes failed".to_string())?;
        let base_image_info =
            ImageInfo::from_xml(&base_image_manifest).with_context(|| "Parse base image info failed".to_string())?;

        // 打开更新镜像文件
        let target_handle = self.wimgapi.open(
            target_image,
            WIM_GENERIC_READ | WIM_GENERIC_MOUNT,
            WIM_OPEN_EXISTING,
            WIM_COMPRESS_NONE,
        )?;
        self.wimgapi
            .set_temp_path(target_handle, get_temp_path())
            .with_context(|| "Set temp path failed".to_string())?;
        let target_image_handle = self
            .wimgapi
            .load_image(target_handle, target_index)
            .with_context(|| "Load target image failed".to_string())?;

        // 读取更新镜像卷信息
        let target_image_manifest = self
            .wimgapi
            .get_image_info(target_image_handle)
            .with_context(|| "Get target image info failed".to_string())?;
        let target_image_attributes = self
            .wimgapi
            .get_attributes(target_handle)
            .with_context(|| "Get target image attributes failed".to_string())?;
        let target_image_info = ImageInfo::from_xml(&target_image_manifest)
            .with_context(|| "Parse target image info failed".to_string())?;
        main_pb.inc(1);

        // 挂载基础镜像文件
        main_pb.set_message(t!("create_patch.mount_base"));
        if !is_tty() {
            println!("{}", t!("create_patch.mount_base"));
        }

        let base_mount = get_temp_path().join(get_tmp_name("base-", "", 6));
        if base_mount.exists() {
            fs::remove_dir_all(&base_mount).with_context(|| "Remove base mount dir failed".to_string())?;
        }
        fs::create_dir_all(&base_mount).with_context(|| "Create base mount dir failed".to_string())?;
        if let Err(e) = self
            .wimgapi
            .mount_image_handle(base_image_handle, &base_mount, WIM_FLAG_MOUNT_READONLY)
        {
            self.wimgapi.close(base_image_handle).ok();
            self.wimgapi.close(base_handle).ok();
            return Err(anyhow!("{}: {}", t!("create_patch.mount_base_failed"), e));
        }
        main_pb.inc(1);

        // 挂载更新镜像文件
        main_pb.set_message(t!("create_patch.mount_target"));
        if !is_tty() {
            println!("{}", t!("create_patch.mount_target"));
        }
        let target_mount = get_temp_path().join(get_tmp_name("target-", "", 6));
        if target_mount.exists() {
            fs::remove_dir_all(&target_mount).with_context(|| "Remove target mount dir failed".to_string())?;
        }
        fs::create_dir_all(&target_mount).with_context(|| "Create target mount dir failed".to_string())?;
        if let Err(e) = self
            .wimgapi
            .mount_image_handle(target_image_handle, &target_mount, WIM_FLAG_MOUNT_READONLY)
        {
            self.wimgapi.unmount_image_handle(base_image_handle).ok();
            self.wimgapi.close(base_image_handle).ok();
            self.wimgapi.close(base_handle).ok();
            self.wimgapi.close(target_image_handle).ok();
            self.wimgapi.close(target_handle).ok();
            return Err(anyhow!("{}: {}", t!("create_patch.mount_target_failed"), e));
        }
        main_pb.inc(1);

        // 比较文件差异
        main_pb.set_message(t!("create_patch.compare_diff"));
        if !is_tty() {
            println!("{}", t!("create_patch.compare_diff"));
        }

        let patch_dir = get_temp_path().join(get_tmp_name("patch-", "", 6));
        if patch_dir.exists() {
            fs::remove_dir_all(&patch_dir).with_context(|| "Remove patch dir failed".to_string())?;
        }
        fs::create_dir_all(&patch_dir).with_context(|| "Create patch dir failed".to_string())?;
        let operations = match self.create_operations(&base_mount, &target_mount, &patch_dir, storage, preset, exclude)
        {
            Ok(operations) => operations,
            Err(e) => {
                self.wimgapi.unmount_image_handle(base_image_handle).ok();
                self.wimgapi.close(base_image_handle).ok();
                self.wimgapi.close(base_handle).ok();
                self.wimgapi.unmount_image_handle(target_image_handle).ok();
                self.wimgapi.close(target_image_handle).ok();
                self.wimgapi.close(target_handle).ok();
                return Err(e);
            }
        };
        main_pb.inc(1);

        // 卸载基础镜像
        main_pb.set_message(t!("create_patch.unmount_base"));
        if !is_tty() {
            println!("{}", t!("create_patch.unmount_base"));
        }
        if let Err(e) = self.wimgapi.unmount_image_handle(base_image_handle) {
            self.wimgapi.close(base_image_handle).ok();
            self.wimgapi.close(base_handle).ok();
            self.wimgapi.unmount_image_handle(target_image_handle).ok();
            self.wimgapi.close(target_image_handle).ok();
            self.wimgapi.close(target_handle).ok();
            return Err(anyhow!("{}: {}", t!("create_patch.unmount_base_failed"), e));
        }
        self.wimgapi
            .close(base_image_handle)
            .with_context(|| "Close base image handle error")?;
        self.wimgapi
            .close(base_handle)
            .with_context(|| "Close base handle error")?;

        // 卸载更新镜像
        main_pb.set_message(t!("create_patch.unmount_target"));
        if !is_tty() {
            println!("{}", t!("create_patch.unmount_target"));
        }
        if let Err(e) = self.wimgapi.unmount_image_handle(target_image_handle) {
            self.wimgapi.close(target_image_handle).ok();
            self.wimgapi.close(target_handle).ok();
            return Err(anyhow!("{}: {}", t!("create_patch.unmount_target_failed"), e));
        }
        self.wimgapi
            .close(target_image_handle)
            .with_context(|| "Close target image handle error")?;
        self.wimgapi
            .close(target_handle)
            .with_context(|| "Close target handle error")?;
        main_pb.inc(1);

        // 创建补丁镜像
        main_pb.set_message(t!("create_patch.create_patch"));
        if !is_tty() {
            println!("{}", t!("create_patch.create_patch"));
        }

        // 生成补丁清单
        let patch_manifest = PatchManifest::new(
            name,
            description,
            author,
            version,
            &format!("{:?}", base_image_attributes.guid),
            &base_image_info,
            &format!("{:?}", target_image_attributes.guid),
            &target_image_info,
            &operations,
        )
        .to_xml()
        .with_context(|| "Serialize patch manifest error")?;

        // 创建补丁文件
        let patch_handle = match self.wimgapi.open(
            patch_image,
            WIM_GENERIC_WRITE,
            WIM_OPEN_ALWAYS,
            match compress {
                Compress::None => WIM_COMPRESS_NONE,
                Compress::Xpress => WIM_COMPRESS_XPRESS,
                Compress::Lzx => WIM_COMPRESS_LZX,
            },
        ) {
            Ok(h) => h,
            Err(e) => {
                self.wimgapi.close(base_image_handle).ok();
                self.wimgapi.close(base_handle).ok();
                self.wimgapi.close(target_image_handle).ok();
                self.wimgapi.close(target_handle).ok();
                return Err(anyhow!("Create patch file error ({})", e));
            }
        };

        // 注册消息回调函数
        self.wimgapi
            .register_message_callback(patch_handle, CreatePatchCallback);

        // 捕获镜像
        let patch_image_handle = match self.wimgapi.capture(patch_handle, &patch_dir, 0) {
            Ok(handle) => handle,
            Err(e) => {
                self.wimgapi.close(patch_handle).ok();
                return Err(anyhow!("Capture patch image error ({})", e));
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
                            String::from_utf16_lossy(std::slice::from_raw_parts(path_ptr, len as usize))
                        };

                        // 过滤系统文件和目录
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

        // 注销消息回调函数
        self.wimgapi
            .unregister_message_callback(patch_handle, CreatePatchCallback);

        // 在</IMAGE>标签前添加基本字段信息
        let image_info = self
            .wimgapi
            .get_image_info(patch_image_handle)
            .with_context(|| "Get patch image info error")?;
        let updated_image_info = if let Some(pos) = image_info.rfind("</IMAGE>") {
            let prefix = &image_info[..pos];
            let suffix = &image_info[pos..];
            format!(
                "{}<NAME>{}</NAME>\
                <DESCRIPTION>{}</DESCRIPTION>\
                <DISPLAYNAME>{}</DISPLAYNAME>\
                <DISPLAYDESCRIPTION>{}</DISPLAYDESCRIPTION>\
                <FLAGS></FLAGS>{}{}",
                prefix, name, description, name, description, patch_manifest, suffix
            )
        } else {
            // 错误: 没找到</IMAGE>标签
            return Err(anyhow!("<IMAGE> tag not found"));
        };

        // 将更新后的XML信息设置回映像
        self.wimgapi
            .set_image_info(patch_image_handle, &updated_image_info)
            .with_context(|| "Set image info error")?;

        // 关闭补丁镜像句柄
        self.wimgapi
            .close(patch_image_handle)
            .with_context(|| "Close patch image handle error")?;
        self.wimgapi
            .close(patch_handle)
            .with_context(|| "Close patch handle error")?;

        main_pb.inc(1);
        main_pb.set_message(format!(
            "{} ({}{})",
            t!("create_patch.success"),
            t!("create_patch.index"),
            base_index
        ));

        main_pb.finish_and_clear();

        Ok(())
    }

    /// 应用补丁
    ///
    /// # 参数
    ///
    /// - `base_image` - 基础镜像路径
    /// - `base_index` - 基础镜像索引
    /// - `patch_image` - 补丁镜像路径
    /// - `target_image` - 目标镜像路径
    /// - `exclude` - 排除路径列表
    /// - `force` - 是否强制应用
    ///
    /// # 返回值
    ///
    /// - `Ok(())` - 成功
    /// - `Err(anyhow::Error)` - 失败
    pub fn apply_patch(
        &self,
        base_image: &Path,
        base_index: Option<u32>,
        patch_image: &Path,
        target_image: &Path,
        exclude: Option<&[String]>,
        force: bool,
    ) -> Result<()> {
        // 打开补丁包
        let patch_handle = self
            .wimgapi
            .open(patch_image, WIM_GENERIC_READ, WIM_OPEN_EXISTING, WIM_COMPRESS_NONE)
            .with_context(|| "Open patch image error")?;
        self.wimgapi
            .set_temp_path(patch_handle, get_temp_path())
            .with_context(|| "Set temp path error")?;

        // 读取补丁包中的补丁信息
        let mut patch_manifest_list: Vec<(u32, PatchManifest)> = Vec::new();
        for index in 1..self.wimgapi.get_image_count(patch_handle) + 1 {
            let patch_image_handle = self
                .wimgapi
                .load_image(patch_handle, index)
                .with_context(|| "Load image error")?;
            let patch_image_info = self
                .wimgapi
                .get_image_info(patch_image_handle)
                .with_context(|| "Get image info error")?;
            self.wimgapi.close(patch_image_handle)?;
            patch_manifest_list.push((
                index,
                self.parse_patch_info(&patch_image_info)
                    .with_context(|| "Parse patch info error")?,
            ));
        }
        self.wimgapi
            .close(patch_handle)
            .with_context(|| "Close patch handle error")?;

        // 打开基础镜像
        let base_handle = self
            .wimgapi
            .open(base_image, WIM_GENERIC_READ, WIM_OPEN_EXISTING, WIM_COMPRESS_NONE)
            .with_context(|| "Open base image error")?;
        self.wimgapi
            .set_temp_path(base_handle, get_temp_path())
            .with_context(|| "Set temp path error")?;

        // 读取基础镜像信息
        let base_attributes = self
            .wimgapi
            .get_attributes(base_handle)
            .with_context(|| "Get base image attributes error")?;
        let mut base_image_info_list: Vec<ImageInfo> = Vec::new();
        for index in 1..self.wimgapi.get_image_count(base_handle) + 1 {
            let base_image_handle = self
                .wimgapi
                .load_image(base_handle, index)
                .with_context(|| "Load image error")?;
            let image_info = self
                .wimgapi
                .get_image_info(base_image_handle)
                .with_context(|| "Get image info error")?;
            self.wimgapi.close(base_image_handle)?;
            base_image_info_list.push(ImageInfo::from_xml(&image_info).with_context(|| "Parse base image info error")?);
        }
        self.wimgapi
            .close(base_handle)
            .with_context(|| "Close base handle error")?;

        // 匹配补丁信息
        let match_info = self.match_patch(
            &format!("{:?}", base_attributes.guid),
            &base_image_info_list,
            &patch_manifest_list,
            force,
        )?;
        if match_info.is_empty() {
            return Err(anyhow!(t!("apply_patch.not_match")));
        }

        // 复制源镜像到临时目录
        fs::copy(base_image, get_temp_path().join(base_image.file_name().unwrap()))
            .with_context(|| "Copy base image error")?;
        let base_image = get_temp_path().join(base_image.file_name().unwrap());

        if let Some(base_index) = base_index {
            if !base_image_info_list
                .iter()
                .any(|base_info| base_info.index == base_index)
            {
                return Err(anyhow!(t!("apply_patch.base_index_not_found")));
            }
            for (base_image_info, match_patch) in match_info {
                if base_index == base_image_info.index {
                    write_console(
                        ConsoleType::Info,
                        &format!(
                            "{}: {}({}{})",
                            t!("apply_patch.apply_patch"),
                            t!("apply_patch.base"),
                            t!("apply_patch.index"),
                            base_image_info.index
                        ),
                    );
                    self.apply_patch_image(&base_image, base_index, patch_image, &match_patch, exclude, force)?;
                }
            }
        } else {
            // 自动匹配补丁
            for (base_image_info, match_patch) in match_info {
                write_console(
                    ConsoleType::Info,
                    &format!(
                        "{}: {}({}{})",
                        t!("apply_patch.apply_patch"),
                        t!("apply_patch.base"),
                        t!("apply_patch.index"),
                        base_image_info.index
                    ),
                );
                self.apply_patch_image(
                    &base_image,
                    base_image_info.index,
                    patch_image,
                    &match_patch,
                    exclude,
                    force,
                )?;
            }
        }

        // 打开基础镜像
        let base_handle = self
            .wimgapi
            .open(&base_image, WIM_GENERIC_READ, WIM_OPEN_EXISTING, WIM_COMPRESS_NONE)
            .with_context(|| "Open base image error")?;
        self.wimgapi
            .set_temp_path(base_handle, get_temp_path())
            .with_context(|| "Set temp path error")?;

        // 创建目标镜像（如果文件存在则覆盖）
        let target_handle = self
            .wimgapi
            .open(target_image, WIM_GENERIC_WRITE, WIM_CREATE_ALWAYS, WIM_COMPRESS_LZX)?;
        self.wimgapi
            .set_temp_path(target_handle, get_temp_path())
            .with_context(|| "Set temp path error")?;

        // 导出更新镜像
        for index in 1..=self.wimgapi.get_image_count(base_handle) {
            let base_image_handle = self
                .wimgapi
                .load_image(base_handle, index)
                .with_context(|| "Load image error")?;
            self.wimgapi
                .export_image(base_image_handle, target_handle, 0)
                .with_context(|| "Export image error")?;
            self.wimgapi
                .close(base_image_handle)
                .with_context(|| "Close image handle error")?;
        }
        self.wimgapi
            .close(base_handle)
            .with_context(|| "Close base handle error")?;
        self.wimgapi
            .close(target_handle)
            .with_context(|| "Close target handle error")?;

        self.multi_pb
            .clear()
            .with_context(|| "Clear multi pb failed".to_string())?;

        Ok(())
    }

    /// 应用补丁镜像
    ///
    /// # 参数
    ///
    /// - `base_image` - 基础镜像路径
    /// - `base_index` - 基础镜像索引
    /// - `patch_image` - 补丁镜像路径
    /// - `patch_manifest_list` - 补丁清单列表
    /// - `exclude` - 排除路径列表
    /// - `force` - 是否强制应用
    ///
    /// # 返回值
    ///
    /// - `Ok(())` - 成功
    /// - `Err(anyhow::Error)` - 失败
    fn apply_patch_image(
        &self,
        base_image: &Path,
        base_index: u32,
        patch_image: &Path,
        patch_manifest_list: &Vec<(u32, PatchManifest)>,
        exclude: Option<&[String]>,
        force: bool,
    ) -> Result<()> {
        // 计算总步骤数：基础镜像挂载 + 每个补丁镜像的4个步骤 + 基础镜像卸载
        let total_steps = 1 + (patch_manifest_list.len() * 4) + 1;

        // 创建进度条
        let main_pb = self.multi_pb.add(ProgressBar::new(total_steps as u64));
        main_pb.set_style(
            ProgressStyle::with_template("{prefix:.bold.dim} [{elapsed_precise}] [{bar}] {pos}/{len}: {msg}")
                .unwrap()
                .progress_chars("=> "),
        );
        main_pb.enable_steady_tick(Duration::from_millis(80));

        // 打开基础镜像
        let base_handle = self.wimgapi.open(
            base_image,
            WIM_GENERIC_READ | WIM_GENERIC_WRITE | WIM_GENERIC_MOUNT,
            WIM_OPEN_EXISTING,
            WIM_COMPRESS_NONE,
        )?;
        self.wimgapi
            .set_temp_path(base_handle, get_temp_path())
            .with_context(|| "Set temp path error")?;
        let base_image_handle = self
            .wimgapi
            .load_image(base_handle, base_index)
            .with_context(|| "Load base image error")?;

        // 获取基础镜像的卷信息
        let mut base_image_volumes = self
            .wimgapi
            .get_image_info(base_image_handle)
            .with_context(|| "Get image info error")?;

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
            .set_temp_path(patch_handle, get_temp_path())
            .with_context(|| "Set temp path error")?;

        // 挂载基础镜像
        main_pb.set_message(t!("create_patch.mount_base"));
        if !is_tty() {
            write_console(ConsoleType::Info, &t!("create_patch.mount_base"));
        }
        let base_mount = get_temp_path().join(get_tmp_name("base-", "", 6));
        if base_mount.exists() {
            fs::remove_dir_all(&base_mount).with_context(|| "Remove base image mount path error")?;
        }
        fs::create_dir_all(&base_mount).with_context(|| "Create base image mount path error")?;
        if let Err(e) = self.wimgapi.mount_image_handle(base_image_handle, &base_mount, 0) {
            self.wimgapi.close(base_image_handle)?;
            self.wimgapi.close(base_handle)?;
            return Err(anyhow!("Mount base image error: {:?}", e));
        }
        main_pb.inc(1);

        for (index, patch_manifest) in patch_manifest_list {
            main_pb.set_message(t!("apply_patch.mount_patch"));
            if !is_tty() {
                write_console(ConsoleType::Info, &t!("apply_patch.mount_patch"));
            }

            // 加载补丁镜像
            let patch_image_handle = self
                .wimgapi
                .load_image(patch_handle, *index)
                .with_context(|| "Load image error")?;

            // 创建补丁包挂载目录
            let patch_mount = get_temp_path().join(get_tmp_name("patch-", "", 6));
            if patch_mount.exists() {
                fs::remove_dir_all(&patch_mount).with_context(|| "Remove patch mount error")?;
            }
            fs::create_dir_all(&patch_mount).with_context(|| "Create patch mount error")?;

            // 挂载补丁镜像
            if let Err(e) = self
                .wimgapi
                .mount_image_handle(patch_image_handle, &patch_mount, WIM_FLAG_MOUNT_READONLY)
            {
                self.wimgapi.close(patch_image_handle)?;
                self.wimgapi.close(patch_handle)?;
                self.wimgapi.unmount_image_handle(base_image_handle).ok();
                self.wimgapi.close(base_image_handle).ok();
                self.wimgapi.close(base_handle).ok();
                return Err(anyhow!(format!("{}: {}", t!("apply_patch.mount_patch_failed"), e)));
            }
            main_pb.inc(1);

            // 合并镜像差异
            main_pb.set_message(t!("apply_patch.merge_diff"));
            if !is_tty() {
                write_console(ConsoleType::Info, &t!("apply_patch.merge_diff"));
            }

            // 应用文件操作
            if let Err(e) = self.apply_operations(&base_mount, &patch_mount, &patch_manifest.operations, exclude, force)
            {
                self.wimgapi.unmount_image_handle(base_image_handle).ok();
                self.wimgapi.close(base_image_handle).ok();
                self.wimgapi.close(base_handle).ok();
                self.wimgapi.unmount_image_handle(patch_image_handle).ok();
                self.wimgapi.close(patch_image_handle).ok();
                self.wimgapi.close(patch_handle).ok();

                return Err(anyhow!("Apply operations error: {:?}", e));
            }
            main_pb.inc(1);

            // 提交更改
            main_pb.set_message(t!("apply_patch.commit_changes"));
            if !is_tty() {
                write_console(ConsoleType::Info, &t!("apply_patch.commit_changes"));
            }
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
            if let Some(name) = &patch_manifest.target_image_info.name {
                base_image_volumes = replace_xml_field(&base_image_volumes, "NAME", name);
            }
            if let Some(display_name) = &&patch_manifest.target_image_info.display_name {
                base_image_volumes = replace_xml_field(&base_image_volumes, "DISPLAYNAME", display_name);
            }
            if let Some(flags) = &&patch_manifest.target_image_info.flags {
                base_image_volumes = replace_xml_field(&base_image_volumes, "FLAGS", flags);
            }
            if let Some(description) = &&patch_manifest.target_image_info.description {
                base_image_volumes = replace_xml_field(&base_image_volumes, "DESCRIPTION", description);
            }
            if let Some(display_description) = &&patch_manifest.target_image_info.display_description {
                base_image_volumes = replace_xml_field(&base_image_volumes, "DISPLAYDESCRIPTION", display_description);
            }
            main_pb.inc(1);

            main_pb.set_message(t!("apply_patch.unmount_patch"));
            if !is_tty() {
                write_console(ConsoleType::Info, &t!("apply_patch.unmount_patch"));
            }

            // 卸载补丁包镜像
            if let Err(e) = self.wimgapi.unmount_image_handle(patch_image_handle) {
                self.wimgapi.unmount_image_handle(base_image_handle).ok();
                self.wimgapi.close(base_image_handle).ok();
                self.wimgapi.close(base_handle).ok();
                self.wimgapi.close(patch_image_handle).ok();
                self.wimgapi.close(patch_handle).ok();
                return Err(anyhow!("{}: {}", t!("apply_patch.unmount_patch_failed"), e));
            }
            self.wimgapi
                .close(patch_image_handle)
                .with_context(|| "Close patch image handle error")?;
            main_pb.inc(1);
        }

        self.wimgapi
            .close(patch_handle)
            .with_context(|| "Close patch handle error")?;

        self.wimgapi
            .set_image_info(base_image_handle, &base_image_volumes)
            .with_context(|| "Set image info error")?;

        // 卸载基础镜像
        main_pb.set_message(t!("create_patch.unmount_base"));
        if !is_tty() {
            write_console(ConsoleType::Info, &t!("create_patch.unmount_base"));
        }
        if let Err(e) = self.wimgapi.unmount_image_handle(base_image_handle) {
            self.wimgapi.close(base_image_handle).ok();
            self.wimgapi.close(base_handle).ok();
            return Err(anyhow!("{}: {}", t!("create_patch.unmount_base_failed"), e));
        }
        self.wimgapi
            .close(base_image_handle)
            .with_context(|| "Close base image handle error")?;
        self.wimgapi
            .close(base_handle)
            .with_context(|| "Close base handle error")?;

        main_pb.inc(1);
        main_pb.set_message(format!(
            "{} ({}{})",
            t!("apply_patch.success"),
            t!("apply_patch.index"),
            base_index
        ));

        main_pb.finish_and_clear();
        Ok(())
    }

    /// 创建文件操作配置
    fn create_operations(
        &self,
        base_mount: &Path,
        target_mount: &Path,
        patch_path: &Path,
        storage: &Storage,
        preset: &Preset,
        exclude: Option<&[String]>,
    ) -> Result<Vec<Operation>> {
        let mut operations = Vec::new();

        // 创建进度条（用于显示具体操作进度）
        let sub_pb = self.multi_pb.add(ProgressBar::new(100));
        sub_pb.set_style(
            ProgressStyle::with_template("{prefix:.bold.dim} {spinner} {wide_msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        sub_pb.enable_steady_tick(Duration::from_millis(80));

        // 比较目录差异
        compare_directories(base_mount, target_mount, |diff_type, old, new, path| {
            // 检查是否需要排除
            if let Some(exclude) = exclude {
                for item in exclude {
                    if path.to_ascii_lowercase().contains(&item.to_ascii_lowercase()) {
                        sub_pb.set_message(format!("{} \\{}", t!("create_patch.exclude"), path));
                        return true;
                    }
                }
            }

            // 更新进度条消息
            let message = match diff_type {
                DiffType::Add => format!("{} \\{}", t!("create_patch.Add"), path),
                DiffType::Delete => format!("{} \\{}", t!("create_patch.Delete"), path),
                DiffType::Modify => format!("{} \\{}", t!("create_patch.Modify"), path),
            };
            sub_pb.set_message(message.clone());
            if !is_tty() {
                println!("{}", message);
            }

            // 构造补丁
            match diff_type {
                // 处理新增操作
                DiffType::Add => {
                    if let Some(new_path) = new {
                        operations.push(Operation {
                            action: Action::Add,
                            path: path.to_string(),
                            size: Some(new_path.metadata().unwrap().len()),
                            storage: None,
                        });

                        // 确保patch目录存在
                        let target_path = patch_path.join(path);
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
                // 处理删除操作
                DiffType::Delete => {
                    operations.push(Operation {
                        action: Action::Delete,
                        path: path.to_string(),
                        size: None,
                        storage: None,
                    });
                }
                // 处理修改操作
                DiffType::Modify => {
                    // 确保patch目录存在
                    if let Some(old_path) = old
                        && let Some(new_path) = new
                    {
                        // 创建父目录
                        if let Some(parent) = patch_path.join(path).parent()
                            && !parent.exists()
                            && let Err(e) = fs::create_dir_all(parent)
                        {
                            eprintln!("Create directory Failed: {:?}", e);
                        }

                        // 记录修改操作
                        operations.push(Operation {
                            action: Action::Modify,
                            path: path.to_string(),
                            size: Some(new_path.metadata().unwrap().len()),
                            storage: Some(match storage {
                                Storage::Full => "full".to_string(),
                                Storage::Zstd => "zstd".to_string(),
                                Storage::Bsdiff => "bsdiff".to_string(),
                            }),
                        });

                        // 处理修改操作
                        match storage {
                            Storage::Full => {
                                // 复制修改前的文件到patch目录
                                if let Err(e) = fs::copy(old_path, patch_path.join(path)) {
                                    eprintln!("Copy file Failed: {:?}", e);
                                }
                            }
                            Storage::Zstd => {
                                // 生成zstd差异文件
                                if let Err(e) = ZstdDiff::file_diff(
                                    old_path,
                                    new_path,
                                    patch_path.join(format!("{}.diff", path)),
                                    match preset {
                                        Preset::Fast => 3,
                                        Preset::Medium => 9,
                                        Preset::Best => 19,
                                        Preset::Extreme => 22,
                                    },
                                ) {
                                    eprintln!("Create diff file Failed: {:?}", e);
                                }
                            }
                            Storage::Bsdiff => {
                                // 生成bsdiff差异文件
                                if let Err(e) =
                                    BsDiff::file_diff(old_path, new_path, patch_path.join(format!("{}.diff", path)))
                                {
                                    eprintln!("Create diff file Failed: {:?}", e);
                                }
                            }
                        }
                    }
                }
            }
            true
        })?;

        // 完成子进度条
        sub_pb.finish_and_clear();

        Ok(operations)
    }

    /// 根据操作配置对基础镜像执行文件操作
    fn apply_operations(
        &self,
        base_mount: &Path,
        patch_mount: &Path,
        operations: &Vec<Operation>,
        exclude: Option<&[String]>,
        force: bool,
    ) -> Result<()> {
        // 创建子进度条，设置总长度为操作数量
        let sub_pb = self.multi_pb.add(ProgressBar::new(operations.len() as u64));
        sub_pb.set_style(
            ProgressStyle::with_template("{prefix:.bold.dim} {spinner} {wide_msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        sub_pb.enable_steady_tick(Duration::from_millis(80));

        for operation in operations {
            // 判断是否需要排除
            if let Some(exclude) = exclude
                && exclude.iter().any(|exclude_item| {
                    operation
                        .path
                        .to_ascii_lowercase()
                        .contains(&exclude_item.to_ascii_lowercase())
                })
            {
                sub_pb.set_message(format!("{} \\{}", t!("create_patch.exclude"), &operation.path));
                if !is_tty() {
                    write_console(
                        ConsoleType::Info,
                        &format!("{} \\{}", t!("create_patch.exclude"), &operation.path),
                    );
                }
                sub_pb.inc(1);
                continue;
            }

            match operation.action {
                // 新增操作
                Action::Add => {
                    let source_path = patch_mount.join(&operation.path);
                    let target_path = base_mount.join(&operation.path);

                    if source_path.is_dir() {
                        // 新建目录
                        fs::create_dir_all(&target_path)?;
                        continue;
                    }

                    sub_pb.set_message(format!("{} \\{}", t!("create_patch.Add"), &operation.path));
                    if !is_tty() {
                        write_console(
                            ConsoleType::Info,
                            &format!("{} \\{}", t!("create_patch.Add"), &operation.path),
                        );
                    }
                    // 确保目标目录存在
                    if let Some(parent) = target_path.parent() {
                        fs::create_dir_all(parent)
                            .with_context(|| format!("Create target directory Failed: {}", parent.display()))?;
                    }
                    if !source_path.exists() {
                        if force {
                            write_console(
                                ConsoleType::Warning,
                                &format!("Patch file source file not exist: \\{}", &operation.path),
                            );
                            continue;
                        }
                        return Err(anyhow!("Patch file source file not exist: \\{}", &operation.path));
                    }
                    // 复制文件
                    if let Err(e) = fs::copy(&source_path, &target_path) {
                        if force {
                            write_console(
                                ConsoleType::Warning,
                                &format!(
                                    "Copy file Failed: {} -> {} ({})",
                                    source_path.display(),
                                    target_path.display(),
                                    e
                                ),
                            );
                            continue;
                        }
                        return Err(anyhow!(format!(
                            "Copy file Failed: {} -> {} ({})",
                            source_path.display(),
                            target_path.display(),
                            e
                        )));
                    }
                    sub_pb.inc(1);
                }
                // 删除操作
                Action::Delete => {
                    let target_path = base_mount.join(&operation.path);
                    sub_pb.set_message(format!("{} \\{}", t!("create_patch.Delete"), &operation.path));
                    if !is_tty() {
                        write_console(
                            ConsoleType::Info,
                            &format!("{} \\{}", t!("create_patch.Delete"), &operation.path),
                        );
                    }
                    if target_path.exists() {
                        if target_path.is_dir() {
                            if let Err(e) = fs::remove_dir_all(&target_path) {
                                if force {
                                    write_console(
                                        ConsoleType::Warning,
                                        &format!("Delete directory Failed: {} -> {}", target_path.display(), e),
                                    );
                                    continue;
                                }
                                return Err(anyhow!(format!(
                                    "Delete directory Failed: {} -> {}",
                                    target_path.display(),
                                    e
                                )));
                            }
                        } else {
                            if let Err(e) = fs::remove_file(&target_path) {
                                if force {
                                    write_console(
                                        ConsoleType::Warning,
                                        &format!("Delete file Failed: {} -> {}", target_path.display(), e),
                                    );
                                    continue;
                                }
                                return Err(anyhow!(format!(
                                    "Delete file Failed: {} -> {}",
                                    target_path.display(),
                                    e
                                )));
                            }
                        }
                    }
                    sub_pb.inc(1);
                }
                // 修改操作
                Action::Modify => {
                    let source_path = patch_mount.join(&operation.path);
                    let target_path = base_mount.join(&operation.path);

                    sub_pb.set_message(format!("{} \\{}", t!("create_patch.Modify"), &operation.path));
                    if !is_tty() {
                        write_console(
                            ConsoleType::Info,
                            &format!("{} \\{}", t!("create_patch.Modify"), &operation.path),
                        );
                    }

                    if let Some(storage) = &operation.storage {
                        match storage.to_lowercase().as_str() {
                            "full" => {
                                // 复制文件
                                if let Err(e) = fs::copy(&source_path, &target_path) {
                                    if force {
                                        write_console(
                                            ConsoleType::Warning,
                                            &format!(
                                                "Copy file Failed: {} -> {} ({})",
                                                source_path.display(),
                                                target_path.display(),
                                                e
                                            ),
                                        );
                                        continue;
                                    }
                                    return Err(anyhow!(format!(
                                        "Copy file Failed: {} -> {} ({})",
                                        source_path.display(),
                                        target_path.display(),
                                        e
                                    )));
                                }
                            }
                            "zstd" => {
                                // 应用zstdiff差异文件
                                let patch_path = patch_mount.join(format!("{}.diff ", &operation.path));
                                if patch_path.exists() {
                                    if let Err(e) = ZstdDiff::file_patch(&target_path, &patch_path, &target_path) {
                                        // 应用zstdiff差异文件失败
                                        if force {
                                            sub_pb.println(format!(
                                                " {}      {}: {} ({})",
                                                style(t!("console.error")).red(),
                                                t!("apply_patch.diff_failed"),
                                                target_path
                                                    .display()
                                                    .to_string()
                                                    .strip_prefix(base_mount.display().to_string().as_str())
                                                    .unwrap(),
                                                e
                                            ));
                                            continue;
                                        }
                                        return Err(anyhow!(format!(
                                            "{}: {} ({})",
                                            t!("apply_patch.diff_failed"),
                                            target_path
                                                .display()
                                                .to_string()
                                                .strip_prefix(base_mount.display().to_string().as_str())
                                                .unwrap(),
                                            e
                                        )));
                                    }
                                } else {
                                    // zstdiff差异文件不存在
                                    if force {
                                        write_console(
                                            ConsoleType::Warning,
                                            &format!("Patch file zstdiff patch file not exist: \\{}", &operation.path),
                                        );
                                        continue;
                                    }
                                    return Err(anyhow!(format!(
                                        "Patch file zstdiff patch file not exist: \\{}",
                                        &operation.path
                                    )));
                                }
                            }
                            "bsdiff" => {
                                // 应用bsdiff差异文件
                                let patch_path = patch_mount.join(format!("{}.diff ", &operation.path));
                                if patch_path.exists() {
                                    if let Err(e) = BsDiff::file_patch(&target_path, &patch_path, &target_path) {
                                        // 应用bsdiff差异文件失败
                                        if force {
                                            sub_pb.println(format!(
                                                " {}      {}: {} ({})",
                                                style(t!("console.error")).red(),
                                                t!("apply_patch.bsdiff_failed"),
                                                target_path
                                                    .display()
                                                    .to_string()
                                                    .strip_prefix(base_mount.display().to_string().as_str())
                                                    .unwrap(),
                                                e
                                            ));
                                            continue;
                                        }
                                        return Err(anyhow!(format!(
                                            "{}: {} ({})",
                                            t!("apply_patch.bsdiff_failed"),
                                            target_path
                                                .display()
                                                .to_string()
                                                .strip_prefix(base_mount.display().to_string().as_str())
                                                .unwrap(),
                                            e
                                        )));
                                    }
                                } else {
                                    // bsdiff差异文件不存在
                                    if force {
                                        write_console(
                                            ConsoleType::Warning,
                                            &format!("Patch file bsdiff patch file not exist: \\{}", &operation.path),
                                        );
                                        continue;
                                    }
                                    return Err(anyhow!(format!(
                                        "Patch file bsdiff patch file not exist: \\{}",
                                        &operation.path
                                    )));
                                }
                            }
                            _ => {}
                        }
                    }
                    sub_pb.inc(1);
                }
            }
        }

        Ok(())
    }

    /// 根据传入的基础 WIM GUID 和卷索引构建补丁链。
    ///
    /// # 参数
    ///
    /// - `base_guid` - 外部传入的基础 WIM GUID
    /// - `base_image_info_list` - 基础镜像信息列表
    /// - `patch_info_list` - 补丁包信息列表
    /// - `force_mode` - 是否强制应用补丁 (对应 --force 参数)
    ///
    /// # 返回值
    ///
    /// - `Vec<(ImageInfo, Vec<(u32, PatchManifest)>)>` - 匹配的基础镜像和补丁包列表
    fn match_patch(
        &self,
        base_guid: &str,
        base_image_info_list: &[ImageInfo],
        patch_info_list: &[(u32, PatchManifest)],
        force_mode: bool,
    ) -> Result<Vec<(ImageInfo, Vec<(u32, PatchManifest)>)>> {
        // 返回的 ImageInfo 是应用所有补丁后的最终目标卷信息
        let mut result: Vec<(ImageInfo, Vec<(u32, PatchManifest)>)> = Vec::new();

        // 用于记录已经被添加到某个链条中的补丁索引，避免重复使用
        let mut all_applied_indices: HashSet<u32> = HashSet::new();

        // 遍历所有可能的起始基础镜像卷
        for initial_base_info in base_image_info_list.iter() {
            let mut current_base_info = initial_base_info.clone();
            let mut patch_chain: Vec<(u32, PatchManifest)> = Vec::new();

            // 循环构建补丁链
            loop {
                // 查找所有以当前身份为基线的未应用的候选补丁
                let mut candidates: Vec<(u32, PatchManifest)> = patch_info_list
                    .iter()
                    .filter(|(index, patch)| {
                        // 身份匹配：补丁期望的基线 WIM GUID 和 Index 必须与当前的卷身份匹配
                        current_base_info.index == patch.base_image_info.index
                            && base_guid == patch.base_image_guid
                            && !all_applied_indices.contains(index)
                    })
                    .map(|(index, patch)| (*index, patch.clone()))
                    .collect();

                // 如果没有找到任何候选补丁，则链条结束
                if candidates.is_empty() {
                    break;
                }

                // 版本号排序
                candidates.sort_by(|a, b| {
                    // 确保按版本号升序应用
                    let version_a = Version::parse(&a.1.patch_version).unwrap_or_else(|_| Version::new(0, 0, 0));
                    let version_b = Version::parse(&b.1.patch_version).unwrap_or_else(|_| Version::new(0, 0, 0));
                    version_a.cmp(&version_b)
                });

                // 选择并校验
                let (index, next_patch) = candidates.remove(0);

                // [核心校验] 在非强制模式下，检查当前基础卷的统计信息是否与补丁期望的基线一致
                if current_base_info != next_patch.base_image_info {
                    if !force_mode {
                        return Err(anyhow!(
                            "{}",
                            t!("apply_patch.base_not_match", index = current_base_info.index),
                        ));
                    }
                    write_console(
                        ConsoleType::Warning,
                        &format!(
                            "{}",
                            t!("apply_patch.base_stat_not_match", index = current_base_info.index)
                        ),
                    );
                }

                // 更新链条状态
                current_base_info = next_patch.target_image_info.clone();
                patch_chain.push((index, next_patch));
                all_applied_indices.insert(index);
            }

            // 如果找到了补丁链，将结果加入
            if !patch_chain.is_empty() {
                result.push((current_base_info, patch_chain));
            }
        }

        Ok(result)
    }

    /// 合并多个补丁包
    ///
    /// # 参数
    ///
    /// * `patches` - 补丁包文件路径列表
    /// * `out` - 输出合并后的补丁包文件路径
    /// * `compress` - 压缩算法
    ///
    /// # 返回值
    ///
    /// * `Ok(())` - 合并成功
    /// * `Err` - 发生错误
    pub fn merge_patches(&self, patches: &[PathBuf], out: &Path, compress: Compress) -> Result<()> {
        let merge_patch_handle = self
            .wimgapi
            .open(
                out,
                WIM_GENERIC_WRITE,
                WIM_CREATE_ALWAYS,
                match compress {
                    Compress::None => WIM_COMPRESS_NONE,
                    Compress::Xpress => WIM_COMPRESS_XPRESS,
                    Compress::Lzx => WIM_COMPRESS_LZX,
                },
            )
            .with_context(|| "Open out patch error ")?;

        self.wimgapi
            .set_temp_path(merge_patch_handle, get_temp_path())
            .with_context(|| "Set temp path error ")?;

        // 遍历补丁包
        for patch_path in patches {
            write_console(
                ConsoleType::Info,
                &format!("{}: {}", t!("merge_patch.merge_patch"), patch_path.display()),
            );
            let patch_handle = self
                .wimgapi
                .open(patch_path, WIM_GENERIC_READ, WIM_OPEN_EXISTING, WIM_COMPRESS_NONE)
                .with_context(|| "Open patch error ")?;

            self.wimgapi
                .set_temp_path(patch_handle, get_temp_path())
                .with_context(|| "Set temp path error ")?;

            for index in 1..=self.wimgapi.get_image_count(patch_handle) {
                let patch_image_handle = self
                    .wimgapi
                    .load_image(patch_handle, index)
                    .with_context(|| "Load patch image error ")?;

                self.wimgapi
                    .export_image(patch_image_handle, merge_patch_handle, 0)
                    .with_context(|| "Export patch image error ")?;

                self.wimgapi
                    .close(patch_image_handle)
                    .with_context(|| "Close patch image handle error ")?;
            }

            self.wimgapi
                .close(patch_handle)
                .with_context(|| "Close patch handle error ")?;
        }

        self.wimgapi
            .close(merge_patch_handle)
            .with_context(|| "Close out patch error ")?;
        Ok(())
    }

    /// 清理无效的挂载点
    ///
    /// # 返回值
    ///
    /// - `Ok(())` - 成功清理
    /// - `Err(anyhow::Error)` - 失败，返回错误信息
    pub fn clean(&self) -> Result<()> {
        // 获取所有挂载点
        let mounted_images: Vec<WimMountInfoLevel1> = self
            .wimgapi
            .get_mounted_image()
            .with_context(|| "Get mounted image error ")?
            .into_iter()
            // 过滤无效挂载点
            .filter(|mount_info| {
                (mount_info.mount_flags & (WIM_MOUNT_FLAG_INVALID | WIM_MOUNT_FLAG_NO_WIM | WIM_MOUNT_FLAG_NO_MOUNTDIR))
                    != 0
            })
            .collect();

        // 检查是否有无效挂载点
        if mounted_images.is_empty() {
            Err(anyhow!("{}", t!("clean.not_invalid_mount")))?;
        }

        // 遍历挂载点并尝试卸载
        for mount_info in mounted_images {
            let result = self.wimgapi.unmount_image(
                Path::new(&mount_info.mount_path),
                mount_info.wim_path.as_ref(),
                mount_info.image_index,
                false,
            );

            write_console(
                match result {
                    Ok(_) => ConsoleType::Info,
                    Err(_) => ConsoleType::Error,
                },
                &format!("{}: {}", t!("clean.unmount"), mount_info.mount_path),
            );
        }

        Ok(())
    }

    /// 获取 WIM 文件中的镜像数量
    ///
    /// # 参数
    ///
    /// - `image_path` - WIM 文件路径
    ///
    /// # 返回值
    ///
    /// - `Ok(u32)` - 镜像数量
    /// - `Err(anyhow::Error)` - 失败，返回错误信息
    pub fn get_image_count(&self, image_path: &Path) -> Result<u32> {
        let handle = self
            .wimgapi
            .open(image_path, WIM_GENERIC_READ, WIM_OPEN_EXISTING, WIM_COMPRESS_NONE)
            .with_context(|| "Open image error ")?;
        let count = self.wimgapi.get_image_count(handle);
        self.wimgapi
            .close(handle)
            .with_context(|| "Close image handle error ")?;
        Ok(count)
    }
}
