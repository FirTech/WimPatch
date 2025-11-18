use quick_xml::SeError;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// 补丁清单结构体
#[derive(Clone, Debug, Serialize, Deserialize)]
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

    /// 基础镜像唯一标识符
    #[serde(rename = "BaseImageGuid")]
    pub base_image_guid: String,

    /// 基础镜像信息
    #[serde(rename = "BaseImageInfo")]
    pub base_image_info: ImageInfo,

    /// 目标镜像唯一标识符
    #[serde(rename = "TargetImageGuid")]
    pub target_image_guid: String,

    /// 目标镜像信息
    #[serde(rename = "TargetImageInfo")]
    pub target_image_info: ImageInfo,

    /// 操作集合
    pub operations: Vec<Operation>,
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

/// 操作集合结构体
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename = "Operation")]
pub struct Operation {
    /// 操作类型
    #[serde(rename = "@action")]
    pub action: Action,

    /// 操作目标路径
    #[serde(rename = "Path")]
    pub path: String,

    /// 大小
    #[serde(rename = "Size", skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,

    /// 存储类型（full/bsdiff/zstdiff）
    #[serde(rename = "Storage", skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
}

/// 目录修改类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Action {
    /// 新增文件或目录
    Add,
    /// 删除文件或目录
    Delete,
    /// 修改文件
    Modify,
}

impl PatchManifest {
    /// 创建补丁清单
    ///
    /// # 参数
    ///
    /// * `name` - 补丁名称
    /// * `description` - 补丁描述
    /// * `author` - 作者
    /// * `version` - 版本
    /// * `base_image_guid` - 基础镜像唯一标识符
    /// * `base_image_info` - 基础镜像信息
    /// * `target_image_guid` - 目标镜像唯一标识符
    /// * `target_image_info` - 目标镜像信息
    /// * `operations` - 操作集合
    ///
    /// # 返回值
    ///
    /// * `PatchManifest` - 新创建的补丁清单
    pub fn new(
        name: &str,
        description: &str,
        author: &str,
        version: &str,
        base_image_guid: &str,
        base_image_info: &ImageInfo,
        target_image_guid: &str,
        target_image_info: &ImageInfo,
        operations: &[Operation],
    ) -> Self {
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
            base_image_guid: base_image_guid.to_string(),
            base_image_info: base_image_info.clone(),
            target_image_guid: target_image_guid.to_string(),
            target_image_info: target_image_info.clone(),
            operations: operations.to_vec(),
        }
    }

    /// 生成XML字符串
    pub fn to_xml(&self) -> Result<String, SeError> {
        quick_xml::se::to_string(self)
    }

    /// 从XML字符串解析
    ///
    /// # 参数
    ///
    /// * `xml_str` - 包含XML内容的字符串
    ///
    /// # 返回值
    ///
    /// * `Ok(PatchManifest)` - 如果解析成功
    /// * `Err` - 如果发生错误
    pub fn from_xml(xml_str: &str) -> Result<Self, quick_xml::DeError> {
        quick_xml::de::from_str(xml_str)
    }
}

impl ImageInfo {
    /// 从字符串解析镜像信息
    pub fn from_xml(xml_str: &str) -> Result<ImageInfo, quick_xml::DeError> {
        quick_xml::de::from_str(xml_str)
    }
}
