use serde::{Deserialize, Serialize};

/// User model representing a user entity from the database.

///

/// This struct maps to the Users table in the database with PascalCase column names.

/// Field names use snake_case in Rust code but are renamed to PascalCase for database compatibility.

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]

pub struct User {
    /// Unique identifier for the user
    pub id: String,

    /// User's display name
    pub name: String,

    /// Optional parent user ID for hierarchical relationships
    pub parent_id: Option<String>,

    /// Optional hashed password
    pub password: Option<String>,

    /// Optional phone number
    pub phone: Option<String>,

    /// Optional email address
    pub email: Option<String>,

    /// Registration date timestamp
    pub date_registered: String,

    /// Last logon date timestamp
    pub date_last_logon: String,

    /// Flag indicating if user is disabled (0 = enabled, 1 = disabled)
    pub is_disabled: i64,
}
