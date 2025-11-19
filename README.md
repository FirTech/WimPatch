# WimPatch 🛠️

[简体中文](README.zh.md) [English](README.md)

WimPatch is an efficient Windows Imaging Format (WIM) patching tool for creating and applying incremental patches for
WIM files, enabling efficient and secure management of WIM file incremental updates.

## Features ✨

- **Patch Creation** 📦: Compare two WIM image files and generate incremental patches
- **Patch Application** 🚀: Apply generated patches to base image files to restore updated images
- **Patch Merging** 🧩: Merge multiple incremental patches into a comprehensive patch
- **Patch Information** ℹ️: View detailed information of patch files, supporting XML format output
- **Diff Optimization** 🧬: Support Zstd and BSDiff differential algorithms to minimize patch package size
- **Interactive Mode** 🗣️: Core commands (`create` and `apply`) support no-parameter invocation, automatically
  entering interactive guided mode.
- **Internationalization** 🌐: Built-in support for both Chinese and English languages
- **High Performance** ⚡: Developed in Rust language, ensuring memory safety and high runtime efficiency and reliability

## Usage Instructions

WimPatch is a command-line tool that provides the following subcommands:

### Create Patch 📦

Create an incremental patch file for applying an updated WIM file to a base WIM file.

> 💡 Interactive Mode (Interactive Mode)
>
> If no parameters are provided, directly run `WimPatch.exe create`, the program will automatically enter interactive
> guided mode, helping you complete all required parameter inputs and validations through clear prompt steps.

```bash
WimPatch.exe create --base <base WIM file> --target <updated WIM file> --out <output patch file> --version <version number> [options]
```

- The `--version` parameter must be specified (following the [SemVer](https://semver.org/) specification). This
  parameter determines the application order of chained patches. **It is strongly recommended to increment the version
  number each time a new patch is generated.**
- If no index is specified, all images are applied by default.
- **Files with spaces in their paths need to be enclosed in double quotes.**
- **File difference data in the patch package is compressed using the `Zstd` algorithm by default to achieve the best
  balance between compression efficiency and speed.**
- **When using `--storage bsdiff`, the `--preset` parameter will be ignored, as the BSDiff storage type does not support
  compression algorithms.**

**Parameter Description**:

| Parameter        | Short | Description                                                                                                                                                                                                                                                                                                                                            | Default Value  |
|------------------|-------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|----------------|
| `--base`         | `-b`  | Base WIM file path, specifies the base WIM file used to create the incremental patch, usually the unupdated WIM file.                                                                                                                                                                                                                                  | Required       |
| `--target`       | `-t`  | Updated WIM file path, specifies the WIM file containing updated content, used to compare with the base WIM file to generate incremental patches.                                                                                                                                                                                                      | Required       |
| `--out`          | `-o`  | Output patch file path, specifies the storage location of the generated incremental patch file.                                                                                                                                                                                                                                                        | Required       |
| `--version`      | `-v`  | Patch file version, must be a valid [SemVer](https://semver.org/) format (e.g., 1.0.0). This parameter determines the application order of chained patches.                                                                                                                                                                                            | Required       |
| `--author`       | `-a`  | Patch file author, used to identify the user or team that created the patch.                                                                                                                                                                                                                                                                           | `unknown`      |
| `--name`         | `-n`  | Patch file name, used to identify the patch file, automatically generated by default (format: base-image-name-patch-vversion).                                                                                                                                                                                                                         | Auto-generated |
| `--description`  | `-d`  | Patch file description, used to explain the purpose and impact of the patch.                                                                                                                                                                                                                                                                           | None           |
| `--index`        | `-i`  | Image index in WIM file (when this parameter is specified, it will be applied to both base and target files; mutually exclusive with parameters that specify base/target indexes individually).                                                                                                                                                        | -              |
| `--base-index`   | N/A   | Image index in base WIM file (must be specified together with `--target-index`; mutually exclusive with `--index`).                                                                                                                                                                                                                                    | -              |
| `--target-index` | N/A   | Image index in updated WIM file (must be specified together with `--base-index`; mutually exclusive with `--index`).                                                                                                                                                                                                                                   | -              |
| `--compress`     | `-c`  | Patch WIM file compression algorithm: `None`, `Xpress`, `Lzx`.                                                                                                                                                                                                                                                                                         | `Lzx`          |
| `--storage`      | `-s`  | Patch file storage type:<br>• **Full**: Full storage, fast but large files<br>• **Zstd**: Zstd algorithm differential storage, balanced size and speed<br>• **Bsdiff**: Bsdiff algorithm differential storage, smallest files but slowest                                                                                                              | `Zstd`         |
| `--preset`       | `-p`  | Compression preset level:<br>• **Fast**: Fast compression, fast processing but lower compression ratio<br>• **Medium**: Medium compression, balanced speed and compression ratio<br>• **Best**: Best compression, high compression ratio but slower processing<br>• **Extreme**: Extreme compression, highest compression ratio but slowest processing | `Medium`       |
| `--exclude`      | `-e`  | File paths to exclude from the patch file, can specify multiple parameters.                                                                                                                                                                                                                                                                            | None           |

**Example**:

```bash
WimPatch.exe create -b "D:\base-v1.0.0.wim" -t "D:\base-v1.1.0.wim" -o "D:\base-patch-v1.1.0.wim" -v 1.1.0 -a "FirTech" -n "1.0.0(patch01)" -d "Update system default configuration files and wallpaper resources. Adjust the default timer settings for hibernation mode."
```

### Apply Patch 🚀

Apply a patch to a base WIM file to generate an updated WIM file.

> 💡 Interactive Mode (Interactive Mode)
>
> If no parameters are provided, directly run `WimPatch.exe apply`, the program will automatically enter interactive
> guided mode, helping you complete all required parameter inputs and validations through clear prompt steps.

```bash
WimPatch.exe apply --base <base WIM file> --patch <patch file> --target <target WIM file> [options]
```

- If no index is specified, all images are applied by default.

**Parameter Description**:

| Parameter   | Short | Description                                                                                                                                         | Default Value |
|-------------|-------|-----------------------------------------------------------------------------------------------------------------------------------------------------|---------------|
| `--base`    | `-b`  | Original WIM image file path                                                                                                                        | Required      |
| `--patch`   | `-p`  | Patch file path                                                                                                                                     | Required      |
| `--target`  | `-t`  | Output image path after applying the patch                                                                                                          | Required      |
| `--index`   | `-i`  | Target image index in base WIM file (only applies the patch to this index. If not specified, it will try to match all volumes in the patch package) | Match all     |
| `--exclude` | `-e`  | File paths to exclude from the patch file (can specify multiple)                                                                                    | None          |
| `--force`   | `-f`  | Force apply patch, skip content verification of base volume. **Warning: May cause image corruption.**                                               | None          |

**Example**:

```bash
WimPatch.exe apply -b "D:\base-v1.0.0.wim" -p "D:\base-patch-v1.1.0.wim" -t "D:\target-v1.1.0.wim"
```

### Merge Patches 🧩

Merge multiple incremental patch files into a comprehensive patch file.

```bash
WimPatch.exe merge <patch file 1> <patch file 2> ... --out <output patch file>
```

**Parameter Description**:

| Parameter | Short | Description                   | Default Value |
|-----------|-------|-------------------------------|---------------|
| `--out`   | `-o`  | Output merged patch file path | Required      |

**Example**:

```bash
WimPatch.exe merge "D:\base-patch-v1.1.0.wim" "D:\base-patch-v1.2.0.wim" -o "D:\base-patch-v1.2.0-merge.wim"
```

### View Patch Information ℹ️

Display detailed information about a patch file.

- Shows patch file version, author, name, description, etc.
- Optionally output detailed information in XML format

```bash
WimPatch.exe info <patch file> [options]
```

**Parameter Description**:
| Parameter | Short | Description | Default Value |
|-----------|-------|--------------------------------------|---------------|
| `--xml`   | `-x`  | Output patch information in XML format | None |

**Example**:

```bash
WimPatch.exe info "D:\base-patch-v1.1.0.wim"
WimPatch.exe info "D:\base-patch-v1.1.0.wim" --xml
```

### Cleanup Mount Points 🧹

Clean up invalid WIM mount points.

```bash
WimPatch.exe clean
```

### Global Options ⚙️

| Parameter       | Short | Description                                                         | Default Value         |
|-----------------|-------|---------------------------------------------------------------------|-----------------------|
| `--buffer-size` | N/A   | Specify buffer size (in bytes)                                      | 65536                 |
| `--debug`       | N/A   | Debug mode, output debug information to console                     | None                  |
| `--language`    | N/A   | Set program language (`En`, `zh-cn`, `zh-tw`, `ja-jp`)              | Auto-detect           |
| `--scratchdir`  | N/A   | Specify scratch directory path for temporary files and mount points | System temp directory |

## Technical Notes 🔍

### Patch Creation/Application Working Principle ⚙️

WimPatch uses Windows Imaging API (WIMGAPI) to process WIM files.

#### Patch Creation Process:

1. **Mount Base Image:** Mount the specified image volume (read-only) from the base WIM file.
2. **Mount Target Image:** Mount the specified image volume (read-only) from the updated WIM file.
3. **Compare File Differences:** Traverse the two mount points, compare file content, attributes, and metadata, and call
   the Zstd/BSDiff algorithm to calculate the file-level binary differences (Delta).
4. **Generate Patch Data:** Compress all difference data and metadata generated in step 3 and write it to the output
   patch file (`.wim`).
5. **Unmount and Cleanup:** Unmount and clean up the mount points of the base and target images.

#### Applying a Patch Process:

1. **Prepare a Writable Baseline:** Copy the target image from the original base WIM file to a temporary directory and
   mount it in read-write mode.
2. **Mount the Patch:** Mount the patch file to be applied (read-only).
3. **Apply Differences:** Iterate through the data in the patch file and apply the patch differences to the writable
   base image mounted in step 1.
4. **Commit Changes:** Commit all applied changes and file differences to the temporary WIM file corresponding to the
   mount point.
5. **Unmount and Clean Up:** Unmount the temporary copy of the base image and the mount point of the patch file.
6. **Export the Target WIM:** Export the temporary WIM file (i.e., the patched image) after the commit in step 4 to the
   user-specified target WIM file location.

### Dependencies 📚

WimPatch uses multiple Rust libraries to implement its functionality:

- `clap`: Command-line argument parsing
- `zstd`: Zstd compression algorithm implementation
- `bsdiff`: BSDiff differential algorithm implementation
- `quick-xml`: XML parsing and generation
- `rust-i18n`: Internationalization support
- Other dependencies can be found in the `Cargo.toml` file

## Notes ⚠️

1. ⚡ Operating WIM files may require administrator privileges
2. 💾 When processing large WIM files, ensure sufficient disk space and memory
3. 💡 Command line environment: Please start the program through the command line (CMD/PowerShell). When running directly
   by double-clicking from Explorer, the program will automatically exit

## License 📝

[Apache License 2.0](LICENSE)
