// 三個 bin 共用的裸機外殼：panic 時停住，輸出寫進 volatile 位址，避免被最佳化掉。
use core::panic::PanicInfo;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {
        cortex_m::asm::udf();
    }
}

static mut SINK: [u8; 16] = [0; 16];

/// 把一個框的 ID 與資料寫進 volatile 位址。
#[inline(never)]
pub fn sink(id: u32, data: &[u8]) {
    let p = &raw mut SINK as *mut u8;
    unsafe {
        core::ptr::write_volatile(p as *mut u32, id);
        for (i, b) in data.iter().take(12).enumerate() {
            core::ptr::write_volatile(p.add(4 + i), *b);
        }
    }
}

/// 外部輸入，避免常數被折疊。
#[inline(never)]
pub fn input() -> u32 {
    let p = &raw const SINK as *const u32;
    unsafe { core::ptr::read_volatile(p) }
}
