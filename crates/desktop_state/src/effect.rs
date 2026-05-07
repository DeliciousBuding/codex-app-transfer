//! Reducer 派出的副作用,由 desktop_app 的 runtime 执行。
//! W1:占位空 enum。

#[derive(Debug, Clone)]
pub enum Effect {
    /// W1 占位。W2 起加 HttpRequest / SpawnTokio / OpenFile / ...
    Noop,
}
