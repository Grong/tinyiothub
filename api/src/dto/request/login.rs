use serde::{Deserialize, Serialize};

// [Comment removed due to encoding issues]
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]

pub struct LoginModel {
    pub username: Option<String>,

    pub password: Option<String>,

    pub login_type: Option<i32>,

    pub client_type: Option<i32>,
}
