use std::sync::Arc;

use once_cell::sync::Lazy;
use tokio::sync::RwLock;

use crate::{
    domain::event::value_objects::EventLevel,
    dto::{
        entity::message::{Message, MessageDto},
        request::pagination::DataObjectWithPagination,
    },
    infrastructure::{
        config,
        event::EventBus,
        persistence::database::{self, Database},
    },
};

// 使用 tokio::sync::RwLock 替代 std::sync::Mutex 以避免死锁
// RwLock 允许多个读取者或一个写入者，更适合异步环境
static MSG: Lazy<Arc<RwLock<Vec<MessageDto>>>> = Lazy::new(|| Arc::new(RwLock::new(Vec::new())));

/// 安全地执行消息回调，使用异步锁避免死锁
async fn msg_callback<F>(func: F)
where
    F: for<'a> FnOnce(&'a mut Vec<MessageDto>) + Send,
{
    let mut messages = MSG.write().await;
    func(&mut messages);
}

/// 安全地读取消息，使用异步锁
async fn msg_read<F, R>(func: F) -> R
where
    F: FnOnce(&Vec<MessageDto>) -> R,
{
    let messages = MSG.read().await;
    func(&messages)
}

/// Add a message using the new event service system
pub async fn add_message(msg: MessageDto, db: &Database, event_bus: Option<&Arc<EventBus>>) {
    let mut message = msg.clone();
    message.id = database::generate_id();

    // Update in-memory cache for backward compatibility
    let message_clone = message.clone();
    msg_callback(|real| {
        real.retain(|m| m.child_object.is_none() || m.child_object != message_clone.child_object);
        real.push(message_clone);
    })
    .await;

    // Clean up old messages
    clear_limit_message(db).await;

    // Convert message to event and post to event service
    let event_level = match message.level {
        5 => EventLevel::Critical,
        4 => EventLevel::Error,
        3 => EventLevel::Warning,
        2 => EventLevel::Info,
        1 => EventLevel::Debug,
        _ => EventLevel::Info,
    };

    // Publish message event to event bus
    if let Some(event_bus) = event_bus {
        use crate::domain::event::{
            entities::Event as DomainEvent,
            value_objects::{
                ContentElement, DeviceEventType, EventSource, EventType, RichContent,
                SystemEventType, TextFormat,
            },
        };

        let event_type = match message.message_type.as_deref() {
            Some("device") => EventType::Device(DeviceEventType::DeviceNormal),
            Some("system") => EventType::System(SystemEventType::UserOperation),
            _ => EventType::System(SystemEventType::UserOperation),
        };

        let source = if let Some(device_id) = &message.child_object {
            EventSource::device(device_id.clone(), Some("message_server".to_string()))
        } else {
            EventSource::system("message_server".to_string(), None::<String>)
        };

        match DomainEvent::new(
            event_type,
            event_level,
            source,
            RichContent::new(
                message.title.clone(),
                vec![ContentElement::Text {
                    content: message.content.clone().unwrap_or_default(),
                    format: TextFormat::Plain,
                }],
            ),
            None,
        ) {
            Ok(event) => {
                let event_bus_clone = event_bus.clone();
                crate::utils::publish_event_safe(event_bus_clone, event).await;
            }
            Err(e) => {
                tracing::error!("Failed to create message event: {}", e);
            }
        }
    }

    // Still store in the old database for backward compatibility during transition
    match Message::add_message(
        db,
        message.level,
        message.title,
        message.content,
        message.message_type,
    )
    .await
    {
        Ok(_) => (),
        Err(e) => {
            let err = format!("Failed to add message: {},{:?}", e, msg);
            tracing::error!("{err}");
        }
    }
}

pub async fn clear_limit_message(db: &Database) {
    let limit_number: usize = config::get().mqtt.client.message_queue_size;

    msg_callback(|real| {
        // 如果消息数量超过限制，删除最旧的消息
        if real.len() > limit_number {
            let mut tmp = real.clone();
            tmp.sort_by(|a, b| a.create_date_time.cmp(&b.create_date_time));
            let len = tmp.len() - limit_number;

            // 收集要删除的消息ID
            let ids_to_remove: Vec<String> = tmp.iter().take(len).map(|m| m.id.clone()).collect();

            // 删除消息
            real.retain(|m| !ids_to_remove.contains(&m.id));
        }
    })
    .await;

    if let Err(e) = Message::clear_limit_messages(db, limit_number as i64).await {
        tracing::error!("Failed to clear limit messages: {}", e);
    }
}

pub async fn get_message(
    page: u32,
    page_size: u32,
    level: Option<String>,
    message_type: Option<String>,
) -> DataObjectWithPagination<MessageDto> {
    msg_read(|real| {
        let mut tmp = real.clone();
        tmp.retain(|m| {
            let lv = match &level {
                Some(l) => {
                    if !l.is_empty() {
                        return l.contains(&m.level.to_string());
                    }
                    true
                }
                None => true,
            };

            let tv = match &message_type {
                Some(t) => {
                    if !t.is_empty() {
                        return match &m.message_type {
                            Some(tp) => t.contains(tp),
                            None => false,
                        };
                    }
                    true
                }
                None => true,
            };
            lv && tv
        });
        DataObjectWithPagination::<MessageDto>::new(&tmp, page, page_size)
    })
    .await
}

/// Helper function to create a system message event
pub async fn add_system_message(
    title: String,
    content: String,
    level: EventLevel,
    event_bus: Option<&Arc<EventBus>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Some(event_bus) = event_bus {
        use crate::domain::event::{
            entities::Event as DomainEvent,
            value_objects::{
                ContentElement, EventSource, RichContent, SystemEventType, TextFormat,
            },
        };

        let event = DomainEvent::new_system_event(
            SystemEventType::UserOperation,
            level,
            EventSource::system("message_server".to_string(), None::<String>),
            RichContent::new(
                title,
                vec![ContentElement::Text { content, format: TextFormat::Plain }],
            ),
        )?;

        event_bus.publish(event).await?;
    } else {
        tracing::info!("System message: {} - {}", title, content);
    }
    Ok(())
}

/// Helper function to create a device message event
pub async fn add_device_message(
    device_id: String,
    title: String,
    content: String,
    level: EventLevel,
    workspace_id: Option<String>,
    event_bus: Option<&Arc<EventBus>>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(event_bus) = event_bus {
        use crate::domain::event::{
            entities::Event as DomainEvent,
            value_objects::{
                ContentElement, DeviceEventType, EventSource, RichContent, TextFormat,
            },
        };

        let event = DomainEvent::new_device_event(
            DeviceEventType::DeviceNormal,
            level,
            EventSource::device(device_id.clone(), Some("message_server".to_string())),
            RichContent::new(
                title,
                vec![ContentElement::Text { content, format: TextFormat::Plain }],
            ),
            workspace_id,
        )?;

        event_bus.publish(event).await?;
    } else {
        tracing::info!("Device message event: {} - {}", device_id, title);
    }
    Ok(())
}
