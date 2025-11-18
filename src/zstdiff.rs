use anyhow::{Context, Result};
use std::fs::File;
use std::io::{copy, BufReader, BufWriter, Cursor, Read, Write};
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
        let mut encoder = Encoder::with_dictionary(&mut buffer, level, base)
            .with_context(|| "Failed to create encoder with dictionary")?;
        encoder
            .write_all(new)
            .with_context(|| "Failed to write new data to encoder")?;
        let result = encoder.finish().with_context(|| "Failed to finish encoding")?;
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
    /// - `old_file_path`: 原始文件路径
    /// - `new_file_path`: 新文件路径
    /// - `patch_file_path`: 输出的补丁文件路径
    /// - `level`: 压缩级别，范围为0至22，0表示无压缩，22表示最大压缩
    ///
    /// # 返回值
    /// 成功时返回Ok(())，失败时返回Err
    pub fn file_diff(
        old_file_path: impl AsRef<Path>,
        new_file_path: impl AsRef<Path>,
        patch_file_path: impl AsRef<Path>,
        level: i32,
    ) -> Result<()> {
        // 读取旧文件
        let mut old_file_content = Vec::new();
        File::open(old_file_path)?
            .read_to_end(&mut old_file_content)
            .with_context(|| "Read old file failed")?;

        // 读取新文件
        let new_file = File::open(new_file_path).with_context(|| "Open new file failed")?;
        let mut new_reader = BufReader::new(new_file);

        // 创建补丁文件
        let patch_file = File::create(patch_file_path).with_context(|| "Create patch file failed")?;
        let mut writer = BufWriter::new(patch_file);

        // 创建编码器，将旧文件内容作为字典
        let mut encoder = Encoder::with_dictionary(&mut writer, level, &old_file_content)
            .with_context(|| "Create encoder with dictionary failed")?;

        // 从新文件读取内容并编码到补丁文件
        copy(&mut new_reader, &mut encoder).with_context(|| "Stream new file into encoder failed")?;

        // 完成编码并写入补丁文件
        encoder.finish().with_context(|| "Finish encoding failed")?;

        Ok(())
    }

    /// 应用zstd差异补丁文件
    ///
    /// # 参数
    /// - `old_file_path`: 原始文件路径
    /// - `patch_file_path`: 补丁文件路径
    /// - `new_file_path`: 输出的新文件路径
    ///
    /// # 返回值
    /// 成功时返回Ok(())，失败时返回Err
    pub fn file_patch(
        old_file_path: impl AsRef<Path>,
        patch_file_path: impl AsRef<Path>,
        new_file_path: impl AsRef<Path>,
    ) -> Result<()> {
        // 读取旧文件
        let mut old_file_content = Vec::new();
        File::open(old_file_path)?
            .read_to_end(&mut old_file_content)
            .with_context(|| "Failed to read old file")?;

        // 读取补丁文件
        let mut patch_content = Vec::new();
        File::open(patch_file_path)?
            .read_to_end(&mut patch_content)
            .with_context(|| "Failed to read patch file")?;

        // 创建新文件
        let new_file = File::create(new_file_path).with_context(|| "Create new file failed")?;
        let mut writer = BufWriter::new(new_file);

        // 创建解码器，将旧文件内容作为字典
        let mut decoder = Decoder::with_dictionary(Cursor::new(&patch_content), &old_file_content)
            .with_context(|| "Failed to create decoder with dictionary")?;

        // 从解码器读取内容并写入新文件
        copy(&mut decoder, &mut writer).with_context(|| "Stream new file into writer failed")?;

        // 完成解码并写入新文件
        writer.flush().with_context(|| "Flush writer failed")?;

        Ok(())
    }
}
