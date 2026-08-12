// Feature modules — Handler → Service → Repo 三层架构
// 逐步迁移，每个模块就绪后取消注释
//
// P4-Task24: admin 域抽取完成 — system/monitoring/batch/jobs/open 与
// device handler 迁入 crates/admin；cron shim 删除（消费者直接用
// tinyiothub_storage::traits::cron）；device shim 删除（消费者直接用
// crate::domains::driver::legacy / crate::domains::thing::legacy）。
// 仅剩 marketplace（Task 25 处理 cloud→marketplace 依赖后迁出）。

pub mod marketplace;
