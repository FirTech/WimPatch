# WimPatch

[简体中文](README.zh.md) [English](README.md)

WimPatch 是一个高效的 Windows 映像文件 (WIM) 补丁工具，用于创建和应用 WIM 文件的增量补丁，大大减少文件传输和存储空间需求。

## 功能特性 ✨

- **补丁创建** 📦: 比较两个 WIM 映像文件并生成增量补丁
- **补丁应用** 🚀: 将生成的补丁应用到基础映像文件，还原为更新后的映像
- **补丁合并** 🧩: 将多个增量补丁合并为一个综合补丁
- **补丁信息** ℹ️: 查看补丁文件的详细信息，支持 XML 格式输出
- **多压缩算法** 🗜️: 支持不同的压缩算法 (None, Xpress, Lzx)
- **多存储类型** 💾: 支持多种存储方式 (Full, Zstd, Bsdiff)
- **国际化支持** 🌐: 内置中英文语言支持
- **高性能** ⚡: 使用 Rust 语言开发，确保高效率和可靠性

## 使用说明

WimPatch 是一个命令行工具，提供以下子命令：

### 创建补丁

```bash
WimPatch create --base <基础WIM文件> --updated <更新后的WIM文件> --out <输出补丁文件>  --version <版本号> [选项]
```

**参数说明**:

- `--base`, `-b`: 源镜像文件路径
- `--updated`, `-u`: 更新镜像文件路径
- `--out`, `-o`: 输出补丁文件路径
- `--index`, `-i`: WIM 文件中的镜像索引 (默认: 1)
- `--compress`, `-c`: 压缩算法 (None, Xpress, Lzx，默认: Lzx)
- `--storage`, `-s`: 补丁文件的存储类型(默认：zstd)
  - `full`: 使用原始文件尺寸
  - `zstd`: 使用`zstd`算法进行差异存储
  - `bsdiff`: 使用`bsdiff`算法进行差异存储
- `--preset`, `-p`: 压缩级别 (Fast, Medium, Best, Extreme，默认: Medium)
- `--version`, `-v`: 补丁文件版本
- `--author`, `-a`: 补丁文件作者 (默认: unknown)
- `--name`, `-n`: 补丁文件名称
- `--description`, `-d`: 补丁文件描述
- `--exclude`, `-e`: 从补丁文件中排除的文件

**示例**:

```bash
WimPatch create -b base.wim -u updated.wim -o patch.wim -v 1.0.0 -a "Author" -n "My Patch" -d "Patch description"
```

### 应用补丁

```bash
WimPatch apply --base <基础WIM文件> --patch <补丁文件> --target <目标WIM文件> [选项]
```

**参数说明**:

- `--base`, `-b`: 原始 WIM 镜像文件路径
- `--patch`, `-p`: 补丁文件路径
- `--target`, `-t`: 应用补丁后的输出镜像路径
- `--index`, `-i`: 基础 WIM 文件中的镜像索引 (默认: 1)
- `--exclude`, `-e`: 从补丁文件中排除的文件
- `--force`, `-f`: 强制应用补丁

**示例**:

```bash
WimPatch apply -b base.wim -p patch.wim -t target.wim
```

### 合并补丁

```bash
WimPatch merge <补丁文件1> <补丁文件2> ... --out <输出补丁文件>
```

**参数说明**:

- `补丁文件`: 要合并的补丁文件列表
- `--out`, `-o`: 输出补丁文件路径

**示例**:

```bash
WimPatch merge patch1.wim patch2.wim -o merged.wim
```

### 查看补丁信息

```bash
WimPatch info <补丁文件> [选项]
```

**参数说明**:

- `补丁文件`: 要查看信息的补丁文件路径
- `--xml`, `-x`: 以 XML 格式输出补丁信息

**示例**:

```bash
WimPatch info patch.wim
WimPatch info patch.wim --xml
```

### 全局选项

- `--buffer-size`: 缓冲区大小 (单位：字节，默认: 65536)
- `--debug`: 调试模式

## 技术说明

### 依赖项

WimPatch 使用多个 Rust 库来实现其功能：

- `clap`: 命令行参数解析
- `zstd`: Zstd 压缩算法实现
- `bsdiff`: BSDiff 差分算法实现
- `quick-xml`: XML 解析和生成
- `rust-i18n`: 国际化支持
- 其他依赖项可在 `Cargo.toml` 文件中查看

### WIM 文件处理

WimPatch 使用 Windows 映像 API (Wimgapi) 来处理 WIM 文件。程序会自动从嵌入式资源中提取所需的 `wimgapi.dll` 文件。

## 注意事项

1. 操作 WIM 文件可能需要管理员权限
2. 处理大型 WIM 文件时，建议确保有足够的磁盘空间和内存
3. 使用 `--storage bsdiff` 时，不建议同时使用 `--preset` 参数
4. 从资源管理器直接启动程序时，会显示命令行工具提示并在 5 秒后退出

## 许可证

[MIT License](LICENSE)
