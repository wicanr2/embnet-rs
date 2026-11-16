//! 不碰協定的外殼：同樣的向量表、reset、panic handler 與 volatile 輸出。
#![no_std]
#![no_main]

#[path = "../../shell.rs"]
mod shell;

#[cortex_m_rt::entry]
fn main() -> ! {
    loop {
        let x = shell::input();
        shell::sink(x, &x.to_le_bytes());
    }
}
