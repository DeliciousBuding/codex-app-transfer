//! UI 触发的意图。W1:占位空 enum,W2 起填充对应 20 个 data-action。

#[derive(Debug, Clone)]
pub enum Action {
    /// W1 占位:让编译通过。W2 起加 ApplyDesktop / BackupConfig / ...
    Noop,
}
