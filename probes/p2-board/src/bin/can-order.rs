//! bxCAN 的送出順序（`003` K-29）：CAN1 用內部 loopback 加 silent，不需要收發器。
//! 結果寫進 P2_RESULT：
//!   [0] 標記 0x0CA0_0CA0
//!   [1..4]  TXFP=0，在初始化模式先放 0x300、0x100、0x200 三框，離開初始化後收到的順序
//!   [4..7]  TXFP=1，同上
//!   [7..10] TXFP=0，啟用後依序寫入 0x300、0x200、0x100
//!   [10..13] TXFP=1，同上
//!   [13] 放入 mailbox 失敗的次數
//!   [14] 進度：(slot << 8) | 階段，用來找出卡住的位置
#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_stm32::can::filter::Mask32;
use embassy_stm32::can::{Can, Fifo, Frame, Rx0InterruptHandler, Rx1InterruptHandler, SceInterruptHandler, TxInterruptHandler};
use embassy_stm32::peripherals::CAN1;
use embassy_stm32::{Peri, bind_interrupts};
use embassy_time::{Duration, with_timeout};

bind_interrupts!(struct Irqs {
    CAN1_RX0 => Rx0InterruptHandler<CAN1>;
    CAN1_RX1 => Rx1InterruptHandler<CAN1>;
    CAN1_SCE => SceInterruptHandler<CAN1>;
    CAN1_TX => TxInterruptHandler<CAN1>;
});

#[unsafe(no_mangle)]
static mut P2_RESULT: [u32; 16] = [0; 16];

fn put(i: usize, v: u32) {
    unsafe { core::ptr::write_volatile((&raw mut P2_RESULT as *mut u32).add(i), v) }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    put(0, 0xBAD0_BAD0);
    loop {}
}

fn raw(f: &Frame) -> u32 {
    match f.id() {
        embedded_can::Id::Extended(e) => e.as_raw(),
        embedded_can::Id::Standard(s) => u32::from(s.as_raw()),
    }
}

async fn run(
    can1: Peri<'_, CAN1>,
    rx: Peri<'_, embassy_stm32::peripherals::PD0>,
    tx: Peri<'_, embassy_stm32::peripherals::PD1>,
    txfp: bool,
    prefill: bool,
    slot: usize,
) {
    put(14, (slot as u32) << 8 | 1);
    let mut can = Can::new(can1, rx, tx, Irqs);
    can.modify_filters().enable_bank(0, Fifo::Fifo0, Mask32::accept_all());
    can.modify_config().set_loopback(true).set_silent(true).set_bitrate(1_000_000);
    can.set_tx_fifo_scheduling(txfp);
    let ids: [u32; 3] = if prefill { [0x300, 0x100, 0x200] } else { [0x300, 0x200, 0x100] };
    let mut fails = 0;
    if prefill {
        for id in ids {
            if let Ok(f) = Frame::new_extended(id, &[0]) {
                if can.try_write(&f).is_err() {
                    fails += 1;
                }
            }
        }
        can.enable().await;
    } else {
        put(14, (slot as u32) << 8 | 2);
        can.enable().await;
        put(14, (slot as u32) << 8 | 3);
        for id in ids {
            if let Ok(f) = Frame::new_extended(id, &[0]) {
                if can.try_write(&f).is_err() {
                    fails += 1;
                }
            }
        }
        put(14, (slot as u32) << 8 | 4);
    }
    put(14, (slot as u32) << 8 | 5);
    for i in 0..3 {
        if let Ok(Ok(env)) = with_timeout(Duration::from_millis(100), can.read()).await {
            put(slot + i, raw(&env.frame));
        }
    }
    put(13, fails);
}

#[embassy_executor::main]
async fn main(_s: Spawner) {
    let mut p = embassy_stm32::init(p2_board::config_216mhz());
    run(p.CAN1.reborrow(), p.PD0.reborrow(), p.PD1.reborrow(), false, true, 1).await;
    run(p.CAN1.reborrow(), p.PD0.reborrow(), p.PD1.reborrow(), true, true, 4).await;
    run(p.CAN1.reborrow(), p.PD0.reborrow(), p.PD1.reborrow(), false, false, 7).await;
    run(p.CAN1.reborrow(), p.PD0.reborrow(), p.PD1.reborrow(), true, false, 10).await;
    put(0, 0x0CA0_0CA0);
    loop {
        embassy_time::Timer::after_secs(1).await;
    }
}
