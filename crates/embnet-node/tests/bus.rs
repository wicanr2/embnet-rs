//! 單一匯流排的處理時間分布（`docs/plans/001` 第八節）。模擬環境：兩個 Node 在同一個行程，沒有 Wire time，
//! 量的是「publish 到接收端交出事件」的軟體處理時間。要用 release 建置：`cargo test --release -- --ignored`。
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::arithmetic_side_effects, missing_docs)]

use std::time::Instant;

use embnet_core::can::{Mtu, NodeId, Priority, SubjectId};
use embnet_core::e2e::StateParams;
use embnet_node::{Event, Node, PubProtection, Publication, SubProtection, Subscription};

type N = Node<4, 4, 4, 8, 64, 32, 4>;

fn measure(protected: bool, len: usize, samples: usize) -> Vec<u64> {
    let s = SubjectId::new(100).unwrap();
    let th = 0x1234;
    let mut a = N::new(NodeId::new(10).unwrap(), Mtu::Classic, 0);
    let mut b = N::new(NodeId::new(20).unwrap(), Mtu::Classic, 0);
    let (pp, sp) = if protected {
        (PubProtection::E2e32 { type_hash: th }, SubProtection::E2e32 { type_hash: th, max_delta: 3, deadline: u64::MAX / 4, params: StateParams::default() })
    } else {
        (PubProtection::None, SubProtection::None)
    };
    a.add_publication(Publication { subject: s, priority: Priority::new(2).unwrap(), protection: pp });
    b.add_subscription(Subscription { subject: s, protection: sp, tid_timeout: 1 });
    let mut buf = [0u8; 64];
    a.handle_timeout(0);
    while let Some((id, n)) = a.poll_transmit(&mut buf) {
        b.on_frame(0, id, &buf[..n]);
    }
    let payload = vec![0x5Au8; len];
    let mut out = Vec::with_capacity(samples);
    for i in 0..samples + 1000 {
        let now = (i as u64 + 1) * 10_000;
        let t0 = Instant::now();
        a.publish(s, &payload).unwrap();
        let mut got = false;
        while let Some((id, n)) = a.poll_transmit(&mut buf) {
            if let Some(Event::Message { .. }) = b.on_frame(now, id, &buf[..n]) {
                got = true;
            }
        }
        let dt = t0.elapsed().as_nanos() as u64;
        assert!(got);
        if i >= 1000 {
            out.push(dt); // 前 1000 筆當暖機，不計
        }
    }
    out.sort_unstable();
    out
}

fn line(name: &str, v: &[u64]) {
    let n = v.len() as f64;
    let q = |p: f64| v[((p * n).ceil() as usize).saturating_sub(1).min(v.len() - 1)];
    // p99 的 95% 信賴區間：排序統計量的常態近似（docs/40-rust-embedded/04 第二節）
    let k = 0.99 * n;
    let w = 1.96 * (n * 0.99 * 0.01).sqrt();
    let lo = v[((k - w).floor() as usize).saturating_sub(1)];
    let hi = v[((k + w).ceil() as usize).min(v.len()) - 1];
    let us = |x: u64| x as f64 / 1000.0;
    println!(
        "EMBNET_BUS PASS link=can-sim env=host-sim case={name} samples={} p50_us={:.2} p99_us={:.2} p99_ci_us={:.2}-{:.2} max_us={:.2}",
        v.len(),
        us(q(0.5)),
        us(q(0.99)),
        us(lo),
        us(hi),
        us(*v.last().unwrap())
    );
}

#[test]
#[ignore = "量測用，要 release 建置"]
fn bus_processing_time() {
    let n = 100_000;
    line("normal_2B_single_frame", &measure(false, 2, n));
    line("protected_2B_single_frame", &measure(true, 2, n));
    line("normal_20B_multi_frame", &measure(false, 20, n));
}
