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
