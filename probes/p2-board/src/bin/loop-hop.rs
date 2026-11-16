//! 實板 loopback：Node、Bridge 與真實的 bxCAN 一起跑，量發佈到接收端交出的延遲（含 1 Mbit/s 的 Wire time）。
//! bxCAN 的 loopback 模式仍依設定的位元時序序列化，所以 Wire time 是真的；silent 模式不驅動 TX 腳，不需要收發器。
//!
//! 階段 A：Node A ─CAN1 loopback→ Node B。
//! 階段 B：Node A ─CAN1 loopback→ Bridge ─CAN3 loopback→ Node B。
//!
//! protected Subject 100（E2E-32，2 B 資料加 5 B E2E 欄位，單框）每 2 ms 一筆，各 10,000 筆。驅動政策：控制器的發送端空閒時才交下一框，
//! 同一時間只有一框在 mailbox 裡，保證同一 session 不亂序（`003` K-29）。
//! 結果：P3_RESULT 與兩個 1 µs 一格的直方圖 HIST_A、HIST_B（2048 格，最後一格是溢位）。
#![no_std]
#![no_main]

use cortex_m::peripheral::DWT;
use embassy_executor::Spawner;
use embassy_stm32::bind_interrupts;
use embassy_stm32::can::filter::Mask32;
use embassy_stm32::can::{Can, Fifo, Frame, Rx0InterruptHandler, Rx1InterruptHandler, SceInterruptHandler, TxInterruptHandler};
use embassy_stm32::peripherals::{CAN1, CAN3};
use embnet_bridge::{Bridge, DropPolicy, LinkKind, Port, Rule, TxOut};
use embnet_core::can::{Mtu, NodeId, Priority, SubjectId};
use embnet_core::e2e::{SYNC_SUBJECT_ID, StateParams, Status};
use embnet_node::{Event, Node, PubProtection, Publication, SubProtection, Subscription};

bind_interrupts!(struct Irqs1 {
    CAN1_RX0 => Rx0InterruptHandler<CAN1>;
    CAN1_RX1 => Rx1InterruptHandler<CAN1>;
    CAN1_SCE => SceInterruptHandler<CAN1>;
    CAN1_TX => TxInterruptHandler<CAN1>;
});
bind_interrupts!(struct Irqs3 {
    CAN3_RX0 => Rx0InterruptHandler<CAN3>;
    CAN3_RX1 => Rx1InterruptHandler<CAN3>;
    CAN3_SCE => SceInterruptHandler<CAN3>;
    CAN3_TX => TxInterruptHandler<CAN3>;
});

const N: u32 = 10_000;
const PERIOD_US: u64 = 2_000;
const HZ: u64 = 216_000_000;
const TH: u64 = 0x4FF0_0AF1_D812_8708;

/// [0] 標記 0x0B0B_0B0B；[1] 進度；[2..5] 階段 A：送出、收到、E2E 不良、最大週期數；
/// [6..9] 階段 B：同上；[10] Bridge forwarded；[11] no_route；[12] local；[13] queue_dropped；[14] rate_limited；[15] 初始化進度
#[unsafe(no_mangle)]
static mut P3_RESULT: [u32; 16] = [0; 16];
#[unsafe(no_mangle)]
static mut HIST_A: [u16; 2048] = [0; 2048];
#[unsafe(no_mangle)]
static mut HIST_B: [u16; 2048] = [0; 2048];

fn put(i: usize, v: u32) {
    unsafe { core::ptr::write_volatile((&raw mut P3_RESULT as *mut u32).add(i), v) }
}
fn get(i: usize) -> u32 {
    unsafe { core::ptr::read_volatile((&raw const P3_RESULT as *const u32).add(i)) }
}
fn hist(b: bool, us: u32) {
    let i = (us as usize).min(2047);
    unsafe {
        let p = if b { (&raw mut HIST_B as *mut u16).add(i) } else { (&raw mut HIST_A as *mut u16).add(i) };
        core::ptr::write_volatile(p, core::ptr::read_volatile(p).saturating_add(1));
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    put(0, 0xBAD0_BAD0);
    loop {}
}

type Nd = Node<2, 2, 2, 4, 64, 16, 2>;
type Br = Bridge<2, 4, 16, 4, 128>;

struct Clock {
    last: u32,
    cyc: u64,
}
impl Clock {
    fn now_cyc(&mut self) -> u64 {
        let c = DWT::cycle_count();
        self.cyc += u64::from(c.wrapping_sub(self.last));
        self.last = c;
        self.cyc
    }
    fn now_us(&mut self) -> u64 {
        self.now_cyc() / (HZ / 1_000_000)
    }
}

fn raw_id(f: &Frame) -> u32 {
    match f.id() {
        embedded_can::Id::Extended(e) => e.as_raw(),
        embedded_can::Id::Standard(s) => u32::from(s.as_raw()),
    }
}

fn nodes() -> (Nd, Nd) {
    let (a_id, b_id, s, p) = (NodeId::new(10).unwrap(), NodeId::new(20).unwrap(), SubjectId::new(100).unwrap(), Priority::new(2).unwrap());
    let mut a = Nd::new(a_id, Mtu::Classic, 0);
    a.add_publication(Publication { subject: s, priority: p, protection: PubProtection::E2e32 { type_hash: TH } });
    let mut b = Nd::new(b_id, Mtu::Classic, 0);
    b.add_subscription(Subscription {
        subject: s,
        protection: SubProtection::E2e32 { type_hash: TH, max_delta: 3, deadline: 50_000, params: StateParams::default() },
        tid_timeout: 100_000,
    });
    (a, b)
}

#[embassy_executor::main]
async fn main(_s: Spawner) {
    put(15, 0x100);
    let p = embassy_stm32::init(p2_board::config_216mhz());
    p2_board::enable_art();
    put(15, 0x200);
    let mut cp = unsafe { cortex_m::Peripherals::steal() };
    cp.SCB.enable_icache();
    cp.DCB.enable_trace();
    DWT::unlock();
    cp.DWT.enable_cycle_counter();
    let mut clk = Clock { last: DWT::cycle_count(), cyc: 0 };

    let mut can3 = Can::new(p.CAN3, p.PA8, p.PA15, Irqs3);
    // embassy-stm32 0.6.0 的 Can::new 會設 MCR.TTCM（為了接收時戳）。CAN3 在 TTCM 開啟時離不開 sleep（實測 MSR.SLAK 停在 1），
    // 我們不用接收時戳，在初始化模式中把它清掉。見 docs/plans/007 第七節。
    embassy_stm32::pac::CAN3.mcr().modify(|w| w.set_ttcm(false));
    // 沒有收發器時 CANRX 腳是浮接的。離開 sleep 與初始化模式要在 CANRX 上看到 11 個連續的 recessive 位元，
    // loopback 模式也一樣（實測 CAN3 的 PA8 浮接為 0，MSR.RX = 0，停在 sleep）。開內部上拉，讓 RX 保持 recessive。
    embassy_stm32::pac::GPIOA.pupdr().modify(|w| w.set_pupdr(8, embassy_stm32::pac::gpio::vals::Pupdr::PULL_UP));
    can3.modify_filters().enable_bank(0, Fifo::Fifo0, Mask32::accept_all());
    can3.modify_config().set_loopback(true).set_silent(true).set_bitrate(1_000_000);
    put(15, 0x500);
    can3.enable().await;
    put(15, 0x600);
    let mut can1 = Can::new(p.CAN1, p.PD0, p.PD1, Irqs1);
    embassy_stm32::pac::CAN1.mcr().modify(|w| w.set_ttcm(false));
    embassy_stm32::pac::GPIOD.pupdr().modify(|w| w.set_pupdr(0, embassy_stm32::pac::gpio::vals::Pupdr::PULL_UP));
    can1.modify_filters().enable_bank(0, Fifo::Fifo0, Mask32::accept_all());
    can1.modify_config().set_loopback(true).set_silent(true).set_bitrate(1_000_000);
    put(15, 0x300);
    can1.enable().await;
    put(15, 0x400);

    let subject = SubjectId::new(100).unwrap();
    let mut buf = [0u8; 128];
    for phase_b in [false, true] {
        let (mut a, mut b) = nodes();
        let mut br = Br::new([LinkKind::Can(Mtu::Classic), LinkKind::Can(Mtu::Classic)]);
        let src = NodeId::new(10).unwrap();
        let base = Rule { port: Port::Subject(subject), source: src, server: None, ingress: 0, egress: 1, period_us: PERIOD_US, burst: 4, drop: DropPolicy::Newest, deadline_us: Some(50_000) };
        br.add_rule(base, 0);
        br.add_rule(Rule { port: Port::Subject(SubjectId::new(SYNC_SUBJECT_ID).unwrap()), period_us: 1_000_000, burst: 2, drop: DropPolicy::Oldest, deadline_us: None, ..base }, 0);
        let off = if phase_b { 6 } else { 2 };
        let mut t_pub = [0u64; 256];
        let (mut sent, mut got, mut bad, mut maxc) = (0u32, 0u32, 0u32, 0u32);
        let start = clk.now_us();
        let mut next = start + 10_000; // 先讓 Heartbeat 與 Sync 送出
        let end_after = start + 10_000 + u64::from(N) * PERIOD_US + 200_000;
        loop {
            let now = clk.now_us();
            if now > end_after {
                break;
            }
            if now >= a.poll_timeout() {
                a.handle_timeout(now);
            }
            if now >= b.poll_timeout() {
                b.handle_timeout(now);
            }
            if sent < N && now >= next {
                let seq = sent as u16;
                t_pub[usize::from(seq as u8)] = clk.now_cyc();
                if a.publish(subject, &seq.to_le_bytes()).is_ok() {
                    sent += 1;
                }
                next += PERIOD_US;
            }
            // Node A 送上 CAN1：發送端空閒才交下一框
            if can1.is_transmitter_idle()
                && let Some((id, n)) = a.poll_transmit(&mut buf)
                && let Ok(f) = Frame::new_extended(id, &buf[..n])
            {
                let _ = can1.try_write(&f);
            }
            // CAN1 收到的框：階段 A 交給 Node B，階段 B 交給 Bridge
            while let Ok(env) = can1.try_read() {
                let (id, data) = (raw_id(&env.frame), env.frame.data());
                if phase_b {
                    br.on_can(0, clk.now_us(), id, data);
                } else if let Some(e) = b.on_frame(clk.now_us(), id, data) {
                    record(&e, &t_pub, &mut clk, false, &mut got, &mut bad, &mut maxc);
                }
            }
            if phase_b {
                if can3.is_transmitter_idle()
                    && let Some(TxOut::Can { id, len }) = br.poll_transmit(1, clk.now_us(), &mut buf)
                    && let Ok(f) = Frame::new_extended(id, &buf[..len])
                {
                    let _ = can3.try_write(&f);
                }
                while let Ok(env) = can3.try_read() {
                    let (id, data) = (raw_id(&env.frame), env.frame.data());
                    if let Some(e) = b.on_frame(clk.now_us(), id, data) {
                        record(&e, &t_pub, &mut clk, true, &mut got, &mut bad, &mut maxc);
                    }
                }
            }
            put(1, (u32::from(phase_b) << 24) | sent);
        }
        put(off, sent);
        put(off + 1, got);
        put(off + 2, bad);
        put(off + 3, maxc);
        if phase_b {
            let s = br.stats;
            put(10, s.forwarded);
            put(11, s.no_route);
            put(12, s.local);
            put(13, s.queue_dropped);
            put(14, s.rate_limited);
        }
    }
    let _ = get(0);
    put(0, 0x0B0B_0B0B);
    loop {
        cortex_m::asm::wfi();
    }
}

fn record(e: &Event<'_>, t_pub: &[u64; 256], clk: &mut Clock, phase_b: bool, got: &mut u32, bad: &mut u32, maxc: &mut u32) {
    match e {
        Event::Message { payload, e2e, .. } => {
            if !matches!(e2e, Some((Status::Ok | Status::OkSomeLost, _))) {
                *bad += 1;
            }
            let seq = u16::from_le_bytes([payload[0], payload[1]]);
            let dc = clk.now_cyc().saturating_sub(t_pub[usize::from(seq as u8)]);
            let c = u32::try_from(dc).unwrap_or(u32::MAX);
            *maxc = (*maxc).max(c);
            hist(phase_b, c / (HZ / 1_000_000) as u32);
            *got += 1;
        }
        Event::Rejected { .. } => *bad += 1,
        _ => {}
    }
}
