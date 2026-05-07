//! Desktop 应用状态机 — W1 起步阶段:仅占位类型 + 文档骨架。
//!
//! 后续 W2 起会逐步落实:
//! - `Model`: 整应用 immutable state(active page / providers / proxy status / ...)
//! - `Action`: UI 触发的意图(用户点按钮 / 输入文本 / 切页)
//! - `Effect`: 派出去的副作用(异步 HTTP / 文件 IO / 系统 API)
//!
//! 目标 reducer 流程:`(Model, Action) -> (Model, Vec<Effect>)`,
//! egui frame 中只读 Model,不直接修改;副作用由 desktop_app crate 的
//! Tokio runtime 执行后再以 Action 形式回来。

pub mod action;
pub mod effect;
pub mod model;

pub use action::Action;
pub use effect::Effect;
pub use model::Model;
