# WimPatch

[简体中文](README.zh.md) [English](README.md)

WimPatch is an efficient Windows Imaging File (WIM) patching tool for creating and applying incremental patches to WIM files, significantly reducing file transfers and storage space requirements.

## Features ✨

- **Patch Creation** 📦: Compares two WIM image files and generates an incremental patch
- **Patch Application** 🚀: Applies the generated patch to the base image file, restoring the updated image
- **Patch Merge** 🧩: Merges multiple incremental patches into a single patch
- **Patch Information** ℹ️: View detailed information about the patch file, supporting XML output
- **Multiple Compression Algorithms** 🗜️: Supports various compression algorithms (None, Xpress, Lzx)
- **Multiple Storage Types** 💾: Supports multiple storage methods (Full, Zstd, Bsdiff)
- **Internationalization Support** 🌐: Built-in Chinese and English language support
- **High Performance** ⚡: Developed in Rust for high efficiency and reliability

## Usage Instructions

WimPatch is a command-line tool that provides the following subcommands:

### Creating a Patch

```bash
WimPatch create --base <Base WIM file> --updated <Updated WIM file> --out <Output patch file> --version <Version number> [Options]

**Parameter Description**:

- `--base`, `-b`: Source image file path
- `--updated`, `-u`: Update image file path
- `--out`, `-o`: Output patch file path
- `--index`, `-i`: Image index in the WIM file (default: 1)
- `--compress`, `-c`: Compression algorithm (None, Xpress, Lzx, default: Lzx)
- `--storage`, `-s`: Storage type for patch files (default: zstd)
- `full`: Use the original file size
- `zstd`: Use the `zstd` algorithm for differential storage
- `bsdiff`: Use the `bsdiff` algorithm for differential storage
- `--preset`, `- `-p`: Compression level (Fast, Medium, Best, Extreme, default: Medium)
- `--version`, `-v`: Patch file version
- `--author`, `-a`: Patch file author (default: unknown)
- `--name`, `-n`: Patch file name
- `--description`, `-d`: Patch file description
- `--exclude`, `-e`: Files to exclude from the patch file

**Example**:

```bash
WimPatch create -b base.wim -u updated.wim -o patch.wim -v 1.0.0 -a "Author" -n "My Patch" -d "Patch description"
```

### Applying a patch

```bash
WimPatch apply --base <base WIM file> --patch <patch file> --target <target WIM file> [options]
```

**Parameter Description**:

- `--base`, `-b`: Original WIM image file path
- `--patch`, `-p`: Patch file path
- `--target`, `-t`: Output image path after applying the patch
- `--index`, `-i`: Image index in the base WIM file (default: 1)
- `--exclude`, `-e`: Files to exclude from the patch file
- `--force`, `-f`: Force the patch to be applied

**Example**:

```bash
WimPatch apply -b base.wim -p patch.wim -t target.wim
```

### Merging patches

```bash
WimPatch merge <patch file 1> <patch file 2> ... --out <output patch file>
```

**Parameter Description**:

- `patch files`: List of patch files to merge
- `--out`, `-o`: Output patch file path

**Example**:

```bash
WimPatch merge patch1.wim patch2.wim -o merged.wim
```

### Viewing patch information

```bash
WimPatch info <patch file> [options]
```

**Parameter Description**:

- `patch file`: Path to the patch file to view information
- `--xml`, `-x`: Output patch information in XML format

**Example**:

```bash
WimPatch info patch.wim
WimPatch info patch.wim --xml
```

### Global Options

- `--buffer-size`: Buffer size (bytes, default: 65536)
- `--debug`: Debug mode

## Technical Description

### Dependencies

WimPatch uses several Rust libraries to implement its functionality:

- `clap`: Command Line Argument Parsing
- `zstd`: Zstd compression algorithm implementation
- `bsdiff`: BSDiff diffing algorithm implementation
- `quick-xml`: XML parsing and generation
- `rust-i18n`: Internationalization support
- Other dependencies can be found in the `Cargo.toml` file.

### WIM File Handling

WimPatch uses the Windows Imaging API (Wimgapi) to handle WIM files. The program automatically extracts the required `wimgapi.dll` file from the embedded resources.

## Notes

1. Operating WIM files may require administrator privileges.
2. When processing large WIM files, it is recommended to ensure sufficient disk space and memory.
3. When using `--storage bsdiff`, it is not recommended to use the `--preset` option at the same time.
4. When launching the program directly from Explorer, a command prompt will be displayed and the program will exit after 5 seconds.

## License

[MIT License](LICENSE)
