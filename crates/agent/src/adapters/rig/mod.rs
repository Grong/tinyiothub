//! rig 引擎适配器（phase 2）：port 接口 → rig 0.42 运行时。

pub mod loop_;
pub mod memory;
pub mod provider;
pub mod tools;

#[cfg(test)]
mod tests;
