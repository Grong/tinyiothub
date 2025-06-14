use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Settings {
    pub system_features: SystemFeature,
    pub features: Feature,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemFeature {
    sso_enforced_for_signin: bool,
    sso_enforced_for_signin_protocol: String,
    enable_marketplace: bool,
    max_plugin_package_size: u64,
    enable_email_code_login: bool,
    enable_email_password_login: bool,
    enable_social_oauth_login: bool,
    is_allow_register: bool,
    is_email_setup: bool,
    is_allow_create_workspace: bool,
    branding: Branding,
}

impl Default for SystemFeature {
    fn default() -> Self {
        Self {
            sso_enforced_for_signin: false,
            sso_enforced_for_signin_protocol: "".to_string(),
            enable_marketplace: false,
            max_plugin_package_size: 0,
            enable_email_code_login: false,
            enable_email_password_login: true,
            enable_social_oauth_login: false,
            is_allow_register: false,
            is_email_setup: false,
            is_allow_create_workspace: false,
            branding: Branding::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    apps: u64, // 应用数量
    devices: u64, // 设备数量
    users: u64, // 用户数量
    workspaces: u64, // 工作空间数量
    plugins: u64, // 插件数量
    integrations: u64, // 集成数量
}

impl Default for Feature {
    fn default() -> Self {
        Self {
            apps: 10,
            devices: 20,
            users: 5,
            workspaces: 1,
            plugins: 20,
            integrations: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branding {
    enabled : bool ,
    application_title: String,
    login_page_logo: String,
    workspace_logo: String,
    favicon: String,
}

impl Default for Branding {
    fn default() -> Self {
        Self {
            enabled: false,
            application_title: "".to_string(),
            login_page_logo: "".to_string(),
            workspace_logo: "".to_string(),
            favicon: "".to_string(),
        }
    }
}

impl Settings {
    pub fn from_json(value: &serde_json::Value) -> Result<Self, serde_json::Error> {
        Ok(serde_json::from_value(value.clone())?)
    }

    pub fn from_json_or_default(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }
}