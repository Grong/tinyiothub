use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use include_dir::{include_dir, Dir};
use mime_guess::from_path;

// 在编译时嵌入静态文件
// 路径相对于 Cargo.toml 所在目录
static STATIC_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/wwwroot");

/// 静态文件服务处理器
pub async fn serve_static_file(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    
    // 处理根路径
    let path = if path.is_empty() || path == "/" {
        "index.html"
    } else {
        path
    };
    
    // 尝试获取文件
    match STATIC_DIR.get_file(path) {
        Some(file) => {
            let mime_type = from_path(path).first_or_octet_stream();
            
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime_type.as_ref())
                .body(Body::from(file.contents()))
                .unwrap()
        }
        None => {
            // 对于 SPA，所有未找到的路由返回 index.html
            if !path.contains('.') {
                if let Some(index) = STATIC_DIR.get_file("index.html") {
                    return Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "text/html")
                        .body(Body::from(index.contents()))
                        .unwrap();
                }
            }
            
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("404 Not Found"))
                .unwrap()
        }
    }
}

/// 检查静态文件是否已嵌入
pub fn is_static_files_embedded() -> bool {
    STATIC_DIR.entries().len() > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_files_embedded() {
        // 在开发环境可能没有构建静态文件
        println!("Static files embedded: {}", is_static_files_embedded());
        println!("Total entries: {}", STATIC_DIR.entries().len());
    }
}
