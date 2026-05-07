//! 应用状态。W1:占位空 struct,W2 起填充。

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Model {
    /// W1 占位字段(避免 #[derive] 在空 struct 上绕弯)。
    /// W2 替换成真正的 active_page / providers / proxy_status 等字段。
    pub _placeholder: (),
}
