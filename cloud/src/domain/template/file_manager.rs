use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json;
use tracing::{error, info, warn};

use tinyiothub_core::models::{
    device_template::CreateDeviceTemplateRequest, template_error::TemplateError,
};

/// 模板文件系统管理器 - 负责模板文件的加载和解析
#[derive(Debug)]
pub struct TemplateFileManager {
    templates_root: PathBuf,
}

impl TemplateFileManager {
    /// 创建新的模板文件管理器
    pub fn new<P: AsRef<Path>>(templates_root: P) -> Self {
        Self { templates_root: templates_root.as_ref().to_path_buf() }
    }

    /// 获取内置模板目录路径
    pub fn get_builtin_path(&self) -> PathBuf {
        self.templates_root.join("builtin")
    }

    /// 获取自定义模板目录路径
    pub fn get_custom_path(&self) -> PathBuf {
        self.templates_root.join("custom")
    }

    /// 获取模式定义目录路径
    pub fn get_schemas_path(&self) -> PathBuf {
        self.templates_root.join("schemas")
    }

    /// 确保模板目录结构存在
    pub fn ensure_directory_structure(&self) -> Result<(), TemplateError> {
        info!("确保模板目录结构存在");

        let directories = vec![
            self.get_builtin_path().join("sensors"),
            self.get_builtin_path().join("cameras"),
            self.get_builtin_path().join("controllers"),
            self.get_builtin_path().join("robots"),
            self.get_custom_path(),
            self.get_schemas_path(),
        ];

        for dir in directories {
            if !dir.exists() {
                fs::create_dir_all(&dir).map_err(|e| {
                    error!("创建目录失败: {:?}, 错误: {}", dir, e);
                    TemplateError::FileSystemError { source: e }
                })?;
                info!("创建目录: {:?}", dir);
            }
        }

        Ok(())
    }
    /// 加载内置模板文件
    pub fn load_builtin_templates(
        &self,
    ) -> Result<Vec<CreateDeviceTemplateRequest>, TemplateError> {
        info!("加载内置模板文件");

        let mut templates = Vec::new();
        let builtin_path = self.get_builtin_path();

        if !builtin_path.exists() {
            warn!("内置模板目录不存在: {:?}", builtin_path);
            return Ok(templates);
        }

        // 遍历所有子目录
        let categories = vec!["sensors", "cameras", "controllers", "robots"];

        for category in categories {
            let category_path = builtin_path.join(category);
            if !category_path.exists() {
                continue;
            }

            match self.load_templates_from_directory(&category_path) {
                Ok(mut category_templates) => {
                    info!("从分类 {} 加载了 {} 个模板", category, category_templates.len());
                    templates.append(&mut category_templates);
                }
                Err(e) => {
                    error!("加载分类 {} 的模板失败: {}", category, e);
                    // 继续加载其他分类，不因为一个分类失败而停止
                }
            }
        }

        info!("总共加载了 {} 个内置模板", templates.len());
        Ok(templates)
    }

    /// 从指定目录加载模板文件
    fn load_templates_from_directory(
        &self,
        dir_path: &Path,
    ) -> Result<Vec<CreateDeviceTemplateRequest>, TemplateError> {
        let mut templates = Vec::new();

        let entries = fs::read_dir(dir_path).map_err(|e| {
            error!("读取目录失败: {:?}, 错误: {}", dir_path, e);
            TemplateError::FileSystemError { source: e }
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| TemplateError::FileSystemError { source: e })?;
            let path = entry.path();

            // 只处理 .json 文件
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                match self.load_template_from_file(&path) {
                    Ok(template) => {
                        templates.push(template);
                    }
                    Err(e) => {
                        error!("加载模板文件失败: {:?}, 错误: {}", path, e);
                        // 继续加载其他文件，不因为一个文件失败而停止
                    }
                }
            }
        }

        Ok(templates)
    }
    /// 从文件加载单个模板
    fn load_template_from_file(
        &self,
        file_path: &Path,
    ) -> Result<CreateDeviceTemplateRequest, TemplateError> {
        // 读取文件内容
        let content = fs::read_to_string(file_path).map_err(|e| {
            error!("读取模板文件失败: {:?}, 错误: {}", file_path, e);
            TemplateError::FileSystemError { source: e }
        })?;

        // 解析 JSON
        let template: CreateDeviceTemplateRequest =
            serde_json::from_str(&content).map_err(|e| {
                error!("解析模板文件JSON失败: {:?}, 错误: {}", file_path, e);
                TemplateError::JsonFormatError {
                    message: format!("文件 {:?} JSON格式错误: {}", file_path, e),
                }
            })?;

        // 基本验证
        self.validate_template_basic(&template)?;

        Ok(template)
    }

    /// 基本模板验证
    fn validate_template_basic(
        &self,
        template: &CreateDeviceTemplateRequest,
    ) -> Result<(), TemplateError> {
        // 检查必填字段
        if template.name.is_empty() {
            return Err(TemplateError::ValidationFailed {
                errors: vec![tinyiothub_core::models::template_error::ValidationError::required_field(
                    "name",
                )],
            });
        }

        if template.category.is_empty() {
            return Err(TemplateError::ValidationFailed {
                errors: vec![tinyiothub_core::models::template_error::ValidationError::required_field(
                    "category",
                )],
            });
        }

        if template.device_type.is_empty() {
            return Err(TemplateError::ValidationFailed {
                errors: vec![tinyiothub_core::models::template_error::ValidationError::required_field(
                    "device_type",
                )],
            });
        }

        if template.display_name.is_empty() {
            return Err(TemplateError::ValidationFailed {
                errors: vec![tinyiothub_core::models::template_error::ValidationError::required_field(
                    "display_name",
                )],
            });
        }

        Ok(())
    }

    /// 保存模板到文件
    pub fn save_template_to_file(
        &self,
        template: &CreateDeviceTemplateRequest,
        file_path: &Path,
    ) -> Result<(), TemplateError> {
        info!("保存模板到文件: {:?}", file_path);

        // 确保父目录存在
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                error!("创建目录失败: {:?}, 错误: {}", parent, e);
                TemplateError::FileSystemError { source: e }
            })?;
        }

        // 序列化为格式化的 JSON
        let content = serde_json::to_string_pretty(template).map_err(|e| {
            error!("序列化模板失败: {}", e);
            TemplateError::SerializationError { source: e }
        })?;

        // 写入文件
        fs::write(file_path, content).map_err(|e| {
            error!("写入模板文件失败: {:?}, 错误: {}", file_path, e);
            TemplateError::FileSystemError { source: e }
        })?;

        info!("成功保存模板到文件: {:?}", file_path);
        Ok(())
    }
    /// 删除模板文件
    pub fn delete_template_file(&self, file_path: &Path) -> Result<(), TemplateError> {
        info!("删除模板文件: {:?}", file_path);

        if !file_path.exists() {
            warn!("模板文件不存在: {:?}", file_path);
            return Ok(());
        }

        fs::remove_file(file_path).map_err(|e| {
            error!("删除模板文件失败: {:?}, 错误: {}", file_path, e);
            TemplateError::FileSystemError { source: e }
        })?;

        info!("成功删除模板文件: {:?}", file_path);
        Ok(())
    }

    /// 获取模板文件路径
    pub fn get_template_file_path(&self, category: &str, template_name: &str) -> PathBuf {
        self.get_builtin_path().join(category).join(format!("{}.json", template_name))
    }

    /// 获取自定义模板文件路径
    pub fn get_custom_template_file_path(&self, template_name: &str) -> PathBuf {
        self.get_custom_path().join(format!("{}.json", template_name))
    }

    /// 列出指定分类的所有模板文件
    pub fn list_template_files(&self, category: &str) -> Result<Vec<PathBuf>, TemplateError> {
        let category_path = self.get_builtin_path().join(category);

        if !category_path.exists() {
            return Ok(Vec::new());
        }

        let mut template_files = Vec::new();
        let entries = fs::read_dir(&category_path).map_err(|e| {
            error!("读取分类目录失败: {:?}, 错误: {}", category_path, e);
            TemplateError::FileSystemError { source: e }
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| TemplateError::FileSystemError { source: e })?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                template_files.push(path);
            }
        }

        Ok(template_files)
    }

    /// 检查模板文件是否存在
    pub fn template_file_exists(&self, category: &str, template_name: &str) -> bool {
        let file_path = self.get_template_file_path(category, template_name);
        file_path.exists()
    }

    /// 获取模板根目录
    pub fn get_templates_root(&self) -> &PathBuf {
        &self.templates_root
    }
}
