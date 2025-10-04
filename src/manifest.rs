use quick_xml::{DeError, SeError};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// 补丁清单结构体
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename = "PatchManifest")]
pub struct PatchManifest {
    /// 补丁清单唯一标识符
    #[serde(rename = "ID")]
    pub id: String,

    /// 补丁名称
    #[serde(rename = "Name")]
    pub name: String,

    /// 补丁版本
    #[serde(rename = "PatchVersion")]
    pub patch_version: String,

    /// 时间戳
    #[serde(rename = "Timestamp")]
    pub timestamp: String,

    /// 工具版本
    #[serde(rename = "ToolVersion")]
    pub tool_version: String,

    /// 作者
    #[serde(rename = "Author")]
    pub author: String,

    /// 描述
    #[serde(rename = "Description")]
    pub description: String,

    /// 基础镜像信息
    #[serde(rename = "BaseImageInfo")]
    pub base_image_info: ImageInfo,

    /// 模板镜像信息
    #[serde(rename = "TargetImageInfo")]
    pub target_image_info: ImageInfo,

    /// 操作集合
    pub operations: Operations,
}

/// 镜像信息结构体
#[derive(Debug, PartialEq, Serialize, Deserialize, Default, Clone)]
pub struct ImageInfo {
    /// 镜像索引
    #[serde(rename = "@INDEX")]
    pub index: u32,

    /// 镜像名称
    #[serde(rename = "NAME")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// 镜像显示名称
    #[serde(rename = "DISPLAYNAME")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,

    /// 镜像描述
    #[serde(rename = "DESCRIPTION")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// 镜像显示描述
    #[serde(rename = "DISPLAYDESCRIPTION")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_description: Option<String>,

    /// 镜像标志
    #[serde(rename = "FLAGS")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<String>,

    /// 目录数量
    #[serde(rename = "DIRCOUNT")]
    pub dir_count: u64,

    /// 文件数量
    #[serde(rename = "FILECOUNT")]
    pub file_count: u64,

    /// 硬链接字节数
    #[serde(rename = "HARDLINKBYTES")]
    pub hard_link_bytes: u64,

    /// 总字节数
    #[serde(rename = "TOTALBYTES")]
    pub total_bytes: u64,
}

impl ImageInfo {
    /// 从字符串解析镜像信息
    pub fn from_str(s: &str) -> Result<ImageInfo, DeError> {
        quick_xml::de::from_str::<ImageInfo>(s)
    }
}

/// 校验和结构
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Checksum {
    /// 算法类型（如SHA256）
    pub algorithm: String,

    /// 校验和值
    pub value: String,
}

/// 带路径属性的操作
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct PathOperation {
    /// 操作目标路径
    pub path: String,

    /// 大小
    pub size: u64,
}

/// 修改操作结构体（扩展PathOperation）
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ModifyOperation {
    /// 操作目标路径
    pub path: String,

    /// 大小
    pub size: u64,

    /// 存储类型（full/bsdiff）
    pub storage: String,
}

/// 操作集合
#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename = "Operations")]
pub struct Operations {
    /// 删除操作列表
    #[serde(rename = "Delete", default, skip_serializing_if = "Vec::is_empty")]
    pub deletes: Vec<PathOperation>,

    /// 新增操作列表
    #[serde(rename = "Add", default, skip_serializing_if = "Vec::is_empty")]
    pub adds: Vec<PathOperation>,

    /// 修改操作列表
    #[serde(rename = "Modify", default, skip_serializing_if = "Vec::is_empty")]
    pub modifies: Vec<ModifyOperation>,
}

impl PatchManifest {
    /// 创建新的补丁清单
    pub fn new(name: &str, description: &str, author: &str, version: &str) -> Self {
        // 生成当前时间的ISO 8601格式时间戳
        let now = SystemTime::now();
        let timestamp = now
            .duration_since(UNIX_EPOCH)
            .map(|dur| dur.as_secs())
            .map(|secs| {
                chrono::DateTime::from_timestamp(secs as i64, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            })
            .unwrap_or_else(|_| "".to_string());

        PatchManifest {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            patch_version: version.to_string(),
            timestamp,
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            author: author.to_string(),
            description: description.to_string(),
            base_image_info: Default::default(),
            target_image_info: Default::default(),
            operations: Operations::new(),
        }
    }

    /// 生成XML字符串
    pub fn to_xml(&self) -> Result<String, SeError> {
        quick_xml::se::to_string(self)
    }

    /// 从XML字符串解析
    pub fn from_xml(xml_str: &str) -> Result<Self, quick_xml::DeError> {
        quick_xml::de::from_str(xml_str)
    }

    /// 保存XML到文件
    pub fn save_to_file(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let xml_content = self.to_xml()?;
        let mut file = File::create(path)?;
        file.write_all(xml_content.as_bytes())?;
        Ok(())
    }
}

impl Operations {
    /// 创建新的操作集合
    pub fn new() -> Self {
        Operations {
            deletes: Vec::new(),
            adds: Vec::new(),
            modifies: Vec::new(),
        }
    }

    /// 添加删除操作
    pub fn add_delete(&mut self, path: String) {
        self.deletes.push(PathOperation { path, size: 0 });
    }

    /// 添加新增操作
    pub fn add_add(&mut self, path: String, size: u64) {
        self.adds.push(PathOperation { path, size });
    }

    /// 添加修改操作
    pub fn add_modify(&mut self, path: String, size: u64, storage: String) {
        self.modifies.push(ModifyOperation {
            path,
            size,
            storage,
        });
    }
}
