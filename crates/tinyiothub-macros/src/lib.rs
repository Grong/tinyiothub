use proc_macro::TokenStream;
use quote::quote;
use syn::{Attribute, DeriveInput, Lit, Meta, NestedMeta, Token, parse_macro_input, punctuated::Punctuated};

#[proc_macro_derive(EdgeEvent)]
pub fn derive_event(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    let expanded = quote! {
        impl Event for #name {
            fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
                self.timestamp
            }
        }
    };

    TokenStream::from(expanded)
}

/// 驱动选项结构
#[derive(Debug, Clone)]
struct DriverOption {
    label: String,
    name: String,
    default_value: String,
    option_type: String,
    required: bool,
}

/// 解析驱动属性
fn parse_driver_attributes(
    attrs: &[Attribute],
) -> Result<(String, String, Option<String>, Vec<DriverOption>), syn::Error> {
    let mut name = None;
    let mut version = None;
    let mut description = None;
    let mut options = Vec::new();

    for attr in attrs {
        if attr.path.is_ident("driver") {
            if let Meta::List(meta_list) = attr.parse_meta()? {
                for nested in meta_list.nested {
                    match nested {
                        NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("name") => {
                            if let Lit::Str(lit_str) = nv.lit {
                                name = Some(lit_str.value());
                            }
                        }
                        NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("version") => {
                            if let Lit::Str(lit_str) = nv.lit {
                                version = Some(lit_str.value());
                            }
                        }
                        NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("description") => {
                            if let Lit::Str(lit_str) = nv.lit {
                                description = Some(lit_str.value());
                            }
                        }
                        _ => {}
                    }
                }
            }
        } else if attr.path.is_ident("driver_option")
            && let Meta::List(meta_list) = attr.parse_meta()?
        {
            let mut opt_label = None;
            let mut opt_name = None;
            let mut opt_default = None;
            let mut opt_type = None;
            let mut opt_required = false;

            for nested in meta_list.nested {
                match nested {
                    NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("label") => {
                        if let Lit::Str(lit_str) = nv.lit {
                            opt_label = Some(lit_str.value());
                        }
                    }
                    NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("name") => {
                        if let Lit::Str(lit_str) = nv.lit {
                            opt_name = Some(lit_str.value());
                        }
                    }
                    NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("default") => {
                        if let Lit::Str(lit_str) = nv.lit {
                            opt_default = Some(lit_str.value());
                        }
                    }
                    NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("option_type") => {
                        if let Lit::Str(lit_str) = nv.lit {
                            opt_type = Some(lit_str.value());
                        }
                    }
                    NestedMeta::Meta(Meta::NameValue(nv)) if nv.path.is_ident("required") => {
                        if let Lit::Bool(lit_bool) = nv.lit {
                            opt_required = lit_bool.value;
                        }
                    }
                    _ => {}
                }
            }

            if let (Some(label), Some(name), Some(default), Some(option_type)) =
                (opt_label, opt_name, opt_default, opt_type)
            {
                options.push(DriverOption {
                    label,
                    name,
                    default_value: default,
                    option_type,
                    required: opt_required,
                });
            }
        }
    }

    let name = name.ok_or_else(|| syn::Error::new_spanned(&attrs[0], "Missing driver name"))?;
    let version = version.ok_or_else(|| syn::Error::new_spanned(&attrs[0], "Missing driver version"))?;

    Ok((name, version, description, options))
}

/// DeviceDriver derive 宏
///
/// 使用方式：
/// ```ignore
/// #[derive(DeviceDriver)]
/// #[driver(name = "SimulatedDriver", version = "1.0.0", description = "Simulated Device Driver")]
/// #[driver_option(label = "Refresh Interval", name = "interval", default = "1000", option_type = "number", required = true)]
/// pub struct SimulatedDriver { ... }
/// ```
///
/// 宏会自动生成：
/// 1. get_driver_info() 方法 - 返回驱动信息
/// 2. get_default_config() 方法 - 返回默认配置
/// 3. default_config() 的 trait 实现 - 自动调用 get_default_config()
#[proc_macro_derive(DeviceDriver, attributes(driver, driver_option))]
pub fn derive_device_driver(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let (driver_name, version, description, options) = match parse_driver_attributes(&input.attrs) {
        Ok(attrs) => attrs,
        Err(e) => return e.to_compile_error().into(),
    };

    // 生成选项代码
    let options_code = options.iter().map(|opt| {
        let label = &opt.label;
        let name = &opt.name;
        let default_value = &opt.default_value;
        let option_type = &opt.option_type;
        let required = opt.required;

        quote! {
            tinyiothub_core::models::component::ComponentOption::new(
                #label.to_string(),
                #name.to_string(),
                #default_value.to_string(),
                #option_type.to_string(),
                #required,
            )
        }
    });

    let description_code = if let Some(desc) = description {
        quote! { Some(#desc.to_string()) }
    } else {
        quote! { None }
    };

    let class_name = format!("tinyiothub::domain::device::driver::drivers::{}", name);

    // 生成默认配置代码
    let default_config_entries = options.iter().map(|opt| {
        let name = &opt.name;
        let default_value = &opt.default_value;

        quote! {
            config.insert(#name.to_string(), #default_value.to_string());
        }
    });

    let expanded = quote! {
        impl #name {
            pub fn get_driver_info() -> tinyiothub_core::models::component::Component {
                let opts = vec![
                    #(#options_code),*
                ];

                tinyiothub_core::models::component::Component::new(
                    tinyiothub_core::models::component::CreateComponentRequest {
                        name: #driver_name.to_string(),
                        version: #version.to_string(),
                        class_name: #class_name.to_string(),
                        device_num: Some(0),
                        description: #description_code,
                        options_descriptors: opts,
                        location: None,
                    }
                )
            }

            /// 自动生成的默认配置（基于 driver_option 宏）
            pub fn get_default_config() -> std::collections::HashMap<String, String> {
                let mut config = std::collections::HashMap::new();
                #(#default_config_entries)*
                config
            }
        }
    };

    TokenStream::from(expanded)
}

/// 驱动注册宏（已废弃，请使用新的插件系统）
///
/// 使用方式：
/// ```ignore
/// register_drivers! {
///     SimulatedDriver,
///     ModbusDriver,
///     OnvifDriver
/// }
/// ```
#[proc_macro]
pub fn register_drivers(input: TokenStream) -> TokenStream {
    let drivers = parse_macro_input!(input with Punctuated::<syn::Ident, Token![,]>::parse_terminated);

    let registry_entries = drivers.iter().map(|driver| {
        quote! {
            {
                let info = #driver::get_driver_info();
                registry.insert(
                    info.name.clone(),
                    Box::new(|device| Box::new(#driver::new(device)) as Box<dyn DeviceDriver>)
                );
            }
        }
    });

    let driver_list_entries = drivers.iter().map(|driver| {
        quote! { #driver::get_driver_info() }
    });

    let expanded = quote! {
        /// 驱动工厂函数类型
        type DriverFactory = Box<dyn Fn(Device) -> Box<dyn DeviceDriver> + Send + Sync>;

        /// 驱动注册表
        static DRIVER_REGISTRY: std::sync::LazyLock<std::collections::HashMap<String, DriverFactory>> = std::sync::LazyLock::new(|| {
            let mut registry: std::collections::HashMap<String, DriverFactory> = std::collections::HashMap::new();
            #(#registry_entries)*
            registry
        });

        /// 获取驱动信息列表
        pub fn get_driver_list() -> Vec<tinyiothub_core::models::component::Component> {
            vec![
                #(#driver_list_entries),*
            ]
        }

        /// 根据驱动名称创建驱动实例
        pub fn create_driver_by_name(
            driver_name: &str,
            device: &Device,
        ) -> Result<Box<dyn DeviceDriver>, tinyiothub_core::error::Error> {
            tracing::debug!("Creating driver with name: {}", driver_name);

            if let Some(factory) = DRIVER_REGISTRY.get(driver_name) {
                let driver = factory(device.clone());
                tracing::info!("Successfully created driver: {}", driver_name);
                Ok(driver)
            } else {
                tracing::error!("Unknown driver name: {}", driver_name);
                tracing::debug!("Available drivers: {:?}", DRIVER_REGISTRY.keys().collect::<Vec<_>>());
                Err(tinyiothub_core::error::Error::Unsupported(format!("Unknown driver name: {}", driver_name)))
            }
        }

        /// 检查驱动是否支持
        pub fn is_driver_supported(driver_name: &str) -> bool {
            DRIVER_REGISTRY.contains_key(driver_name)
        }

        /// 获取所有支持的驱动名称
        pub fn get_supported_driver_names() -> Vec<String> {
            DRIVER_REGISTRY.keys().cloned().collect()
        }
    };

    TokenStream::from(expanded)
}
