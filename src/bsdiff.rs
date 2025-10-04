use std::fs;
use std::fs::File;
use std::path::Path;

pub struct BsDiff {}

impl BsDiff {
    /// 创建差异文件
    ///
    /// # 参数
    /// - `old_file`: 旧文件路径
    /// - `update_file`: 更新后的文件路径
    /// - `patch_file`: 输出的bsdiff文件路径
    ///
    /// # 返回值
    /// - `std::io::Result<()>`: 操作结果，成功返回Ok(())，失败返回对应的错误信息
    pub fn file_diff(
        old_file: impl AsRef<Path>,
        update_file: impl AsRef<Path>,
        patch_file: impl AsRef<Path>,
    ) -> std::io::Result<()> {
        let old = fs::read(old_file)?;
        let update = fs::read(update_file)?;
        let mut patch = Vec::new();

        bsdiff::diff(&old, &update, &mut patch)?;
        fs::write(patch_file, &patch)?;
        Ok(())
    }

    /// 修补文件
    ///
    /// # 参数
    /// - `old_file`: 旧文件路径
    /// - `patch_file`: bsdiff文件路径
    /// - `new_file`: 输出的新文件路径
    ///
    /// # 返回值
    /// - `Result<()>`: 操作结果，成功返回Ok(())，失败返回对应的错误信息
    pub fn file_patch(
        old_file: impl AsRef<Path>,
        patch_file: impl AsRef<Path>,
        new_file: impl AsRef<Path>,
    ) -> std::io::Result<()> {
        let old = fs::read(old_file)?;
        let mut patch = File::open(patch_file)?;
        let mut new = Vec::new();
        bsdiff::patch(&old, &mut patch, &mut new)?;
        fs::write(new_file, &new)?;
        Ok(())
    }
}
