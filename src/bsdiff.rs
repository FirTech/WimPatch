use anyhow::{Context, Result};
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

pub struct BsDiff {}

impl BsDiff {
    /// 创建差异文件
    ///
    /// # 参数
    /// - `old_file_path`: 旧文件路径
    /// - `update_file_path`: 更新后的文件路径
    /// - `patch_file_path`: 输出的bsdiff文件路径
    ///
    /// # 返回值
    /// - `Result<()>`: 操作结果，成功返回Ok(())，失败返回对应的错误信息
    pub fn file_diff(
        old_file_path: impl AsRef<Path>,
        new_file_path: impl AsRef<Path>,
        patch_file_path: impl AsRef<Path>,
    ) -> Result<()> {
        let old = fs::read(old_file_path).with_context(|| "Read old file error")?;
        let update = fs::read(new_file_path).with_context(|| "Read new file error")?;

        let patch_file = File::create(patch_file_path).with_context(|| "Create patch file failed".to_string())?;
        let mut writer = BufWriter::new(patch_file);

        bsdiff::diff(&old, &update, &mut writer)?;
        writer.flush().with_context(|| "Flush patch writer failed")?;
        Ok(())
    }

    /// 修补文件
    ///
    /// # 参数
    /// - `old_file_path`: 旧文件路径
    /// - `patch_file_path`: bsdiff文件路径
    /// - `new_file_path`: 输出的新文件路径
    ///
    /// # 返回值
    /// - `Result<()>`: 操作结果，成功返回Ok(())，失败返回对应的错误信息
    pub fn file_patch(
        old_file_path: impl AsRef<Path>,
        patch_file_path: impl AsRef<Path>,
        new_file_path: impl AsRef<Path>,
    ) -> Result<()> {
        let old = fs::read(old_file_path).with_context(|| "Read old file error")?;
        let mut patch = File::open(patch_file_path).with_context(|| "Open patch file error")?;
        let mut new = Vec::new();

        bsdiff::patch(&old, &mut patch, &mut new)?;
        fs::write(new_file_path, &new).with_context(|| "Write new file error")?;
        Ok(())
    }
}
