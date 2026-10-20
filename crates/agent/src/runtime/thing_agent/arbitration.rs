//! E1 仲裁退化形态（spec §4 Hub 层）：每设备单一互斥资源，E4 声明制后细化。
//!
//! 多个执行体（chat agent、值守 run）可能同时想操作同一台设备——这里只
//! 做最退化的一层互斥：同一 thing_id 同一时刻只允许一个持有者。持有者
//! 崩溃不死锁：租约带 TTL（默认 10min，AGH 参照），过期后自动可再
//! acquire。释放走 RAII——[`Lease`] 析构即还锁（token 校验，不会误删
//! TTL 过期后被他人重 acquire 的新锁）。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;

/// 默认租约 TTL（AGH 参照 10min）——覆盖一次 run 的执行窗口；持有者
/// 崩溃后最长 10min 自动恢复可 acquire，不死锁。
pub const DEFAULT_LEASE_TTL: Duration = Duration::from_secs(10 * 60);

/// E1 仲裁退化形态（spec §4 Hub 层）：每设备单一互斥资源，E4 声明制后细化。
/// busy 拒绝必须带持有者名（人读得懂的拒绝）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("busy: {holder}")]
pub struct Busy {
    pub holder: String,
}

struct LockEntry {
    holder: String,
    /// 每把锁的唯一令牌——Lease Drop 时按令牌校验，TTL 过期后被他人重
    /// acquire 的锁不被旧 Lease 误删。
    token: u64,
    acquired_at: Instant,
    ttl: Duration,
}

impl LockEntry {
    fn alive(&self) -> bool {
        self.acquired_at.elapsed() < self.ttl
    }
}

/// 每设备互斥锁注册表。Cheap to clone —— 所有克隆共享同一张表（跨 run、
/// 跨 chat/自治工具共享同一份仲裁状态）。
#[derive(Clone, Default)]
pub struct ArbitrationRegistry {
    locks: Arc<DashMap<String, LockEntry>>,
    next_token: Arc<AtomicU64>,
}

impl ArbitrationRegistry {
    /// 尝试占有 `thing_id` 的互斥锁。空闲或前任租约已过期 → 获得
    /// [`Lease`]；否则返回 [`Busy`]（含当前持有者名）。
    pub fn try_acquire(&self, thing_id: &str, holder: String, ttl: Duration) -> Result<Lease, Busy> {
        let mint = |holder: String| {
            let token = self.next_token.fetch_add(1, Ordering::Relaxed);
            (
                LockEntry {
                    holder,
                    token,
                    acquired_at: Instant::now(),
                    ttl,
                },
                token,
            )
        };
        // entry API：检查-插入原子，两个并发 acquire 只有一个胜出。
        match self.locks.entry(thing_id.to_string()) {
            Entry::Vacant(v) => {
                let (entry, token) = mint(holder);
                v.insert(entry);
                Ok(self.lease(thing_id, token))
            }
            Entry::Occupied(mut o) => {
                let current = o.get();
                if current.alive() {
                    return Err(Busy {
                        holder: current.holder.clone(),
                    });
                }
                let (entry, token) = mint(holder);
                o.insert(entry);
                Ok(self.lease(thing_id, token))
            }
        }
    }

    /// 默认 TTL（10min）的 acquire 便捷入口。
    pub fn try_acquire_default(&self, thing_id: &str, holder: String) -> Result<Lease, Busy> {
        self.try_acquire(thing_id, holder, DEFAULT_LEASE_TTL)
    }

    fn lease(&self, thing_id: &str, token: u64) -> Lease {
        Lease {
            locks: Arc::clone(&self.locks),
            thing_id: thing_id.to_string(),
            token,
        }
    }
}

/// 互斥租约——析构即释放（RAII）。只删自己那把锁：token 不匹配（TTL
/// 过期后已被他人重 acquire）时不动表。
pub struct Lease {
    locks: Arc<DashMap<String, LockEntry>>,
    thing_id: String,
    token: u64,
}

impl std::fmt::Debug for Lease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lease")
            .field("thing_id", &self.thing_id)
            .field("token", &self.token)
            .finish()
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.locks
            .remove_if(&self.thing_id, |_, entry| entry.token == self.token);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TTL: Duration = Duration::from_secs(60);

    #[test]
    fn mutual_exclusion_per_thing() {
        let reg = ArbitrationRegistry::default();
        let _a = reg
            .try_acquire("dev_1", "run-A".into(), TTL)
            .expect("首次 acquire 必成");

        // 同设备第二个持有者 → busy，且带上当前持有者名。
        let err = reg
            .try_acquire("dev_1", "run-B".into(), TTL)
            .expect_err("同设备并发必须 busy");
        assert_eq!(err, Busy { holder: "run-A".into() });

        // 不同设备互不影响。
        reg.try_acquire("dev_2", "run-B".into(), TTL).expect("不同设备不互斥");
    }

    #[test]
    fn busy_message_carries_holder_name() {
        let reg = ArbitrationRegistry::default();
        let _a = reg
            .try_acquire("dev_1", "值守run#42".into(), TTL)
            .expect("首次 acquire 必成");
        let err = reg.try_acquire("dev_1", "chat".into(), TTL).expect_err("必 busy");
        // 人读得懂的拒绝：错误文案必须带持有者名。
        assert!(err.to_string().contains("值守run#42"), "busy 文案必须带持有者名: {err}");
    }

    #[test]
    fn reacquire_after_lease_drop() {
        let reg = ArbitrationRegistry::default();
        {
            let _a = reg
                .try_acquire("dev_1", "run-A".into(), TTL)
                .expect("首次 acquire 必成");
        } // Lease 析构 → 释放
        reg.try_acquire("dev_1", "run-B".into(), TTL)
            .expect("释放后必须可再 acquire");
    }

    #[test]
    fn ttl_expiry_allows_reacquire() {
        let reg = ArbitrationRegistry::default();
        // TTL=0 → 租约立即过期（持有者崩溃不死锁的退化验证）。
        let _dead = reg
            .try_acquire("dev_1", "crashed-run".into(), Duration::ZERO)
            .expect("首次 acquire 必成");
        // 旧 Lease 还持有（未 drop），但已过期 → 新持有者直接接管。
        reg.try_acquire("dev_1", "run-B".into(), TTL)
            .expect("TTL 过期后必须可再 acquire");
    }

    #[test]
    fn stale_lease_drop_does_not_remove_new_owner() {
        let reg = ArbitrationRegistry::default();
        let stale = reg
            .try_acquire("dev_1", "crashed-run".into(), Duration::ZERO)
            .expect("首次 acquire 必成");
        let _live = reg.try_acquire("dev_1", "run-B".into(), TTL).expect("过期接管必成");
        // 崩溃持有者姗姗来迟的 Drop —— 不得误删 run-B 的锁。
        drop(stale);
        let err = reg
            .try_acquire("dev_1", "run-C".into(), TTL)
            .expect_err("run-B 的锁必须仍在");
        assert_eq!(err.holder, "run-B");
    }

    #[test]
    fn concurrent_acquire_exactly_one_wins() {
        let reg = ArbitrationRegistry::default();
        // 屏障齐步出发 + 胜者持锁 100ms：输给真实互斥而非线程调度顺序。
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let mut handles = Vec::new();
        for i in 0..8 {
            let reg = reg.clone();
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                match reg.try_acquire("dev_1", format!("run-{i}"), TTL) {
                    Ok(lease) => {
                        std::thread::sleep(Duration::from_millis(100));
                        drop(lease);
                        true
                    }
                    Err(_) => false,
                }
            }));
        }
        let wins = handles
            .into_iter()
            .map(|h| h.join().expect("thread"))
            .filter(|won| *won)
            .count();
        assert_eq!(wins, 1, "并发 acquire 只能有一个胜出");
    }
}
