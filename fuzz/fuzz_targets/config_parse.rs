//! 設定檔：任意文字不得讓 embnet-config panic。
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data)
        && let Ok(cfg) = embnet_config::parse(s)
    {
        let _ = embnet_config::check(&cfg);
    }
});
