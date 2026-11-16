//! datagram 解碼與 UART 的 COBS 接收端：任意位元組都不得 panic；解出的 datagram 重新編碼後要一致。
//! 隨機輸入幾乎過不了 CRC，所以另外把輸入補上正確的 CRC-32C 再解一次（結構化輸入，docs/40-rust-embedded/03 第二節）。
#![no_main]
use embnet_core::crc::crc32c;
use embnet_core::datagram::{self, CobsReceiver};
use libfuzzer_sys::fuzz_target;

fn roundtrip(d: &[u8]) {
    if let Ok((h, p)) = datagram::decode(d) {
        let mut out = [0u8; 2048];
        if let Ok(n) = datagram::encode(&h, p, &mut out) {
            assert_eq!(&out[..n], d);
            let mut framed = [0u8; 2100];
            if let Ok(m) = datagram::cobs_frame(&out[..n], &mut framed) {
                let mut rx = CobsReceiver::<2100>::new();
                let mut got = false;
                for &b in &framed[..m] {
                    if let Some(Ok(x)) = rx.push(b) {
                        assert_eq!(x, d);
                        got = true;
                    }
                }
                assert!(got);
            }
        }
    }
}

fuzz_target!(|data: &[u8]| {
    roundtrip(data);
    if data.len() <= 1500 {
        let mut fixed = data.to_vec();
        fixed.extend_from_slice(&crc32c(data).to_be_bytes());
        roundtrip(&fixed);
    }
    let mut rx = CobsReceiver::<256>::new();
    for &b in data {
        if let Some(Ok(d)) = rx.push(b) {
            let _ = datagram::decode(d);
        }
    }
});
