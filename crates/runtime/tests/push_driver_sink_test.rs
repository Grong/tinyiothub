//! 推送驱动 sink → 共享管线 → 缓存更新 + PropertyChange 发布（spec §一/E3）。
//!
//! 覆盖：
//! 1) `DataServer::consume_driver_reading`（run() 中 spawn 的消费任务调用的同一函数）：
//!    stub 推送驱动的物收到 DriverReading 后，device_cache 属性值更新且 EventBus
//!    订阅者收到 ThingEventType::PropertyChange；
//! 2) `DataServer::run` 的 sink 消费任务装配：value_sink() 发送的读数被消费，
//!    shutdown 后消费任务退出（通道关闭）。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tinyiothub_core::driver::{DriverMode, DriverReading, PropertyChangeSource, ResultValue, ThingDriver, ValueSink};
use tinyiothub_core::error::Error;
use tinyiothub_core::models::event::{EventType, ThingEventType};
use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_core::models::thing_property::ThingProperty;
use tinyiothub_runtime::data_server::{DataServer, DriverCache};
use tinyiothub_runtime::driver::DriverWrapper;
use tinyiothub_runtime::event_bus::EventBus;
use tinyiothub_runtime::ports::ThingCacheSource;

struct FakeCache {
    devices: Mutex<Vec<Thing>>,
}

impl FakeCache {
    fn with_device(device: Thing) -> Self {
        Self {
            devices: Mutex::new(vec![device]),
        }
    }

    fn empty() -> Self {
        Self {
            devices: Mutex::new(vec![]),
        }
    }
}

impl ThingCacheSource for FakeCache {
    fn all(&self) -> Vec<Thing> {
        self.devices.lock().unwrap().clone()
    }

    fn get(&self, id: &str) -> Option<Thing> {
        self.devices.lock().unwrap().iter().find(|d| d.id == id).cloned()
    }

    fn get_by_name(&self, name: &str) -> Option<Thing> {
        self.devices.lock().unwrap().iter().find(|d| d.name == name).cloned()
    }

    fn insert(&self, device: Thing) {
        self.devices.lock().unwrap().push(device);
    }

    fn update(&self, device: Thing) {
        let mut devices = self.devices.lock().unwrap();
        if let Some(existing) = devices.iter_mut().find(|d| d.id == device.id) {
            *existing = device;
        }
    }

    fn remove(&self, id: &str) {
        self.devices.lock().unwrap().retain(|d| d.id != id);
    }
}

/// stub 推送驱动：mode()=Push，start_push 记录收到的 sink。
struct StubPushDriver {
    thing: Thing,
    sink_seen: Arc<Mutex<Option<ValueSink>>>,
}

impl ThingDriver for StubPushDriver {
    fn thing(&self) -> &Thing {
        &self.thing
    }

    fn thing_mut(&mut self) -> &mut Thing {
        &mut self.thing
    }

    fn read_data(&mut self) -> Result<Vec<ResultValue>, Error> {
        Ok(vec![])
    }

    fn execute_command(&mut self, _cmd: &ThingCommand) -> Result<bool, Error> {
        Ok(true)
    }

    fn mode(&self) -> DriverMode {
        DriverMode::Push
    }

    fn start_push(&mut self, sink: ValueSink, _events: Arc<dyn PropertyChangeSource>) {
        *self.sink_seen.lock().unwrap() = Some(sink);
    }
}

fn campus_thing() -> Thing {
    let mut thing = Thing {
        id: "campus-1".to_string(),
        name: "园区总表".to_string(),
        ..Thing::default()
    };
    let prop = ThingProperty {
        id: "prop-power".to_string(),
        thing_id: "campus-1".to_string(),
        name: "total_power".to_string(),
        data_type: Some("float".to_string()),
        ..ThingProperty::default()
    };
    thing.properties = Some(vec![prop]);
    thing
}

#[tokio::test]
async fn push_reading_flows_through_shared_pipeline() {
    let sink_seen: Arc<Mutex<Option<ValueSink>>> = Arc::new(Mutex::new(None));
    let mut wrapper = DriverWrapper::new(Box::new(StubPushDriver {
        thing: campus_thing(),
        sink_seen: sink_seen.clone(),
    }));
    assert_eq!(wrapper.inner_driver().mode(), DriverMode::Push);

    // trait 表面：start_push 收到运行时注入的 sink 与事件源
    let event_bus = Arc::new(EventBus::new());
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let events_source: Arc<dyn PropertyChangeSource> = event_bus.clone();
    wrapper.inner_driver_mut().start_push(tx, events_source);
    assert!(sink_seen.lock().unwrap().is_some(), "start_push 必须收到 sink");

    // 装配：driver_cache 以 thing_id 为键持有 wrapper；device_cache 持有同一物
    let driver_cache: DriverCache = moka::sync::Cache::new(100);
    driver_cache.insert("campus-1".to_string(), Arc::new(parking_lot::RwLock::new(wrapper)));
    let device_cache: Arc<dyn ThingCacheSource> = Arc::new(FakeCache::with_device(campus_thing()));

    // E5：先 subscribe 再发送
    let mut event_rx = event_bus.subscribe();

    DataServer::consume_driver_reading(
        DriverReading {
            thing_id: "campus-1".to_string(),
            values: vec![ResultValue::new(
                "total_power".to_string(),
                "float".to_string(),
                Some("42".to_string()),
            )],
        },
        &driver_cache,
        &device_cache,
        event_bus.clone(),
    )
    .await;

    // 1) 缓存先于发布：consume 返回时 device_cache 必已更新
    let cached = device_cache.get("campus-1").expect("cached thing");
    let prop = &cached.properties.as_ref().unwrap()[0];
    assert_eq!(prop.current_value.as_deref(), Some("42"), "device_cache 属性值必须更新");

    // 2) 订阅者收到恰好 1 条 PropertyChange，metadata 携带新值
    let event = tokio::time::timeout(Duration::from_secs(2), event_rx.recv())
        .await
        .expect("必须在 2s 内收到事件")
        .expect("事件通道必须打开");
    assert!(
        matches!(event.event_type(), EventType::Device(ThingEventType::PropertyChange)),
        "必须是 PropertyChange 事件, got {:?}",
        event.event_type()
    );
    assert_eq!(event.source().thing_id(), Some("campus-1"));
    assert_eq!(
        event.content().metadata().get("value"),
        Some(&serde_json::Value::String("42".to_string())),
        "metadata.value 必须是新值"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(200), event_rx.recv())
            .await
            .is_err(),
        "只允许 1 条事件"
    );

    // 3) 未知物的读数被丢弃（不 panic、缓存不变）
    DataServer::consume_driver_reading(
        DriverReading {
            thing_id: "ghost".to_string(),
            values: vec![ResultValue::new(
                "total_power".to_string(),
                "float".to_string(),
                Some("1".to_string()),
            )],
        },
        &driver_cache,
        &device_cache,
        event_bus.clone(),
    )
    .await;
    assert!(device_cache.get("ghost").is_none());
}

/// X4：无 wrapper 的物（空间物/无驱动物）→ device_cache 回退写值（spec §一）。
#[tokio::test]
async fn sink_reading_for_thing_without_wrapper_falls_back_to_cache() {
    let mut spatial = Thing {
        id: "spatial-1".to_string(),
        name: "中庭空间".to_string(),
        ..Thing::default()
    };
    let prop = ThingProperty {
        id: "prop-spatial-power".to_string(),
        thing_id: "spatial-1".to_string(),
        name: "total_power".to_string(),
        data_type: Some("float".to_string()),
        ..ThingProperty::default()
    };
    spatial.properties = Some(vec![prop]);

    // 装配：device_cache 有物，driver_cache 为空（无 wrapper）
    let driver_cache: DriverCache = moka::sync::Cache::new(100);
    let device_cache: Arc<dyn ThingCacheSource> = Arc::new(FakeCache::with_device(spatial));
    let event_bus = Arc::new(EventBus::new());

    // E5：先确认订阅建立再发送
    let baseline = event_bus.subscriber_count();
    let mut event_rx = event_bus.subscribe();
    assert_eq!(event_bus.subscriber_count(), baseline + 1, "订阅必须先于发送建立");

    DataServer::consume_driver_reading(
        DriverReading {
            thing_id: "spatial-1".to_string(),
            values: vec![ResultValue::new(
                "total_power".to_string(),
                "float".to_string(),
                Some("42".to_string()),
            )],
        },
        &driver_cache,
        &device_cache,
        event_bus.clone(),
    )
    .await;

    // 1) 缓存先于发布：consume 返回时 device_cache 必已更新
    let cached = device_cache.get("spatial-1").expect("cached thing");
    let prop = &cached.properties.as_ref().unwrap()[0];
    assert_eq!(
        prop.current_value.as_deref(),
        Some("42"),
        "回退路径必须更新 device_cache 属性值"
    );

    // 2) 订阅者收到恰好 1 条 PropertyChange
    let event = tokio::time::timeout(Duration::from_secs(2), event_rx.recv())
        .await
        .expect("必须在 2s 内收到事件")
        .expect("事件通道必须打开");
    assert!(
        matches!(event.event_type(), EventType::Device(ThingEventType::PropertyChange)),
        "必须是 PropertyChange 事件, got {:?}",
        event.event_type()
    );
    assert_eq!(event.source().thing_id(), Some("spatial-1"));
    assert!(
        tokio::time::timeout(Duration::from_millis(200), event_rx.recv())
            .await
            .is_err(),
        "只允许 1 条事件"
    );
}

/// X4 两路径并存：有 wrapper 的物走 wrapper 路径，无 wrapper 的物走 device_cache 回退。
#[tokio::test]
async fn wrapper_path_and_cache_fallback_coexist() {
    let sink_seen: Arc<Mutex<Option<ValueSink>>> = Arc::new(Mutex::new(None));
    let wrapper = DriverWrapper::new(Box::new(StubPushDriver {
        thing: campus_thing(),
        sink_seen: sink_seen.clone(),
    }));
    let driver_cache: DriverCache = moka::sync::Cache::new(100);
    driver_cache.insert("campus-1".to_string(), Arc::new(parking_lot::RwLock::new(wrapper)));

    let mut spatial = Thing {
        id: "spatial-1".to_string(),
        name: "中庭空间".to_string(),
        ..Thing::default()
    };
    spatial.properties = Some(vec![ThingProperty {
        id: "prop-spatial-power".to_string(),
        thing_id: "spatial-1".to_string(),
        name: "total_power".to_string(),
        data_type: Some("float".to_string()),
        ..ThingProperty::default()
    }]);

    let device_cache: Arc<dyn ThingCacheSource> = Arc::new(FakeCache::with_device(campus_thing()));
    device_cache.insert(spatial);
    let event_bus = Arc::new(EventBus::new());
    let baseline = event_bus.subscriber_count();
    let mut event_rx = event_bus.subscribe();
    assert_eq!(event_bus.subscriber_count(), baseline + 1, "订阅必须先于发送建立");

    for thing_id in ["campus-1", "spatial-1"] {
        DataServer::consume_driver_reading(
            DriverReading {
                thing_id: thing_id.to_string(),
                values: vec![ResultValue::new(
                    "total_power".to_string(),
                    "float".to_string(),
                    Some("7".to_string()),
                )],
            },
            &driver_cache,
            &device_cache,
            event_bus.clone(),
        )
        .await;
    }

    // 两条路径都写缓存
    for thing_id in ["campus-1", "spatial-1"] {
        let cached = device_cache.get(thing_id).expect("cached thing");
        let prop = &cached.properties.as_ref().unwrap()[0];
        assert_eq!(
            prop.current_value.as_deref(),
            Some("7"),
            "{thing_id} 属性值必须更新"
        );
    }
    // 两条路径各发 1 条 PropertyChange
    let mut seen = std::collections::HashSet::new();
    for _ in 0..2 {
        let event = tokio::time::timeout(Duration::from_secs(2), event_rx.recv())
            .await
            .expect("必须在 2s 内收到事件")
            .expect("事件通道必须打开");
        assert!(matches!(
            event.event_type(),
            EventType::Device(ThingEventType::PropertyChange)
        ));
        seen.insert(event.source().thing_id().unwrap().to_string());
    }
    assert_eq!(seen.len(), 2, "campus-1 与 spatial-1 必须各发 1 条事件");
}

/// 回退也 miss（物已删的迟到读数）→ 丢弃，不 panic、不发布事件。
#[tokio::test]
async fn reading_for_thing_missing_everywhere_is_dropped() {
    let driver_cache: DriverCache = moka::sync::Cache::new(100);
    let device_cache: Arc<dyn ThingCacheSource> = Arc::new(FakeCache::empty());
    let event_bus = Arc::new(EventBus::new());
    let baseline = event_bus.subscriber_count();
    let mut event_rx = event_bus.subscribe();
    assert_eq!(event_bus.subscriber_count(), baseline + 1, "订阅必须先于发送建立");

    DataServer::consume_driver_reading(
        DriverReading {
            thing_id: "ghost".to_string(),
            values: vec![ResultValue::new(
                "total_power".to_string(),
                "float".to_string(),
                Some("1".to_string()),
            )],
        },
        &driver_cache,
        &device_cache,
        event_bus.clone(),
    )
    .await;

    assert!(device_cache.get("ghost").is_none());
    assert!(
        tokio::time::timeout(Duration::from_millis(200), event_rx.recv())
            .await
            .is_err(),
        "双 miss 的读数不得发布任何事件"
    );
}

#[tokio::test]
async fn run_spawns_sink_consumer_and_shutdown_stops_it() {
    let device_cache: Arc<dyn ThingCacheSource> = Arc::new(FakeCache::empty());
    let event_bus = Arc::new(EventBus::new());
    let server = DataServer::new(device_cache, event_bus);

    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
    server.run(shutdown_rx).await.expect("run must succeed");

    // 消费任务已装配：value_sink() 注入的读数有接收方（未知物被消费任务丢弃，不 panic）
    let sink = server.value_sink();
    sink.send(DriverReading {
        thing_id: "nobody".to_string(),
        values: vec![],
    })
    .await
    .expect("消费任务必须持有接收端");

    // shutdown 后消费任务退出并丢弃接收端 → 通道关闭 → send 失败
    shutdown_tx.send(()).expect("shutdown broadcast");
    let mut closed = false;
    for _ in 0..40 {
        if sink
            .send(DriverReading {
                thing_id: "nobody".to_string(),
                values: vec![],
            })
            .await
            .is_err()
        {
            closed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(closed, "shutdown 后 sink 消费任务必须退出（通道关闭）");
}
