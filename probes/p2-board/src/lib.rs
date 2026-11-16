//! 兩支實板韌體共用的設定：216 MHz 時脈、快取與 ART。
#![no_std]

use embassy_stm32::rcc::{AHBPrescaler, APBPrescaler, Pll, PllMul, PllPDiv, PllPreDiv, PllQDiv, PllSource, Sysclk};

/// HSI 16 MHz ÷ 8 × 216 ÷ 2 = 216 MHz；Q 輸出 48 MHz。APB1 54 MHz、APB2 108 MHz。
pub fn config_216mhz() -> embassy_stm32::Config {
    let mut c = embassy_stm32::Config::default();
    c.rcc.hsi = true;
    c.rcc.pll_src = PllSource::HSI;
    c.rcc.pll = Some(Pll {
        prediv: PllPreDiv::DIV8,
        mul: PllMul::MUL216,
        divp: Some(PllPDiv::DIV2),
        divq: Some(PllQDiv::DIV9),
        divr: None,
    });
    c.rcc.sys = Sysclk::PLL1_P;
    c.rcc.ahb_pre = AHBPrescaler::DIV1;
    c.rcc.apb1_pre = APBPrescaler::DIV4;
    c.rcc.apb2_pre = APBPrescaler::DIV2;
    c
}

/// 開啟快閃記憶體的 ART 加速與預取。
pub fn enable_art() {
    embassy_stm32::pac::FLASH.acr().modify(|w| {
        w.set_arten(true);
        w.set_prften(true);
    });
}
