//! E2E 接收判定與 Sync 解碼：任意輸入不得 panic；交給應用的資料必定是輸入的一段。
//! 除了直接餵位元組，也用輸入決定序號與資料，先正確編碼再做一處修改，走到 CRC 之後的分支。
#![no_main]
use embnet_core::e2e::{self, DataId, SeqReceiver, Status};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 8 {
        return;
    }
    let last = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let max_delta = data[4] % 8;
    let id = DataId { subject_id: 100, source: 10, type_hash: 0x1234 };
    let wire = &data[8..];

    // 1. 直接餵位元組
    let mut rx = SeqReceiver::new(max_delta);
    if data[5] & 1 != 0 {
        rx.sync(last);
    }
    let (status, d) = rx.check(&id, wire);
    if let Some(d) = d {
        assert!(status.deliver());
        assert_eq!(d.len() + 5, wire.len());
    }

    // 2. 先正確編碼，再依 data[6] 決定要不要翻一個位元
    let seq = last.wrapping_add(u32::from(data[5]));
    let mut buf = [0u8; 300];
    let payload = &wire[..wire.len().min(256)];
    if let Some(n) = e2e::encode(&id, seq, payload, &mut buf) {
        let flip = usize::from(data[6]);
        let mutated = flip < n * 8;
        if mutated {
            buf[flip / 8] ^= 1 << (flip % 8);
        }
        let mut rx = SeqReceiver::new(max_delta);
        rx.sync(last.wrapping_add(1));
        let (st, d) = rx.check(&id, &buf[..n]);
        if !mutated {
            let d0 = data[5];
            let expect = match d0 {
                0 => Status::Repeated,
                1 => Status::Ok,
                x if x <= max_delta => Status::OkSomeLost,
                _ => Status::WrongSequence,
            };
            // sync(last+1) 讓接收端的 last = last，所以 d = data[5]
            assert_eq!(st, expect);
            if st.deliver() {
                assert_eq!(d, Some(payload));
            }
        } else if let Some(d) = d {
            // 改了一個位元仍被接受，只可能是改到 seq_low 而剛好落在可接受的範圍：這時 CRC 不會過
            panic!("單一位元錯誤被接受：flip={flip} len={} d={}", n, d.len());
        }
    }

    if let Some(it) = e2e::sync_decode(10, wire) {
        for _ in it {}
    }
});
