//! P0 的 Renode 檢查韌體（docs/plans/004 第六節）。結果寫進 P0_RESULT，由 Renode 的 monitor 讀出：
//!   [0] CAN3 loopback：1 = 收到的框與送出的一致
//!   [1..4] 依 ID 0x300、0x200、0x100 的順序連續送出後，收到的 ID 順序
//!   [4] DWT CYCCNT 在 100 ms 內的增量乘以 10，也就是推算的時脈 Hz
//!   [5] 0xD0E0_D0E0 表示跑完
#![no_std]
#![no_main]

use cortex_m::peripheral::{DCB, DWT};
use embassy_executor::Spawner;
use embassy_stm32::can::filter::Mask32;
use embassy_stm32::can::{
    Can, Fifo, Frame, Rx0InterruptHandler, Rx1InterruptHandler, SceInterruptHandler, TxInterruptHandler,
};
use embassy_stm32::peripherals::CAN3;
use embassy_stm32::bind_interrupts;
use embassy_time::Timer;

bind_interrupts!(struct Irqs {
    CAN3_RX0 => Rx0InterruptHandler<CAN3>;
    CAN3_RX1 => Rx1InterruptHandler<CAN3>;
    CAN3_SCE => SceInterruptHandler<CAN3>;
    CAN3_TX => TxInterruptHandler<CAN3>;
});

#[unsafe(no_mangle)]
static mut P0_RESULT: [u32; 8] = [0; 8];

fn put(i: usize, v: u32) {
    unsafe { core::ptr::write_volatile((&raw mut P0_RESULT as *mut u32).add(i), v) }
}

fn raw_id(f: &Frame) -> u32 {
    match f.id() {
        embedded_can::Id::Extended(e) => e.as_raw(),
        embedded_can::Id::Standard(s) => s.as_raw() as u32,
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    put(5, 0xBAD0_BAD0);
    loop {}
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_stm32::init(Default::default());

    let mut cp = unsafe { cortex_m::Peripherals::steal() };
    cp.DCB.enable_trace();
    DWT::unlock();
    cp.DWT.enable_cycle_counter();
    let _ = DCB::is_debugger_attached();

    let mut can = Can::new(p.CAN3, p.PA8, p.PA15, Irqs);
    can.modify_filters().enable_bank(0, Fifo::Fifo0, Mask32::accept_all());
    can.modify_config().set_loopback(true).set_silent(true).set_bitrate(1_000_000);
    can.enable().await;

    // 1. loopback
    let data = [1u8, 2, 3, 4, 5, 6, 7, 8];
    let tx = Frame::new_extended(0x107D_552A, &data).unwrap();
    can.write(&tx).await;
    let ok = match can.read().await {
        Ok(env) => raw_id(&env.frame) == 0x107D_552A && env.frame.data() == data,
        Err(_) => false,
    };
    put(0, ok as u32);

    // 2. 送出順序
    for id in [0x300u32, 0x200, 0x100] {
        can.write(&Frame::new_extended(id, &[0]).unwrap()).await;
    }
    for i in 0..3 {
        if let Ok(env) = can.read().await {
            put(1 + i, raw_id(&env.frame));
        }
    }

    // 3. DWT 頻率
    let c0 = DWT::cycle_count();
    Timer::after_millis(100).await;
    let c1 = DWT::cycle_count();
    put(4, c1.wrapping_sub(c0).wrapping_mul(10));

    put(5, 0xD0E0_D0E0);
    loop {
        Timer::after_secs(1).await;
    }
}
