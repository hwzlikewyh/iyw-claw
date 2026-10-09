// codeg v0.34.0 的生产逻辑；由原模块统一导出。
#[allow(unused_imports)]
use super::*;

impl AgentTarget {
    pub fn generation(&self) -> &str {
        match self {
            AgentTarget::Element(e) => &e.generation,
            AgentTarget::Point(p) => &p.generation,
        }
    }
}
