//! 可注入 [`DirectiveSink`] 桩（T14 测试用）—— 在 ThingAgentManager
//! （T15，进程内 per-workspace 注册表）就位前，让指令端点 / chat 工具
//! 可测：记录投递的信号，可配置注入错误（如队列满 Rejected）。

use std::sync::Mutex;

use crate::loop_::thing_agent::{DirectiveSink, EnqueueError, WakeSignal};

#[derive(Default)]
pub struct StubDirectiveSink {
    signals: Mutex<Vec<WakeSignal>>,
    fail_with: Mutex<Option<EnqueueError>>,
    drained: Mutex<Vec<String>>,
}

impl StubDirectiveSink {
    /// 所有投递都返回 `error` 的桩（队列满 / 去重等错误路径测试）。
    pub fn failing(error: EnqueueError) -> Self {
        Self {
            signals: Mutex::new(vec![]),
            fail_with: Mutex::new(Some(error)),
            drained: Mutex::new(vec![]),
        }
    }

    /// 已成功投递的信号（按到达顺序）。
    pub fn signals(&self) -> Vec<WakeSignal> {
        self.signals.lock().expect("signals lock").clone()
    }

    /// 已收到 drain 请求的工作区（按到达顺序，O26 测试用）。
    pub fn drained(&self) -> Vec<String> {
        self.drained.lock().expect("drained lock").clone()
    }
}

#[async_trait::async_trait]
impl DirectiveSink for StubDirectiveSink {
    fn enqueue(&self, signal: WakeSignal) -> Result<(), EnqueueError> {
        if let Some(error) = *self.fail_with.lock().expect("fail_with lock") {
            return Err(error);
        }
        self.signals.lock().expect("signals lock").push(signal);
        Ok(())
    }

    async fn drain(&self, workspace_id: &str) {
        self.drained.lock().expect("drained lock").push(workspace_id.to_string());
    }
}
