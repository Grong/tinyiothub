use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Rich text content supporting multiple media types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RichContent {
    title: String,
    elements: Vec<ContentElement>,
    metadata: HashMap<String, serde_json::Value>,
}

/// Individual content elements within rich content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentElement {
    Text {
        content: String,
        format: TextFormat,
    },
    Image {
        url: Option<String>,
        base64: Option<String>,
        alt_text: String,
        width: Option<u32>,
        height: Option<u32>,
    },
    Link {
        url: String,
        text: String,
        target: LinkTarget,
    },
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Code {
        content: String,
        language: Option<String>,
    },
}

/// Text formatting options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TextFormat {
    Plain,
    Markdown,
    Html,
}

/// Link target options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LinkTarget {
    Self_,
    Blank,
    Parent,
    Top,
}

impl RichContent {
    /// Create new rich content with title and elements
    pub fn new(title: String, elements: Vec<ContentElement>) -> Self {
        Self { title, elements, metadata: HashMap::new() }
    }

    /// Create new rich content with just a title and text
    pub fn new_text(title: String, content: String) -> Self {
        Self {
            title,
            elements: vec![ContentElement::Text { content, format: TextFormat::Plain }],
            metadata: HashMap::new(),
        }
    }

    /// Create new rich content with markdown text
    pub fn new_markdown(title: String, content: String) -> Self {
        Self {
            title,
            elements: vec![ContentElement::Text { content, format: TextFormat::Markdown }],
            metadata: HashMap::new(),
        }
    }

    /// Create new empty rich content with just a title
    pub fn new_empty(title: String) -> Self {
        Self { title, elements: Vec::new(), metadata: HashMap::new() }
    }

    /// Get the title
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Get the content elements
    pub fn elements(&self) -> &[ContentElement] {
        &self.elements
    }

    /// Get the metadata
    pub fn metadata(&self) -> &HashMap<String, serde_json::Value> {
        &self.metadata
    }

    /// Add metadata to the content
    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Add an element to the content
    pub fn add_element(mut self, element: ContentElement) -> Self {
        self.elements.push(element);
        self
    }

    /// Add a text element
    pub fn add_text(mut self, content: String, format: TextFormat) -> Self {
        self.elements.push(ContentElement::Text { content, format });
        self
    }

    /// Add a link element
    pub fn add_link(mut self, url: String, text: String, target: LinkTarget) -> Self {
        self.elements.push(ContentElement::Link { url, text, target });
        self
    }

    /// Add a code element
    pub fn add_code(mut self, content: String, language: Option<String>) -> Self {
        self.elements.push(ContentElement::Code { content, language });
        self
    }

    /// Validate content size (max 10MB as per requirements)
    pub fn validate_size(&self) -> Result<(), String> {
        let serialized = serde_json::to_string(self)
            .map_err(|e| format!("Failed to serialize content: {}", e))?;

        const MAX_SIZE: usize = 10 * 1024 * 1024; // 10MB

        if serialized.len() > MAX_SIZE {
            return Err(format!(
                "Content size {} exceeds maximum of {} bytes",
                serialized.len(),
                MAX_SIZE
            ));
        }

        Ok(())
    }

    /// Get a plain text representation of the content
    pub fn to_plain_text(&self) -> String {
        let mut result = format!("{}\n", self.title);

        for element in &self.elements {
            match element {
                ContentElement::Text { content, .. } => {
                    result.push_str(content);
                    result.push('\n');
                }
                ContentElement::Link { text, url, .. } => {
                    result.push_str(&format!("{} ({})\n", text, url));
                }
                ContentElement::Code { content, .. } => {
                    result.push_str(content);
                    result.push('\n');
                }
                ContentElement::Table { headers, rows } => {
                    result.push_str(&headers.join(" | "));
                    result.push('\n');
                    for row in rows {
                        result.push_str(&row.join(" | "));
                        result.push('\n');
                    }
                }
                ContentElement::Image { alt_text, .. } => {
                    result.push_str(&format!("[Image: {}]\n", alt_text));
                }
            }
        }

        result
    }

    /// Check if the content is empty (no elements)
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Get the number of elements
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    /// Get a preview of the content (first N characters of plain text)
    pub fn get_preview(&self, max_chars: usize) -> String {
        let plain_text = self.to_plain_text();
        if plain_text.len() <= max_chars {
            plain_text
        } else {
            format!("{}...", &plain_text[..max_chars])
        }
    }
}

impl ContentElement {
    /// Create a plain text element
    pub fn plain_text(content: String) -> Self {
        Self::Text { content, format: TextFormat::Plain }
    }

    /// Create a markdown text element
    pub fn markdown(content: String) -> Self {
        Self::Text { content, format: TextFormat::Markdown }
    }

    /// Create an HTML text element
    pub fn html(content: String) -> Self {
        Self::Text { content, format: TextFormat::Html }
    }

    /// Create a simple link element
    pub fn link(url: String, text: String) -> Self {
        Self::Link { url, text, target: LinkTarget::Self_ }
    }

    /// Create a code element
    pub fn code(content: String, language: Option<String>) -> Self {
        Self::Code { content, language }
    }
}

impl std::fmt::Display for RichContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_plain_text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_text() {
        let content = RichContent::new_text("Test Title".to_string(), "Test content".to_string());

        assert_eq!(content.title(), "Test Title");
        assert_eq!(content.elements().len(), 1);
        assert!(!content.is_empty());
        assert_eq!(content.element_count(), 1);

        match &content.elements()[0] {
            ContentElement::Text { content, format } => {
                assert_eq!(content, "Test content");
                assert!(matches!(format, TextFormat::Plain));
            }
            _ => panic!("Expected text element"),
        }
    }

    #[test]
    fn test_new_markdown() {
        let content = RichContent::new_markdown("Title".to_string(), "# Header".to_string());

        match &content.elements()[0] {
            ContentElement::Text { content, format } => {
                assert_eq!(content, "# Header");
                assert!(matches!(format, TextFormat::Markdown));
            }
            _ => panic!("Expected text element"),
        }
    }

    #[test]
    fn test_builder_pattern() {
        let content = RichContent::new_empty("Test".to_string())
            .add_text("Hello".to_string(), TextFormat::Plain)
            .add_link("https://example.com".to_string(), "Example".to_string(), LinkTarget::Blank)
            .add_code("println!(\"Hello\");".to_string(), Some("rust".to_string()))
            .with_metadata("key".to_string(), serde_json::Value::String("value".to_string()));

        assert_eq!(content.element_count(), 3);
        assert_eq!(
            content.metadata().get("key").unwrap(),
            &serde_json::Value::String("value".to_string())
        );
    }

    #[test]
    fn test_to_plain_text() {
        let content = RichContent::new_text("Title".to_string(), "Content".to_string()).add_link(
            "https://example.com".to_string(),
            "Link".to_string(),
            LinkTarget::Blank,
        );

        let plain = content.to_plain_text();
        assert!(plain.contains("Title"));
        assert!(plain.contains("Content"));
        assert!(plain.contains("Link"));
        assert!(plain.contains("https://example.com"));
    }

    #[test]
    fn test_validate_size() {
        let small_content = RichContent::new_text("Title".to_string(), "Small content".to_string());
        assert!(small_content.validate_size().is_ok());

        // Test with large content would require creating a very large string
        // For now, we just test that the validation function exists and works with small content
    }

    #[test]
    fn test_content_element_constructors() {
        let text = ContentElement::plain_text("Hello".to_string());
        let markdown = ContentElement::markdown("# Header".to_string());
        let html = ContentElement::html("<p>Hello</p>".to_string());
        let link = ContentElement::link("https://example.com".to_string(), "Example".to_string());
        let code = ContentElement::code("fn main() {}".to_string(), Some("rust".to_string()));

        match text {
            ContentElement::Text { format: TextFormat::Plain, .. } => {}
            _ => panic!("Expected plain text"),
        }

        match markdown {
            ContentElement::Text { format: TextFormat::Markdown, .. } => {}
            _ => panic!("Expected markdown text"),
        }

        match html {
            ContentElement::Text { format: TextFormat::Html, .. } => {}
            _ => panic!("Expected HTML text"),
        }

        match link {
            ContentElement::Link { target: LinkTarget::Self_, .. } => {}
            _ => panic!("Expected link with Self target"),
        }

        match code {
            ContentElement::Code { language: Some(lang), .. } => {
                assert_eq!(lang, "rust");
            }
            _ => panic!("Expected code with language"),
        }
    }
}
