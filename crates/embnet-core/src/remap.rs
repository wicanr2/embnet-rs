//! Bridge 的 Transfer-ID 對應（`docs/spec/06-forwarding.md` 第三節）。

/// 一個 session 的 32 位元展開值。模數必須是 2 的冪且不大於 256。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TidRemap {
    u: Option<u32>,
}

impl TidRemap {
    /// 尚未見過這個 session。
    pub const fn new() -> Self {
        Self { u: None }
    }

    /// 入口收到 Transfer-ID `t`（模數 `modulo_in`），更新並回傳展開值。
    pub fn ingress(&mut self, t: u8, modulo_in: u32) -> u32 {
        let mask = modulo_in.wrapping_sub(1);
        let t = u32::from(t) & mask;
        let u = match self.u {
            None => t,
            Some(u) => u.wrapping_add(t.wrapping_sub(u & mask) & mask),
        };
        self.u = Some(u);
        u
    }

    /// 出口的 Transfer-ID（模數 `modulo_out`）。
    pub fn egress(&self, modulo_out: u32) -> Option<u8> {
        self.u.map(|u| (u & modulo_out.wrapping_sub(1)) as u8)
    }
}

impl TidRemap {
    /// 目前的 32 位元展開值；尚未見過時為 `None`。
    pub const fn value(&self) -> Option<u32> {
        self.u
    }
}
