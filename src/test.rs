#[cfg(test)]
mod tests {
    use crate::bsdiff::BsDiff;
    use crate::utils::{compare_directories, replace_xml_field, DiffType};
    use crate::wimgapi::{
        Wimgapi, WIM_COMPRESS_LZX, WIM_COMPRESS_NONE, WIM_CREATE_ALWAYS,
        WIM_FLAG_MOUNT_READONLY, WIM_GENERIC_MOUNT, WIM_GENERIC_READ, WIM_GENERIC_WRITE, WIM_MSG_PROCESS,
        WIM_MSG_PROGRESS, WIM_OPEN_EXISTING, WIM_REFERENCE_APPEND,
    };
    use crate::zstdiff::ZstdDiff;
    use crate::TEMP_PATH;
    use indicatif::{ProgressBar, ProgressStyle};
    use std::path::PathBuf;
    use std::thread::sleep;
    use std::time::Duration;
    use std::{fs, ptr, thread};

    /// 进度条测试
    #[test]
    fn test_progress() {
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(Duration::from_millis(80));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} {msg}")
                .unwrap()
                // For more spinners check out the cli-spinners project:
                // https://github.com/sindresorhus/cli-spinners/blob/master/spinners.json
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.set_message("Calculating...");
        thread::sleep(Duration::from_secs(5));

        pb.finish_with_message("Done");
    }

    /// 对比目录差异
    #[test]
    fn compare_dir() {
        let src = PathBuf::from(r"D:\UserData\Desktop\test\WimPatch\base");
        let update = PathBuf::from(r"D:\UserData\Desktop\test\WimPatch\Update");
        let patch = PathBuf::from(r"D:\UserData\Desktop\test\WimPatch\Patch");

        if let Err(err) = compare_directories(src, &update, |diff_type, old, new, path| {
            // 构造补丁
            match diff_type {
                DiffType::Added => {
                    println!("添加: {:?}", path);
                    if let Some(new_path) = new {
                        // 确保patch目录存在
                        let target_path = patch.join(path);
                        if new_path.is_dir() {
                            if let Err(e) = fs::create_dir_all(&target_path) {
                                eprintln!("创建目录失败: {:?}", e);
                            }
                            return true;
                        }
                        // 创建父目录
                        if let Some(parent) = target_path.parent() {
                            if !parent.exists() {
                                if let Err(e) = fs::create_dir_all(parent) {
                                    eprintln!("创建目录失败: {:?}", e);
                                }
                            }
                        }
                        // 复制新增的文件到patch目录
                        if let Err(e) = fs::copy(new_path, &target_path) {
                            eprintln!("复制文件失败: {:?}", e);
                        }
                    }
                }
                DiffType::Removed => {
                    println!("删除: {:?}", path);
                }
                DiffType::Modified => {
                    println!("修改: {:?}", path);
                    // 确保patch目录存在
                    if let Some(old_path) = old {
                        if let Some(new_path) = new {
                            let target_path = patch.join(format!("{}.diff", path));
                            // 创建父目录
                            if let Some(parent) = target_path.parent() {
                                if !parent.exists() {
                                    if let Err(e) = fs::create_dir_all(parent) {
                                        eprintln!("创建目录失败: {:?}", e);
                                    }
                                }
                            }

                            BsDiff::file_diff(old_path, new_path, target_path).unwrap();
                            // 复制修改前的文件到patch目录
                            // if let Err(e) = fs::copy(old_path, &target_path) {
                            //     eprintln!("复制文件失败: {:?}", e);
                            // }
                        }
                    }
                }
            }
            return true;
        }) {
            eprintln!("比较目录时出错: {:?}", err);
        }
    }

    #[test]
    fn file_patch_zstd() {
        let old_file = PathBuf::from(r"D:\UserData\Desktop\About1.exe");
        let updated_file = PathBuf::from(r"D:\UserData\Desktop\About2.exe");

        let patch_file = old_file.parent().unwrap().join(format!(
            "{}.diff",
            old_file.file_name().unwrap().to_string_lossy()
        ));
        let new_file = old_file.parent().unwrap().join(format!(
            "{}-new.{}",
            old_file.file_stem().unwrap().to_string_lossy(),
            old_file.extension().unwrap().to_string_lossy()
        ));

        ZstdDiff::file_diff(&old_file, updated_file, &patch_file, 9).unwrap();
        ZstdDiff::file_patch(old_file, patch_file, new_file).unwrap();
    }

    // 回调函数，用于处理WIM消息并显示进度
    #[allow(non_snake_case)]
    extern "system" fn WIMMessageCallback(
        dwMessageId: u32,
        wParam: usize,
        lParam: isize,
        pvUserData: *mut std::ffi::c_void,
    ) -> u32 {
        match dwMessageId {
            // 进度回调
            WIM_MSG_PROGRESS => {
                println!("进度: {}, 剩余: {}秒", wParam, lParam / 1000);
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

    // 创建wim
    #[test]
    fn create() {
        let src = PathBuf::from(r"D:\UserData\Desktop\test\WimPatch\base");
        let target = PathBuf::from(format!("{}.wim", src.to_string_lossy()));

        let wimgapi = Wimgapi::new(None).unwrap();

        // 创建wim
        let handle = wimgapi
            .open(
                &target,
                WIM_GENERIC_WRITE,
                WIM_CREATE_ALWAYS,
                WIM_COMPRESS_LZX,
            )
            .unwrap();

        // 设置捕获标志以排除系统文件
        // let capture_flags =
        //     WIM_FLAG_EXCLUDE_HIDDEN | WIM_FLAG_EXCLUDE_SYSTEM | WIM_FLAG_EXCLUDE_CRITICAL;

        // 注册消息回调函数以显示进度和排除特定路径
        wimgapi.register_message_callback(handle, WIMMessageCallback);

        // 捕获src目录到wim
        let hImage = wimgapi.capture(handle, &src, 0).unwrap();

        // 注销消息回调函数
        wimgapi.unregister_message_callback(handle, WIMMessageCallback);

        // 在</IMAGE>标签前添加基本字段信息
        let image_info = wimgapi.get_image_info(hImage).unwrap();
        let updated_image_info = if let Some(pos) = image_info.rfind("</IMAGE>") {
            let prefix = &image_info[..pos];
            let suffix = &image_info[pos..];
            format!(
                // 映像名称
                "{}<NAME></NAME>\
                <DESCRIPTION></DESCRIPTION>\
                <DISPLAYNAME></DISPLAYNAME>\
                <DISPLAYDESCRIPTION></DISPLAYDESCRIPTION>\
                <FLAGS></FLAGS>{}",
                prefix, suffix
            )
        } else {
            // 如果没找到</IMAGE>标签，则保持原样
            image_info
        };

        // 将更新后的XML信息设置回映像
        wimgapi.set_image_info(hImage, &updated_image_info).unwrap();

        wimgapi.close(hImage).unwrap();
        wimgapi.close(handle).unwrap();

        println!(
            "WIM file created successfully at: {}",
            target.to_string_lossy()
        );
    }

    // 创建引用差分wim
    #[test]
    fn create_patch() {
        let wimgapi = Wimgapi::new(None).unwrap();
        let base_wim = PathBuf::from(r"D:\UserData\Desktop\test\WimPatch\base.wim");
        let target_path = PathBuf::from(r"D:\UserData\Desktop\test\WimPatch\Patch");
        let save_path = PathBuf::from(r"D:\UserData\Desktop\test\WimPatch\Patch.wim");

        // 打开 base.wim（只读）
        let h_base = wimgapi
            .open(
                &base_wim,
                WIM_GENERIC_READ,
                WIM_OPEN_EXISTING,
                WIM_COMPRESS_NONE,
            )
            .unwrap();
        println!("open base.wim: {}", h_base);

        // 创建 patch.wim（写入，指定压缩）
        let h_patch = wimgapi
            .open(
                &save_path,
                WIM_GENERIC_WRITE,
                WIM_CREATE_ALWAYS,
                WIM_COMPRESS_LZX,
            )
            .unwrap();
        println!("open patch.wim: {}", h_patch);

        // 让 patch 引用 base 的资源（“引用式 delta”的关键）
        println!(
            "set ref: {:?}",
            wimgapi
                .set_reference_file(h_patch, &base_wim, WIM_REFERENCE_APPEND)
                .unwrap()
        );

        // 捕获 src_dir 到 patch（此刻写数据时会尽量“引用 base”而不重复写）
        let h_image = wimgapi.capture(h_patch, &target_path, 0).unwrap();
        println!("capture: {}", h_image);

        // 提交新镜像（把 image 元数据/目录表等落盘）
        wimgapi.commit(h_image, 0).unwrap();

        // 关闭句柄
        wimgapi.close(h_image).unwrap();
        wimgapi.close(h_patch).unwrap();
        wimgapi.close(h_base).unwrap();
    }

    // 获取挂载的wim镜像
    #[test]
    fn get_mounted_image_info() {
        let wimgapi = Wimgapi::new(None).unwrap();
        let mounted_images = wimgapi.get_mounted_image().unwrap();
        for image in mounted_images {
            println!("{:#?}", image);
        }
    }

    // 挂载镜像测试
    #[test]
    fn mount_image() {
        let wim_path = PathBuf::from(r"D:\UserData\Desktop\test\WimPatch\base.wim");
        let mount_path = TEMP_PATH.join("mount");
        // 确保挂载目录存在
        if !mount_path.exists() {
            fs::create_dir_all(&mount_path).unwrap();
        }
        let wimgapi = Wimgapi::new(None).unwrap();
        let h_wim = wimgapi
            .open(
                &wim_path,
                WIM_GENERIC_READ | WIM_GENERIC_MOUNT,
                WIM_OPEN_EXISTING,
                WIM_COMPRESS_NONE,
            )
            .unwrap();

        wimgapi.set_temp_path(h_wim, &TEMP_PATH).unwrap();
        let h_image = wimgapi.load_image(h_wim, 1).unwrap();
        wimgapi
            .mount_image_handle(h_image, &mount_path, WIM_FLAG_MOUNT_READONLY)
            .unwrap();

        sleep(Duration::from_secs(3));

        // 卸载镜像
        wimgapi.unmount_image_handle(h_image).unwrap();

        wimgapi.close(h_image).unwrap();
        wimgapi.close(h_wim).unwrap();
    }

    #[test]
    fn get_mount_image_info() {
        let wimgapi = Wimgapi::new(None).unwrap();

        let mounted_images = wimgapi.get_mounted_image().unwrap();
        println!("{}", mounted_images.len());
        for image in mounted_images {
            println!("{:#?}", image);
        }
    }

    #[test]
    fn modify_image_info() {
        let xml = r#"
	<IMAGE INDEX="1">
		<DIRCOUNT>713</DIRCOUNT>
		<FILECOUNT>5380</FILECOUNT>
		<TOTALBYTES>1513254928</TOTALBYTES>
		<HARDLINKBYTES>70049826</HARDLINKBYTES>
		<CREATIONTIME>
			<HIGHPART>0x01DA844F</HIGHPART>
			<LOWPART>0xF3BEDE84</LOWPART>
		</CREATIONTIME>
		<LASTMODIFICATIONTIME>
			<HIGHPART>0x01DB5F6E</HIGHPART>
			<LOWPART>0x47D41114</LOWPART>
		</LASTMODIFICATIONTIME>
		<WIMBOOT>0</WIMBOOT>
		<NAME>Windows 11PE 网络版</NAME>
		<DESCRIPTION>Windows 11PE 网络版</DESCRIPTION>
		<FLAGS>WindowsPE</FLAGS>
		<DISPLAYNAME>Windows 11PE 网络版</DISPLAYNAME>
		<DISPLAYDESCRIPTION>Windows 11PE 网络版</DISPLAYDESCRIPTION>
	</IMAGE>
	"#;

        println!("原始 XML:");
        println!("{}", xml);

        // 使用更简单的字符串替换方法，但处理不同值的情况
        let modified_xml = replace_xml_field(&xml, "NAME", "Windows 11PE 专业版");
        let modified_xml =
            replace_xml_field(&modified_xml, "DESCRIPTION", "Windows 11PE 专业增强版");
        let modified_xml =
            replace_xml_field(&modified_xml, "DISPLAYNAME", "Windows 11PE Professional");
        let modified_xml = replace_xml_field(
            &modified_xml,
            "DISPLAYDESCRIPTION",
            "Windows 11PE Professional Enhanced Edition",
        );

        println!("\n修改后的 XML:");
        println!("{}", modified_xml);

        // 验证修改结果
        assert!(modified_xml.contains("<NAME>Windows 11PE 专业版</NAME>"));
        assert!(modified_xml.contains("<DESCRIPTION>Windows 11PE 专业增强版</DESCRIPTION>"));
        assert!(modified_xml.contains("<DISPLAYNAME>Windows 11PE Professional</DISPLAYNAME>"));
        assert!(modified_xml.contains(
            "<DISPLAYDESCRIPTION>Windows 11PE Professional Enhanced Edition</DISPLAYDESCRIPTION>"
        ));

        println!("\n修改验证成功！所有字段都已成功更新。");
    }
}
