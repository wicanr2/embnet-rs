//! 規格用到的兩種 CRC（`docs/spec/README.md`）。查表法，表在編譯期產生。

const fn table16() -> [u16; 256] {
    let mut t = [0u16; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut c = (i as u16) << 8;
        let mut j = 0u8;
        while j < 8 {
            c = if c & 0x8000 != 0 { (c << 1) ^ 0x1021 } else { c << 1 };
            j = j.wrapping_add(1);
        }
        // i < 256，索引不會越界；const fn 裡沒有 get_mut 可用。
        #[allow(clippy::indexing_slicing)]
        {
            t[i] = c;
        }
        i = i.wrapping_add(1);
    }
    t
}

const fn table32_reflected() -> [u32; 256] {
    // CRC-32/AUTOSAR 的多項式 0xF4ACFB13，反射後為 0xC8DF352F。
    let mut t = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut c = i as u32;
        let mut j = 0u8;
        while j < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xC8DF_352F } else { c >> 1 };
            j = j.wrapping_add(1);
        }
        #[allow(clippy::indexing_slicing)]
        {
            t[i] = c;
        }
        i = i.wrapping_add(1);
    }
    t
}

static T16: [u16; 256] = table16();
static T32: [u32; 256] = table32_reflected();

/// CRC-16/IBM-3740（也稱 CRC-16/CCITT-FALSE）的累加器。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crc16(u16);

impl Default for Crc16 {
    fn default() -> Self {
        Self::new()
    }
}

impl Crc16 {
    /// 初值 0xFFFF。
    pub const fn new() -> Self {
        Self(0xFFFF)
    }

    /// 累加一段資料。
    pub fn update(&mut self, data: &[u8]) {
        for &b in data {
            let idx = usize::from(((self.0 >> 8) as u8) ^ b);
            let t = T16.get(idx).copied().unwrap_or(0);
            self.0 = (self.0 << 8) ^ t;
        }
    }

    /// 目前的值（xorout 為 0）。
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// 一次算完 CRC-16/IBM-3740。
pub fn crc16(data: &[u8]) -> u16 {
    let mut c = Crc16::new();
    c.update(data);
    c.get()
}

/// CRC-32/AUTOSAR 的累加器。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crc32Autosar(u32);

impl Default for Crc32Autosar {
    fn default() -> Self {
        Self::new()
    }
}

impl Crc32Autosar {
    /// 初值 0xFFFFFFFF。
    pub const fn new() -> Self {
        Self(0xFFFF_FFFF)
    }

    /// 累加一段資料。
    pub fn update(&mut self, data: &[u8]) {
        for &b in data {
            let idx = usize::from((self.0 as u8) ^ b);
            let t = T32.get(idx).copied().unwrap_or(0);
            self.0 = (self.0 >> 8) ^ t;
        }
    }

    /// 最終值（xorout 0xFFFFFFFF）。
    pub const fn get(self) -> u32 {
        self.0 ^ 0xFFFF_FFFF
    }
}

/// 一次算完 CRC-32/AUTOSAR。
pub fn crc32_autosar(data: &[u8]) -> u32 {
    let mut c = Crc32Autosar::new();
    c.update(data);
    c.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_values() {
        assert_eq!(crc16(b"123456789"), 0x29B1);
        assert_eq!(crc32_autosar(b"123456789"), 0x1697_D06A);
    }
}

const fn table_reflected32(poly_reflected: u32) -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut c = i as u32;
        let mut j = 0u8;
        while j < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ poly_reflected } else { c >> 1 };
            j = j.wrapping_add(1);
        }
        #[allow(clippy::indexing_slicing)]
        {
            t[i] = c;
        }
        i = i.wrapping_add(1);
    }
    t
}

// CRC-32C 的多項式 0x1EDC6F41，反射後為 0x82F63B78。
static T32C: [u32; 256] = table_reflected32(0x82F6_3B78);

/// CRC-32C（CRC-32/ISCSI），datagram 用（`docs/spec/04-datagram-links.md`）。
pub fn crc32c(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        let t = T32C.get(usize::from((c as u8) ^ b)).copied().unwrap_or(0);
        c = (c >> 8) ^ t;
    }
    c ^ 0xFFFF_FFFF
}

/// CRC-64/XZ，Type hash 用（`docs/spec/07-config.md` 第二節）。逐位元計算，只在建置期用。
pub fn crc64_xz(data: &[u8]) -> u64 {
    let mut c = u64::MAX;
    for &b in data {
        c ^= u64::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xC96C_5795_D787_0F42 } else { c >> 1 };
        }
    }
    c ^ u64::MAX
}

#[cfg(test)]
mod tests_more {
    use super::*;

    #[test]
    fn check_values_more() {
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
        assert_eq!(crc64_xz(b"123456789"), 0x995D_C9BB_DF19_39FA);
    }
}
