use tinyiothub_core::media::{DataKind, MediaError, SourceItem};

pub fn select_model_sources<'a>(
    items: &'a [SourceItem],
    requested: &[String],
) -> Result<Vec<&'a SourceItem>, MediaError> {
    let selected: Vec<_> = items
        .iter()
        .filter(|item| requested.iter().any(|id| id == &item.source))
        .collect();
    if selected.iter().any(|item| item.data_kind == DataKind::Video) {
        return Err(MediaError::VideoIsForHumanNotModel);
    }
    Ok(selected
        .into_iter()
        .filter(|item| matches!(item.data_kind, DataKind::Detections | DataKind::Image))
        .collect())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tinyiothub_core::driver::manifest::SourceDecl;
    use tinyiothub_core::media::{DataKind, MediaError, SourceItem};

    #[test]
    fn model_reads_are_explicit_and_video_is_rejected() {
        let mut item: SourceItem = serde_json::from_value(json!({
            "v":1,"source":"front","dataKind":"video","seq":1,
            "time":"2026-10-27T10:00:00Z","data":null
        }))
        .expect("video source");
        assert!(
            matches!(super::select_model_sources(std::slice::from_ref(&item), &["other".into()]), Ok(selected) if selected.is_empty())
        );
        assert!(matches!(
            super::select_model_sources(std::slice::from_ref(&item), &["front".into()]),
            Err(MediaError::VideoIsForHumanNotModel)
        ));
        item.data_kind = DataKind::Detections;
        assert_eq!(
            super::select_model_sources(&[item], &["front".into()])
                .expect("detections")
                .len(),
            1
        );
    }

    #[test]
    fn requested_values_and_custom_sources_are_not_model_media() {
        let items: Vec<SourceItem> = serde_json::from_value(json!([
            {"v":1,"source":"temperature","dataKind":"values","seq":1,"time":"2026-10-27T10:00:00Z","data":21},
            {"v":1,"source":"lidar","dataKind":"x_lidar","seq":2,"time":"2026-10-27T10:00:00Z","data":{"points":3}},
            {"v":1,"source":"objects","dataKind":"detections","seq":3,"time":"2026-10-27T10:00:00Z","data":[]}
        ]))
        .expect("source items");
        assert_eq!(items[1].data_kind, DataKind::Custom("x_lidar".into()));
        let selected = super::select_model_sources(&items, &["temperature".into(), "lidar".into(), "objects".into()])
            .expect("selected model sources");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].source, "objects");
    }

    #[test]
    fn source_defaults_to_values_and_unknown_kind_is_preserved() {
        let old: SourceDecl =
            serde_json::from_value(json!({"id":"temperature","kind":"poll"})).expect("old declaration");
        assert_eq!(old.data_kind, DataKind::Values);
        let item: SourceItem = serde_json::from_value(json!({
            "v":1,"source":"custom","dataKind":"x_lidar","seq":4,
            "time":"2026-10-27T10:00:00Z","data":{"points":3}
        }))
        .expect("custom source");
        assert_eq!(item.data_kind, DataKind::Custom("x_lidar".into()));
        assert_eq!(item.data["points"], 3);
    }
}
