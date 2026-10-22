//! E4c 多资源 claim 注册表（spec §2.1）：把 E1 的每设备单互斥扩展为按
//! `(thing, resource)` 的声明式仲裁。
//!
//! 多个执行体（chat agent、值守 run、composite 步骤）可能同时想操作同一台
//! 设备的同一份能力——调用方按工具声明给出 [`ResourceSpec`]，注册表排序后
//! all-or-nothing 地 claim 全部资源：任一 busy 则释放已获 claim 并拒绝。
//! 未声明 resources 的变更类工具走兼容入口 [`ArbitrationRegistry::try_acquire`]，
//! claim 合成的 [`DEVICE_RESOURCE`] exclusive 资源，保持 E1 每设备互斥语义。
//!
//! 持有者崩溃不死锁：每个 claim 带 TTL（默认 10min，AGH 参照），过期后自动
//! 可再 claim。释放走 RAII——[`Lease`] 析构即还全部 claim（token 校验，不会
//! 误删 TTL 过期后被他人重 claim 的新持有者）。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;

/// 默认租约 TTL（AGH 参照 10min）——覆盖一次 run 的执行窗口；持有者
/// 崩溃后最长 10min 自动恢复可 claim，不死锁。
pub const DEFAULT_LEASE_TTL: Duration = Duration::from_secs(10 * 60);

/// E2 stop 闸门默认 TTL：阻断该设备后续 call/set 的窗口。
pub const DEFAULT_STOP_TTL: Duration = Duration::from_secs(60);

/// 未声明 resources 的变更类工具所 claim 的合成资源 id——exclusive，
/// 等价于 E1 的每设备互斥。
pub const DEVICE_RESOURCE: &str = "__device__";

/// 资源占用模式。exclusive 单持有者；shared 最多 `capacity` 个存活持有者。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceMode {
    Exclusive,
    Shared,
}

/// 一次 claim 的资源声明：工具层从 device declaration 解析后传入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceSpec {
    pub id: String,
    pub mode: ResourceMode,
    /// shared 时有效（至少按 1 处理）；exclusive 恒为单持有者。
    pub capacity: u32,
}

/// busy 拒绝必须带持有者名与资源 id（人读得懂的拒绝）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("busy: {holder} holds {resource}")]
pub struct Busy {
    pub holder: String,
    pub resource: String,
}

/// claim 的唯一键：`(thing_id, resource_id)`。E4c 仍按 thing 全局，workspace
/// 鉴权在工具层。
type ClaimKey = (String, String);

struct HolderEntry {
    holder: String,
    /// 每次 claim 的唯一令牌——Lease Drop 时按令牌校验，TTL 过期后被他人
    /// 重 claim 的持有者不被旧 Lease 误删。
    token: u64,
    acquired_at: Instant,
    ttl: Duration,
}

impl HolderEntry {
    fn new(holder: &str, token: u64, ttl: Duration) -> Self {
        Self {
            holder: holder.to_string(),
            token,
            acquired_at: Instant::now(),
            ttl,
        }
    }

    fn alive(&self) -> bool {
        self.acquired_at.elapsed() < self.ttl
    }
}

struct ClaimEntry {
    mode: ResourceMode,
    capacity: u32,
    holders: Vec<HolderEntry>,
}

/// 资源 claim 注册表。Cheap to clone —— 所有克隆共享同一张表（跨 run、
/// 跨 chat/自治工具共享同一份仲裁状态）。
#[derive(Clone, Default)]
pub struct ArbitrationRegistry {
    claims: Arc<DashMap<ClaimKey, ClaimEntry>>,
    /// E2 stop 闸门：设备 → (原因, 截止时刻)。期间 call/set 一律拒绝。
    stops: Arc<DashMap<String, (String, Instant)>>,
    next_token: Arc<AtomicU64>,
}

impl ArbitrationRegistry {
    /// 按 `specs` claim 该设备的多个资源：按资源 id 排序后 all-or-nothing。
    /// 任一资源 busy → 释放本次已获 claim，返回首个 [`Busy`]。
    ///
    /// `specs` 为空时不注入资源（合成 `__device__` 由工具层决定）。
    pub fn try_acquire_resources(
        &self,
        thing_id: &str,
        holder: String,
        specs: &[ResourceSpec],
        ttl: Duration,
    ) -> Result<Lease, Busy> {
        // stop 与 claim 互斥：闸门期间的 claim 直接拒绝（stop 优先）。
        if let Some(reason) = self.stop_reason(thing_id) {
            return Err(stop_busy(&reason));
        }

        // 固定排序：并发 claim 重叠资源集时不会互相等成环（也不做排队）。
        // 重复 id 去重：同一份声明重复出现不得 self-busy。
        let mut sorted: Vec<&ResourceSpec> = specs.iter().collect();
        sorted.sort_by(|a, b| a.id.cmp(&b.id));
        sorted.dedup_by(|a, b| a.id == b.id);

        let mut held = Vec::with_capacity(sorted.len());
        for spec in sorted {
            match self.claim(thing_id, &holder, spec, ttl) {
                Ok(token) => held.push(HeldClaim {
                    thing_id: thing_id.to_string(),
                    resource: spec.id.clone(),
                    token,
                }),
                Err(busy) => {
                    self.release_all(&held);
                    return Err(busy);
                }
            }
        }
        // 复检 stop：关闭 retain（set_stop 释放）与新 insert 之间的 TOCTOU。
        if let Some(reason) = self.stop_reason(thing_id) {
            self.release_all(&held);
            return Err(stop_busy(&reason));
        }
        Ok(Lease {
            claims: Arc::clone(&self.claims),
            held,
        })
    }

    /// 兼容入口（E1 语义）：claim 合成的 [`DEVICE_RESOURCE`] exclusive 资源。
    pub fn try_acquire(&self, thing_id: &str, holder: String, ttl: Duration) -> Result<Lease, Busy> {
        self.try_acquire_resources(thing_id, holder, &[device_spec()], ttl)
    }

    /// 默认 TTL（10min）的 acquire 便捷入口。
    pub fn try_acquire_default(&self, thing_id: &str, holder: String) -> Result<Lease, Busy> {
        self.try_acquire(thing_id, holder, DEFAULT_LEASE_TTL)
    }

    /// E2 stop：置设备 stop 闸门并释放该设备全部 claim（在途单次调用不可中断）。
    ///
    /// 截止时刻用 `checked_add`：超大 ttl 不得 panic（调用方还会自行 clamp）。
    pub fn set_stop(&self, thing_id: &str, reason: String, ttl: Duration) {
        let until = Instant::now().checked_add(ttl).unwrap_or_else(|| {
            // 溢出的 ttl 按最大可表示窗口处理，而不是 panic / 永不过期。
            Instant::now() + Duration::from_secs(3600)
        });
        self.stops.insert(thing_id.to_string(), (reason, until));
        // 释放占用：stop 是最高优先级的"让路"指令。
        self.force_release_thing(thing_id);
    }

    /// stop 闸门命中返回原因；已过期/未设置返回 None。
    ///
    /// 过期清理必须与 `set_stop` 互斥：先读后删会让并发 `set_stop` 刚插入的
    /// 新闸门被这次删除静默丢掉。`remove_if` 在分片锁内复检截止时刻——
    /// 只删「此刻仍已过期」的条目。
    pub fn stop_reason(&self, thing_id: &str) -> Option<String> {
        if let Some(entry) = self.stops.get(thing_id)
            && Instant::now() < entry.1
        {
            return Some(entry.0.clone());
        }
        let now = Instant::now();
        self.stops.remove_if(thing_id, |_, (_, until)| *until <= now);
        None
    }

    /// 强制释放设备全部 claim（不设 stop 闸门）。迟到 Lease Drop 按 token
    /// 不会误删后续新持有者。
    pub fn force_release_thing(&self, thing_id: &str) {
        self.claims.retain(|key, _| key.0 != thing_id);
    }

    /// E1 命名的兼容别名。
    pub fn force_release(&self, thing_id: &str) {
        self.force_release_thing(thing_id);
    }

    fn mint(&self) -> u64 {
        self.next_token.fetch_add(1, Ordering::Relaxed)
    }

    /// claim 单个资源。entry API 保证检查-插入原子。
    fn claim(&self, thing_id: &str, holder: &str, spec: &ResourceSpec, ttl: Duration) -> Result<u64, Busy> {
        // capacity=0 的声明是错误输入；按 1 处理（fail-closed 而非放行无限）。
        let capacity = match spec.mode {
            ResourceMode::Exclusive => 1,
            ResourceMode::Shared => spec.capacity.max(1),
        };
        let key = (thing_id.to_string(), spec.id.clone());
        match self.claims.entry(key) {
            Entry::Vacant(v) => {
                let token = self.mint();
                v.insert(ClaimEntry {
                    mode: spec.mode,
                    capacity,
                    holders: vec![HolderEntry::new(holder, token, ttl)],
                });
                Ok(token)
            }
            Entry::Occupied(mut o) => {
                let entry = o.get_mut();
                // 过期持有者不占位（崩溃恢复）。
                entry.holders.retain(HolderEntry::alive);
                if entry.holders.is_empty() {
                    // 空 entry 的 mode/capacity 跟随本次声明。
                    entry.mode = spec.mode;
                    entry.capacity = capacity;
                }
                // 任一方声明 exclusive 即按 exclusive 冲突（fail-closed）。
                let exclusive = entry.mode == ResourceMode::Exclusive || spec.mode == ResourceMode::Exclusive;
                let full = if exclusive {
                    !entry.holders.is_empty()
                } else {
                    entry.holders.len() as u32 >= entry.capacity.min(capacity)
                };
                if full {
                    let current = &entry.holders[0];
                    return Err(Busy {
                        holder: current.holder.clone(),
                        resource: spec.id.clone(),
                    });
                }
                let token = self.mint();
                entry.holders.push(HolderEntry::new(holder, token, ttl));
                Ok(token)
            }
        }
    }

    fn release_all(&self, held: &[HeldClaim]) {
        for claim in held {
            release_claim(&self.claims, claim);
        }
    }
}

/// stop 闸门期间的 claim 拒绝：holder 带 stop 原因，resource 为合成设备资源。
fn stop_busy(reason: &str) -> Busy {
    Busy {
        holder: format!("stop:{reason}"),
        resource: DEVICE_RESOURCE.to_string(),
    }
}

fn device_spec() -> ResourceSpec {
    ResourceSpec {
        id: DEVICE_RESOURCE.to_string(),
        mode: ResourceMode::Exclusive,
        capacity: 1,
    }
}

/// 释放一条 claim：按 token 精确删除自己的持有者；entry 空了再删键。
fn release_claim(claims: &DashMap<ClaimKey, ClaimEntry>, claim: &HeldClaim) {
    let key = (claim.thing_id.clone(), claim.resource.clone());
    if let Entry::Occupied(mut occupied) = claims.entry(key) {
        let empty = {
            let entry = occupied.get_mut();
            entry.holders.retain(|h| h.token != claim.token);
            entry.holders.is_empty()
        };
        if empty {
            occupied.remove();
        }
    }
}

/// 多资源 claim 租约——析构即释放全部 claim（RAII）。只删自己的 token：
/// TTL 过期后已被他人重 claim 的资源不动。
pub struct Lease {
    claims: Arc<DashMap<ClaimKey, ClaimEntry>>,
    held: Vec<HeldClaim>,
}

/// spec §2.1 的命名别名。
pub type ResourceLease = Lease;

struct HeldClaim {
    thing_id: String,
    resource: String,
    token: u64,
}

impl std::fmt::Debug for Lease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let held: Vec<_> = self.held.iter().map(|h| (&h.thing_id, &h.resource, h.token)).collect();
        f.debug_struct("Lease").field("held", &held).finish()
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        for claim in &self.held {
            release_claim(&self.claims, claim);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TTL: Duration = Duration::from_secs(60);

    fn exclusive(id: &str) -> ResourceSpec {
        ResourceSpec {
            id: id.into(),
            mode: ResourceMode::Exclusive,
            capacity: 1,
        }
    }

    fn shared(id: &str, capacity: u32) -> ResourceSpec {
        ResourceSpec {
            id: id.into(),
            mode: ResourceMode::Shared,
            capacity,
        }
    }

    #[test]
    fn mutual_exclusion_per_thing() {
        let reg = ArbitrationRegistry::default();
        let _a = reg
            .try_acquire("dev_1", "run-A".into(), TTL)
            .expect("首次 acquire 必成");

        // 同设备第二个持有者 → busy，且带上当前持有者名与合成资源 id。
        let err = reg
            .try_acquire("dev_1", "run-B".into(), TTL)
            .expect_err("同设备并发必须 busy");
        assert_eq!(
            err,
            Busy {
                holder: "run-A".into(),
                resource: DEVICE_RESOURCE.into(),
            }
        );

        // 不同设备互不影响。
        reg.try_acquire("dev_2", "run-B".into(), TTL).expect("不同设备不互斥");
    }

    #[test]
    fn busy_message_carries_holder_and_resource() {
        let reg = ArbitrationRegistry::default();
        let _a = reg
            .try_acquire("dev_1", "值守run#42".into(), TTL)
            .expect("首次 acquire 必成");
        let err = reg.try_acquire("dev_1", "chat".into(), TTL).expect_err("必 busy");
        // 人读得懂的拒绝：错误文案必须带持有者名与资源 id。
        let msg = err.to_string();
        assert!(msg.contains("值守run#42"), "busy 文案必须带持有者名: {msg}");
        assert!(msg.contains(DEVICE_RESOURCE), "busy 文案必须带资源 id: {msg}");
    }

    /// 不同资源互不干扰：同设备上 action:reboot 与 action:calibrate 可并行。
    #[test]
    fn exclusive_claims_are_per_resource() {
        let reg = ArbitrationRegistry::default();
        let _a = reg
            .try_acquire_resources("dev_1", "run-A".into(), &[exclusive("action:reboot")], TTL)
            .expect("首个 claim 必成");
        reg.try_acquire_resources("dev_1", "run-B".into(), &[exclusive("action:calibrate")], TTL)
            .expect("不同资源可并行");

        let err = reg
            .try_acquire_resources("dev_1", "run-C".into(), &[exclusive("action:reboot")], TTL)
            .expect_err("同名资源必须互斥");
        assert_eq!(err.holder, "run-A");
        assert_eq!(err.resource, "action:reboot");
    }

    /// exclusive 的 capacity 字段被忽略：恒为单持有者。
    #[test]
    fn exclusive_ignores_capacity() {
        let reg = ArbitrationRegistry::default();
        let _a = reg
            .try_acquire_resources(
                "dev_1",
                "run-A".into(),
                &[ResourceSpec {
                    id: "x".into(),
                    mode: ResourceMode::Exclusive,
                    capacity: 8,
                }],
                TTL,
            )
            .expect("首个 claim 必成");
        reg.try_acquire_resources(
            "dev_1",
            "run-B".into(),
            &[ResourceSpec {
                id: "x".into(),
                mode: ResourceMode::Exclusive,
                capacity: 8,
            }],
            TTL,
        )
        .expect_err("exclusive 只能有一个持有者");
    }

    #[test]
    fn shared_capacity_counts_alive_holders() {
        let reg = ArbitrationRegistry::default();
        let a = reg
            .try_acquire_resources("dev_1", "run-A".into(), &[shared("stream", 2)], TTL)
            .expect("shared 首个必成");
        let b = reg
            .try_acquire_resources("dev_1", "run-B".into(), &[shared("stream", 2)], TTL)
            .expect("capacity 内必成");

        let err = reg
            .try_acquire_resources("dev_1", "run-C".into(), &[shared("stream", 2)], TTL)
            .expect_err("超出 capacity 必须 busy");
        assert_eq!(err.resource, "stream");
        assert_eq!(err.holder, "run-A");

        // 释放一个 → capacity 复活。
        drop(a);
        reg.try_acquire_resources("dev_1", "run-C".into(), &[shared("stream", 2)], TTL)
            .expect("释放后必须可再 claim");
        drop(b);
    }

    /// shared 与 exclusive 声明同一资源时 fail-closed 冲突。
    #[test]
    fn shared_and_exclusive_conflict() {
        let reg = ArbitrationRegistry::default();
        let _shared = reg
            .try_acquire_resources("dev_1", "run-A".into(), &[shared("stream", 4)], TTL)
            .expect("shared claim");
        reg.try_acquire_resources("dev_1", "run-B".into(), &[exclusive("stream")], TTL)
            .expect_err("exclusive 必须与 shared 冲突");
        drop(_shared);

        let _exclusive = reg
            .try_acquire_resources("dev_1", "run-A".into(), &[exclusive("stream")], TTL)
            .expect("exclusive claim");
        reg.try_acquire_resources("dev_1", "run-B".into(), &[shared("stream", 4)], TTL)
            .expect_err("shared 必须与 exclusive 冲突");
    }

    /// 排序 + all-or-nothing：后面资源 busy 时，前面已获 claim 必须释放。
    #[test]
    fn sorted_all_or_nothing_releases_partial_claims() {
        let reg = ArbitrationRegistry::default();
        let _a = reg
            .try_acquire_resources("dev_1", "run-A".into(), &[exclusive("res:b")], TTL)
            .expect("run-A 占 b");

        // 故意乱序传入：排序后先 a 后 b，在 b 上失败。
        let err = reg
            .try_acquire_resources("dev_1", "run-B".into(), &[exclusive("res:b"), exclusive("res:a")], TTL)
            .expect_err("b 被占必须 busy");
        assert_eq!(err.holder, "run-A");
        assert_eq!(err.resource, "res:b");

        // B 已获的 a 必须回滚 → C 可立即 claim。
        reg.try_acquire_resources("dev_1", "run-C".into(), &[exclusive("res:a")], TTL)
            .expect("partial claim 必须已释放");
    }

    /// 排序报错必须指向排序后的首个 busy 资源（而非入参首个）。
    #[test]
    fn sorted_order_reports_first_busy_resource() {
        let reg = ArbitrationRegistry::default();
        let _a = reg
            .try_acquire_resources("dev_1", "run-A".into(), &[exclusive("res:a")], TTL)
            .expect("run-A 占 a");
        let _b = reg
            .try_acquire_resources("dev_1", "run-B".into(), &[exclusive("res:b")], TTL)
            .expect("run-B 占 b");

        // 入参 [b, a]：不排序会先撞 b（holder B）；排序后先撞 a（holder A）。
        let err = reg
            .try_acquire_resources("dev_1", "run-C".into(), &[exclusive("res:b"), exclusive("res:a")], TTL)
            .expect_err("两个资源都被占，必 busy");
        assert_eq!(err.holder, "run-A");
        assert_eq!(err.resource, "res:a");
    }

    /// 重复声明 id 去重，不得 self-busy。
    #[test]
    fn duplicate_spec_ids_do_not_self_busy() {
        let reg = ArbitrationRegistry::default();
        let _a = reg
            .try_acquire_resources("dev_1", "run-A".into(), &[exclusive("res:a"), exclusive("res:a")], TTL)
            .expect("重复 id 去重后不得 self-busy");
        reg.try_acquire_resources("dev_1", "run-B".into(), &[exclusive("res:a")], TTL)
            .expect_err("去重后仍只有一个持有者");
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

    /// shared capacity 不计已过期持有者。
    #[test]
    fn expired_shared_holders_do_not_count_toward_capacity() {
        let reg = ArbitrationRegistry::default();
        let _dead = reg
            .try_acquire_resources("dev_1", "crashed".into(), &[shared("stream", 1)], Duration::ZERO)
            .expect("首个 claim 必成");
        reg.try_acquire_resources("dev_1", "run-B".into(), &[shared("stream", 1)], TTL)
            .expect("过期持有者不得占用 capacity");
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

    /// 多资源 Lease 的迟到 Drop 只按 token 删自己的 claim。
    #[test]
    fn stale_multi_resource_drop_only_releases_own_tokens() {
        let reg = ArbitrationRegistry::default();
        let stale = reg
            .try_acquire_resources(
                "dev_1",
                "crashed".into(),
                &[exclusive("res:a"), exclusive("res:b")],
                Duration::ZERO,
            )
            .expect("过期租约");
        let _live = reg
            .try_acquire_resources("dev_1", "run-B".into(), &[exclusive("res:a"), exclusive("res:b")], TTL)
            .expect("过期接管");
        drop(stale);
        let err = reg
            .try_acquire_resources("dev_1", "run-C".into(), &[exclusive("res:b")], TTL)
            .expect_err("run-B 的 claim 必须仍在");
        assert_eq!(err.holder, "run-B");
    }

    /// 空 specs：不注入资源（由工具层决定），返回空 Lease 且 Drop 安全。
    #[test]
    fn empty_specs_yield_empty_lease() {
        let reg = ArbitrationRegistry::default();
        let lease = reg
            .try_acquire_resources("dev_1", "run-A".into(), &[], TTL)
            .expect("空 specs 必成");
        drop(lease);
        reg.try_acquire("dev_1", "run-B".into(), TTL)
            .expect("空 Lease 不占资源");
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

    /// E2 stop：闸门命中带原因；释放该设备全部 claim；期间 claim 一律拒绝；
    /// 闸门过期后自动恢复。
    #[test]
    fn stop_gate_releases_all_claims_and_rejects_new_claims() {
        let reg = ArbitrationRegistry::default();
        let lease = reg
            .try_acquire_resources("dev_1", "run-A".into(), &[exclusive("res:a"), exclusive("res:b")], TTL)
            .expect("acquire");
        reg.set_stop("dev_1", "operator stop".into(), Duration::from_secs(60));
        assert_eq!(reg.stop_reason("dev_1").as_deref(), Some("operator stop"));

        // stop 与 claim 互斥：新 claim 被拒绝，holder 带 stop:<reason>。
        let err = reg
            .try_acquire_resources("dev_1", "run-B".into(), &[exclusive("res:a")], TTL)
            .expect_err("stop 期间 claim 必须拒绝");
        assert_eq!(
            err,
            Busy {
                holder: "stop:operator stop".into(),
                resource: DEVICE_RESOURCE.into(),
            }
        );
        // 兼容入口同样被拒绝。
        let err = reg
            .try_acquire("dev_1", "run-B".into(), TTL)
            .expect_err("stop 期间 try_acquire 必须拒绝");
        assert_eq!(err.holder, "stop:operator stop");

        // 迟到 Lease Drop 不改变闸门。
        drop(lease);
        assert_eq!(reg.stop_reason("dev_1").as_deref(), Some("operator stop"));

        // 零 TTL = 立即过期。
        reg.set_stop("dev_2", "x".into(), Duration::ZERO);
        assert_eq!(reg.stop_reason("dev_2"), None);
    }

    /// stop 拒绝的 claim 不得留下 partial claim：acquire res:a 后 stop，
    /// 闸门过期时 res:a 与 res:b 都空闲。
    #[test]
    fn stopped_claim_leaves_no_partial_claims() {
        let reg = ArbitrationRegistry::default();
        let lease = reg
            .try_acquire_resources("dev_1", "run-A".into(), &[exclusive("res:a")], TTL)
            .expect("acquire");
        reg.set_stop("dev_1", "operator stop".into(), Duration::from_millis(10));
        let err = reg
            .try_acquire_resources("dev_1", "run-B".into(), &[exclusive("res:a"), exclusive("res:b")], TTL)
            .expect_err("stop 期间 claim 必须拒绝");
        assert_eq!(err.holder, "stop:operator stop");

        std::thread::sleep(Duration::from_millis(30));
        // 闸门过期：stop 已释放 run-A 的 claim，失败的 run-B 也未留下 partial。
        reg.try_acquire_resources("dev_1", "run-C".into(), &[exclusive("res:a"), exclusive("res:b")], TTL)
            .expect("stop 过期后资源必须全部空闲");
        drop(lease);
    }

    /// Issue 1 回归：过期闸门的清理不得删除并发 `set_stop` 刚置的新闸门。
    ///
    /// 旧实现 get（看到过期）→ remove 之间若插入新闸门，remove 会把它删掉。
    /// `remove_if` 在分片锁内复检截止时刻，因此新闸门（长 TTL）必须始终可见。
    #[test]
    fn expired_stop_sweep_never_deletes_freshly_set_gate() {
        let reg = ArbitrationRegistry::default();
        // 先放一个已过期条目，保证清扫路径被走到。
        reg.set_stop("dev-1", "stale".into(), Duration::ZERO);

        let barrier = Arc::new(std::sync::Barrier::new(2));
        let sweeper = {
            let reg = reg.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                // 持续走「读 → 过期清扫」路径，与 set_stop 抢同一分片。
                for _ in 0..5_000 {
                    let _ = reg.stop_reason("dev-1");
                }
            })
        };

        barrier.wait();
        for i in 0..5_000 {
            reg.set_stop("dev-1", "fresh".into(), TTL);
            assert_eq!(
                reg.stop_reason("dev-1").as_deref(),
                Some("fresh"),
                "第 {i} 轮：新设置的闸门不得被并发过期清扫删除"
            );
            // 下一轮重新制造过期条目（0 TTL），让清扫路径保持活跃。
            reg.set_stop("dev-1", "stale".into(), Duration::ZERO);
        }
        sweeper.join().expect("sweeper thread");
        reg.set_stop("dev-1", "fresh".into(), TTL);
        assert_eq!(reg.stop_reason("dev-1").as_deref(), Some("fresh"));
    }

    /// 超大 ttl 不得 panic（checked_add 兜底）。
    #[test]
    fn set_stop_with_huge_ttl_does_not_panic() {
        let reg = ArbitrationRegistry::default();
        reg.set_stop("dev-1", "huge".into(), Duration::MAX);
        assert_eq!(reg.stop_reason("dev-1").as_deref(), Some("huge"));
    }

    #[test]
    fn force_release_thing_frees_all_resources_without_stop_gate() {
        let reg = ArbitrationRegistry::default();
        let _lease = reg
            .try_acquire_resources("dev_3", "run-A".into(), &[exclusive("res:a"), exclusive("res:b")], TTL)
            .expect("acquire");
        reg.force_release_thing("dev_3");
        reg.try_acquire_resources("dev_3", "run-B".into(), &[exclusive("res:a"), exclusive("res:b")], TTL)
            .expect("force_release 后必须可再 claim");
        assert_eq!(reg.stop_reason("dev_3"), None);
    }
}
