pub mod handler;
pub mod types;

// 心跳状态/配置已降为 AppState 字段（G3）——无全局态。
// init_heartbeat_state 调用点同步删除。
