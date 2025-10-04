use anyhow::{Context, Result};
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use zstd::{Decoder, Encoder};

pub struct ZstdDiff {}

impl ZstdDiff {
    /// 生成zstd差异补丁
    ///
    /// # 参数
    /// - `base`: 原始文件内容
    /// - `new`: 新文件内容
    /// - `level`: 压缩级别，范围为0至22，0表示无压缩，22表示最大压缩
    ///
    /// # 返回值
    /// - `Result<Vec<u8>>`: 操作结果，成功返回Ok(差异补丁内容)，失败返回对应的错误信息
    pub fn diff(base: &[u8], new: &[u8], level: i32) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        let mut encoder = Encoder::with_dictionary(&mut buffer, level, base)?;
        encoder.write_all(new)?;
        let result = encoder
            .finish()
            .with_context(|| "Failed to finish encoding")?;
        Ok(result.to_owned())
    }

    /// 应用zstd差异补丁
    ///
    /// # 参数
    /// - `base`: 原始文件内容
    /// - `patch`: 差异补丁内容
    ///
    /// # 返回值
    /// - `Result<Vec<u8>>`: 操作结果，成功返回Ok(新文件内容)，失败返回对应的错误信息
    pub fn patch(base: &[u8], patch: &[u8]) -> Result<Vec<u8>> {
        // 创建带有字典的解码器
        let mut decoder = Decoder::with_dictionary(Cursor::new(&patch), base)
            .with_context(|| "Failed to create decoder with dictionary")?;
        let mut result = Vec::new();
        decoder
            .read_to_end(&mut result)
            .with_context(|| "Failed to decode patch")?;
        Ok(result)
    }

    /// 生成zstd差异补丁文件
    ///
    /// # 参数
    /// - `old_file`: 原始文件路径
    /// - `new_file`: 新文件路径
    /// - `patch_file`: 输出的补丁文件路径
    ///
    /// # 返回值
    /// 成功时返回Ok(())，失败时返回Err
    pub fn file_diff(
        old_file: impl AsRef<Path>,
        new_file: impl AsRef<Path>,
        patch_file: impl AsRef<Path>,
        level: i32,
    ) -> Result<()> {
        // 读取旧文件内容作为字典
        let mut old_file_content = Vec::new();
        File::open(old_file)?
            .read_to_end(&mut old_file_content)
            .with_context(|| "Failed to read old file")?;

        // 读取新文件内容
        let mut new_file_content = Vec::new();
        File::open(new_file)?
            .read_to_end(&mut new_file_content)
            .with_context(|| "Failed to read new file")?;

        // 创建输出缓冲区
        let mut buffer = Vec::new();

        // 创建带有字典的编码器
        let mut encoder = Encoder::with_dictionary(&mut buffer, level, &old_file_content)
            .with_context(|| "Failed to create encoder with dictionary")?;
        encoder.write_all(&new_file_content)?;
        let patch_content = encoder
            .finish()
            .with_context(|| "Failed to finish encoding")?;

        // 写入补丁文件
        File::create(patch_file)?
            .write_all(patch_content)
            .with_context(|| "Failed to write patch file")?;

        Ok(())
    }

    /// 应用zstd差异补丁文件
    ///
    /// # 参数
    /// - `old_file`: 原始文件路径
    /// - `patch_file`: 补丁文件路径
    /// - `new_file`: 输出的新文件路径
    ///
    /// # 返回值
    /// 成功时返回Ok(())，失败时返回Err
    pub fn file_patch(
        old_file: impl AsRef<Path>,
        patch_file: impl AsRef<Path>,
        new_file: impl AsRef<Path>,
    ) -> Result<()> {
        // 读取旧文件内容作为字典
        let mut old_file_content = Vec::new();
        File::open(old_file)?
            .read_to_end(&mut old_file_content)
            .with_context(|| "Failed to read old file")?;

        // 读取补丁文件内容
        let mut patch_content = Vec::new();
        File::open(patch_file)?
            .read_to_end(&mut patch_content)
            .with_context(|| "Failed to read patch file")?;

        // 创建带有字典的解码器
        let mut decoder = Decoder::with_dictionary(Cursor::new(&patch_content), &old_file_content)
            .with_context(|| "Failed to create decoder with dictionary")?;
        let mut new_file_content = Vec::new();
        decoder
            .read_to_end(&mut new_file_content)
            .with_context(|| "Failed to decode patch")?;

        // 写入新文件
        File::create(new_file)?
            .write_all(&new_file_content)
            .with_context(|| "Failed to write new file")?;

        Ok(())
    }
}
