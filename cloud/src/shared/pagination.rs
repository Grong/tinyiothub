use serde::{Deserialize, Serialize};

/// Pagination query parameters for API requests
#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "snake_case")]
pub struct PaginationQuery {
    #[serde(deserialize_with = "deserialize_optional_u32")]
    pub page: Option<u32>,
    #[serde(deserialize_with = "deserialize_optional_u32")]
    pub page_size: Option<u32>,
}

/// Custom deserializer for optional u32 from string
fn deserialize_optional_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    let opt: Option<String> = Option::deserialize(deserializer)?;
    match opt {
        Some(s) => s
            .parse::<u32>()
            .map(Some)
            .map_err(|_| D::Error::custom(format!("invalid number: {}", s))),
        None => Ok(None),
    }
}

/// Pagination parameters for API requests
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Pagination {
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
    pub total_count: u32,
}

/// Data object with pagination information
#[derive(Serialize, Deserialize, Debug)]
pub struct DataObjectWithPagination<T> {
    pub pagination: Pagination,
    pub data: Vec<T>,
}

impl<T> DataObjectWithPagination<T>
where
    T: Serialize + Clone,
{
    pub fn new(data: &[T], page: u32, page_size: u32) -> Self {
        let total_count = data.len();
        let mut tmp_page = page;
        let total_page = (total_count as f32 / page_size as f32).ceil() as u32;

        if tmp_page > total_page {
            tmp_page = total_page;
        }

        let pagination = Pagination {
            page_size,
            page: tmp_page,
            total_pages: total_page,
            total_count: total_count as u32,
        };

        if total_page == 0 {
            return DataObjectWithPagination::<T> { pagination, data: data[0..0].to_vec() };
        }

        let start = ((pagination.page - 1) * pagination.page_size) as usize;
        let mut end = start + pagination.page_size as usize;

        if end > total_count {
            end = total_count;
        }

        DataObjectWithPagination::<T> { pagination, data: data[start..end].to_vec() }
    }

    pub fn default(page: u32, page_size: u32) -> Self {
        DataObjectWithPagination::<T> {
            pagination: Pagination { page, page_size, total_pages: 0, total_count: 0 },
            data: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_query_deserialization() {
        let json = r#"{"page": "2", "page_size": "50"}"#;
        let query: PaginationQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.page, Some(2));
        assert_eq!(query.page_size, Some(50));

        let json_empty = r#"{"page": null, "page_size": null}"#;
        let query: PaginationQuery = serde_json::from_str(json_empty).unwrap();
        assert_eq!(query.page, None);
        assert_eq!(query.page_size, None);
    }

    #[test]
    fn test_pagination_query_invalid_number() {
        let json = r#"{"page": "not_a_number"}"#;
        let result: Result<PaginationQuery, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_data_object_with_pagination_basic() {
        let data: Vec<i32> = (1..=25).collect();
        let result = DataObjectWithPagination::new(&data, 1, 10);
        assert_eq!(result.pagination.page, 1);
        assert_eq!(result.pagination.page_size, 10);
        assert_eq!(result.pagination.total_pages, 3);
        assert_eq!(result.pagination.total_count, 25);
        assert_eq!(result.data.len(), 10);
        assert_eq!(result.data[0], 1);
        assert_eq!(result.data[9], 10);
    }

    #[test]
    fn test_data_object_with_pagination_second_page() {
        let data: Vec<i32> = (1..=25).collect();
        let result = DataObjectWithPagination::new(&data, 2, 10);
        assert_eq!(result.pagination.page, 2);
        assert_eq!(result.data.len(), 10);
        assert_eq!(result.data[0], 11);
        assert_eq!(result.data[9], 20);
    }

    #[test]
    fn test_data_object_with_pagination_last_partial_page() {
        let data: Vec<i32> = (1..=25).collect();
        let result = DataObjectWithPagination::new(&data, 3, 10);
        assert_eq!(result.pagination.page, 3);
        assert_eq!(result.data.len(), 5);
        assert_eq!(result.data[0], 21);
        assert_eq!(result.data[4], 25);
    }

    #[test]
    fn test_data_object_with_pagination_page_beyond_total() {
        let data: Vec<i32> = (1..=10).collect();
        let result = DataObjectWithPagination::new(&data, 100, 10);
        assert_eq!(result.pagination.page, 1);
        assert_eq!(result.pagination.total_pages, 1);
        assert_eq!(result.data.len(), 10);
    }

    #[test]
    fn test_data_object_with_pagination_empty_data() {
        let data: Vec<i32> = vec![];
        let result = DataObjectWithPagination::new(&data, 1, 10);
        assert_eq!(result.pagination.page, 0);
        assert_eq!(result.pagination.total_pages, 0);
        assert_eq!(result.pagination.total_count, 0);
        assert!(result.data.is_empty());
    }

    #[test]
    fn test_data_object_with_pagination_default() {
        let result: DataObjectWithPagination<i32> = DataObjectWithPagination::default(1, 20);
        assert_eq!(result.pagination.page, 1);
        assert_eq!(result.pagination.page_size, 20);
        assert_eq!(result.pagination.total_pages, 0);
        assert_eq!(result.pagination.total_count, 0);
        assert!(result.data.is_empty());
    }
}
