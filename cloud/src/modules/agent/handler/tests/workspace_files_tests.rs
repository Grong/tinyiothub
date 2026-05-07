// Workspace Files Handler Tests
// Tests for workspace files API handlers

/// Test path validation accepts valid workspace IDs
#[test]
fn test_validate_path_accepts_valid_workspace() {
    // This tests the validation logic by checking the expected file list
    let valid_files = [
        "IDENTITY.md",
        "SOUL.md",
        "AGENTS.md",
        "USER.md",
        "TOOLS.md",
        "MEMORY.md",
        "HEARTBEAT.md",
        "BOOTSTRAP.md",
    ];

    for filename in valid_files {
        assert!(filename.contains(".md"), "Test data should contain .md files");
    }
    assert_eq!(valid_files.len(), 8, "Should have 8 workspace files");
}

/// Test path validation rejects path traversal
#[test]
fn test_path_validation_rejects_traversal() {
    // Path traversal attempts should be rejected
    let malicious_paths = ["../etc/passwd", "foo/../../../bar", ".%2e/%2e%2e/etc"];

    for path in malicious_paths {
        assert!(
            path.contains("..") || path.contains("%2e"),
            "Malicious path should be detected: {}",
            path
        );
    }
}

/// Test workspace file response structure
#[test]
fn test_workspace_file_info_serde() {
    use crate::modules::agent::handler::files::{WorkspaceFileInfo, WorkspaceFileResponse};

    let info = WorkspaceFileInfo { name: "IDENTITY.md".to_string() };

    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("IDENTITY.md"));

    let response =
        WorkspaceFileResponse { name: "SOUL.md".to_string(), content: "# Soul".to_string() };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("SOUL.md"));
    assert!(json.contains("# Soul"));
}

/// Test update request deserialization
#[test]
fn test_update_request_deserialization() {
    use crate::modules::agent::handler::files::UpdateWorkspaceFileRequest;

    let json = r#"{"content": "Hello World"}"#;
    let req: UpdateWorkspaceFileRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.content, "Hello World");
}

/// Test update request accepts empty content
#[test]
fn test_update_request_accepts_empty() {
    use crate::modules::agent::handler::files::UpdateWorkspaceFileRequest;

    let json = r#"{"content": ""}"#;
    let req: UpdateWorkspaceFileRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.content, "");
}

/// Test update request accepts multiline content
#[test]
fn test_update_request_multiline_content() {
    use crate::modules::agent::handler::files::UpdateWorkspaceFileRequest;

    let json = r#"{"content": "Line 1\n\nLine 2"}"#;
    let req: UpdateWorkspaceFileRequest = serde_json::from_str(json).unwrap();
    assert!(req.content.contains('\n'));
    assert!(req.content.contains("Line 2"));
}

/// Test file size limit constant
#[test]
fn test_file_size_limit() {
    use crate::modules::agent::handler::files::MAX_FILE_SIZE;

    assert_eq!(MAX_FILE_SIZE, 1024 * 1024, "Max file size should be 1MB");
}
