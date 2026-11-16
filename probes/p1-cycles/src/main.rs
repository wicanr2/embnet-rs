//! 每一步做 K 次，記下 DWT CYCCNT 的增量除以 K。結果寫進 P1_RESULT：
//!   [0] 標記 0x0C1C_0C1C 表示跑完
//!   [1] K
//!   [2..] 各步驟每次的 CYCCNT 增量（順序見 STEPS）
//! Renode 的 CYCCNT 以 72 MHz 對虛擬時間計數，CPU 以 462 MIPS 推進，所以指令數 = 增量 × 462 / 72。
#![no_std]
#![no_main]

use cortex_m::peripheral::DWT;
use embnet_core::can::{CanId, Mtu, NodeId, Priority, Segmenter, SessionTable, SubjectId};
use embnet_core::datagram::{self, Header};
use embnet_core::e2e::{self, DataId, SeqReceiver};
use embnet_core::mac::{self, TagLen};

const K: u32 = 200;

#[unsafe(no_mangle)]
static mut P1_RESULT: [u32; 16] = [0; 16];

fn put(i: usize, v: u32) {
    unsafe { core::ptr::write_volatile((&raw mut P1_RESULT as *mut u32).add(i), v) }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    put(0, 0xBAD0_BAD0);
    loop {}
}

#[inline(never)]
fn measure(mut f: impl FnMut()) -> u32 {
    let c0 = DWT::cycle_count();
    for _ in 0..K {
        f();
    }
    DWT::cycle_count().wrapping_sub(c0) / K
}

#[cortex_m_rt::entry]
fn main() -> ! {
    let mut cp = cortex_m::Peripherals::take().unwrap_or_else(|| loop {});
    cp.DCB.enable_trace();
    DWT::unlock();
    cp.DWT.enable_cycle_counter();

    let key = [0x11u8; 16]; // test-only
    let id = DataId { subject_id: 100, source: 10, type_hash: 0x4FF0_0AF1_D812_8708 };
    let data2 = [0x12u8, 0x34];
    let mut wire = [0u8; 64];
    let n_mac = mac::encode(&key, TagLen::Bits32, &id, 1, &data2, &mut wire).unwrap_or(0);
    let mac_wire = wire;
    let mut e2e_wire = [0u8; 64];
    let n_e2e = e2e::encode(&id, 1, &data2, &mut e2e_wire).unwrap_or(0);

    let payload9 = [0x55u8; 9];
    let can_id = CanId::Subject {
        priority: Priority::new(2).unwrap_or_else(|| loop {}),
        subject: SubjectId::new(100).unwrap_or_else(|| loop {}),
        source: NodeId::new(10).unwrap_or_else(|| loop {}),
    };
    let mut frames = [[0u8; 8]; 2];
    let mut lens = [0usize; 2];
    let mut seg = Segmenter::new(&payload9, Mtu::Classic, 3);
    for (f, l) in frames.iter_mut().zip(lens.iter_mut()) {
        *l = seg.next_frame(f).unwrap_or(0);
    }
    let h = Header { priority: 2, service: false, request: false, port: 100, source: 10, destination: 0xFF, transfer_id: 1 };
    let mut dg = [0u8; 64];
    let n_dg = datagram::encode(&h, &[0xAA; 20], &mut dg).unwrap_or(0);

    let mut sink = 0u32;
    let mut i = 0usize;
    let mut step = |v: u32| {
        put(2 + i, v);
        i += 1;
    };

    // STEPS
    // 0 mac32 產生（2 B 資料，CMAC 輸入 17 B = 2 個 AES 區塊）
    step(measure(|| {
        let mut o = [0u8; 16];
        sink ^= mac::encode(&key, TagLen::Bits32, &id, 1, &data2, &mut o).unwrap_or(0) as u32 ^ u32::from(o[3]);
    }));
    // 1 mac32 驗證
    step(measure(|| {
        let mut rx = SeqReceiver::new(3);
        rx.sync(1);
        sink ^= mac::check(&mut rx, &key, TagLen::Bits32, &id, &mac_wire[..n_mac]).0 as u32;
    }));
    // 2 e2e32 產生
    step(measure(|| {
        let mut o = [0u8; 16];
        sink ^= e2e::encode(&id, 1, &data2, &mut o).unwrap_or(0) as u32 ^ u32::from(o[3]);
    }));
    // 3 e2e32 驗證
    step(measure(|| {
        let mut rx = SeqReceiver::new(3);
        rx.sync(1);
        sink ^= rx.check(&id, &e2e_wire[..n_e2e]).0 as u32;
    }));
    // 4 CAN ID 解碼
    step(measure(|| {
        sink ^= CanId::decode(core::hint::black_box(can_id.encode())).map_or(1, |_| 0);
    }));
    // 5 分框：9 B 切成 2 框
    step(measure(|| {
        let mut s = Segmenter::new(core::hint::black_box(&payload9), Mtu::Classic, 3);
        let mut b = [0u8; 8];
        while let Some(n) = s.next_frame(&mut b) {
            sink ^= n as u32;
        }
    }));
    // 6 重組：2 框，經 session 表（8 個 session）
    let mut table: SessionTable<8, 32> = SessionTable::new();
    let mut now = 0u64;
    step(measure(|| {
        now += 1_000_000; // 每輪間隔 1 秒，避開去重複
        for (f, &l) in frames.iter().zip(lens.iter()) {
            if let Ok(Some(a)) = table.push(now, 500_000, can_id, &f[..l]) {
                sink ^= a.payload.len() as u32;
            }
        }
    }));
    // 7 datagram 解碼與 CRC-32C（20 B payload）
    step(measure(|| {
        sink ^= datagram::decode(core::hint::black_box(&dg[..n_dg])).map_or(1, |(_, p)| p.len() as u32);
    }));
    // 8 mac32 產生，金鑰預先展開
    let mk = mac::MacKey::new(&key).unwrap_or_else(|| loop {});
    step(measure(|| {
        let mut o = [0u8; 16];
        sink ^= mk.encode(TagLen::Bits32, &id, 1, &data2, &mut o).unwrap_or(0) as u32 ^ u32::from(o[3]);
    }));
    // 9 mac32 驗證，金鑰預先展開
    step(measure(|| {
        let mut rx = SeqReceiver::new(3);
        rx.sync(1);
        sink ^= mk.check(&mut rx, TagLen::Bits32, &id, &mac_wire[..n_mac]).0 as u32;
    }));
    // 10 金鑰展開本身
    step(measure(|| {
        sink ^= mac::MacKey::new(core::hint::black_box(&key)).map_or(1, |_| 0);
    }));

    put(1, K);
    put(15, sink);
    put(0, 0x0C1C_0C1C);
    loop {
        cortex_m::asm::wfi();
    }
}
