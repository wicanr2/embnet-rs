//! 任意的框序列餵給 Node（含 protected 與 normal 訂閱、Service、Heartbeat、Sync）：不得 panic。
//! 第一個位元組選模式：偶數是原始框 [時間增量 1 B][CAN ID 4 B LE][長度 1 B][資料]；
//! 奇數是由另一個 Node 正確發出，再依輸入丟框、重複或翻位元，走到重組與 E2E 之後的分支。
#![no_main]
use embnet_core::can::{Mtu, NodeId, Priority, SubjectId};
use embnet_core::e2e::StateParams;
use embnet_node::{Node, PubProtection, Publication, SubProtection, Subscription};
use libfuzzer_sys::fuzz_target;

type N = Node<2, 4, 4, 4, 64, 16, 2>;

fn receiver() -> Option<N> {
    let mut n: N = Node::new(NodeId::new(20)?, Mtu::Classic, 0);
    for (sid, prot) in [(100u16, true), (200, false), (8000, false)] {
        let protection = if prot {
            SubProtection::E2e32 { type_hash: 0x1234, max_delta: 3, deadline: 50_000, params: StateParams::default() }
        } else {
            SubProtection::None
        };
        n.add_subscription(Subscription { subject: SubjectId::new(sid)?, protection, tid_timeout: 100_000 });
    }
    Some(n)
}

fuzz_target!(|data: &[u8]| {
    let Some(mut n) = receiver() else { return };
    let Some((&mode, rest)) = data.split_first() else { return };
    let mut out = [0u8; 64];
    let mut now = 0u64;
    if mode & 1 == 0 {
        let mut rest = rest;
        while rest.len() >= 6 {
            now += u64::from(rest[0]) * 1000;
            let id = u32::from_le_bytes([rest[1], rest[2], rest[3], rest[4]]);
            let len = usize::from(rest[5] % 65).min(rest.len() - 6);
            let _ = n.on_frame(now, id, &rest[6..6 + len]);
            if rest[0] & 0x80 != 0 {
                n.handle_timeout(now);
                while n.poll_transmit(&mut out).is_some() {}
            }
            rest = &rest[6 + len..];
        }
        return;
    }
    let (Some(src), Some(p2), Some(p4), Some(s100), Some(s200)) =
        (NodeId::new(10), Priority::new(2), Priority::new(4), SubjectId::new(100), SubjectId::new(200))
    else {
        return;
    };
    let mut tx: N = Node::new(src, Mtu::Classic, 0);
    tx.add_publication(Publication { subject: s100, priority: p2, protection: PubProtection::E2e32 { type_hash: 0x1234 } });
    tx.add_publication(Publication { subject: s200, priority: p4, protection: PubProtection::None });
    tx.handle_timeout(0);
    // 每筆：[控制 1 B][長度 1 B][資料]。控制位元：0 選 Subject，1 丟框，2 重複，3 翻位元，4 跑 handle_timeout
    let mut rest = rest;
    while rest.len() >= 2 {
        let ctl = rest[0];
        let len = usize::from(rest[1] % 40).min(rest.len() - 2);
        let payload = &rest[2..2 + len];
        rest = &rest[2 + len..];
        now += 10_000;
        let subject = if ctl & 1 == 0 { s100 } else { s200 };
        let _ = tx.publish(subject, payload);
        if ctl & 0x10 != 0 {
            tx.handle_timeout(now);
            n.handle_timeout(now);
        }
        let mut k = 0u32;
        while let Some((id, len)) = tx.poll_transmit(&mut out) {
            k += 1;
            let mut f = out;
            if ctl & 0x02 != 0 && k == 1 {
                continue;
            }
            if ctl & 0x08 != 0 && len > 0 {
                let bit = usize::from(ctl >> 5) + (k as usize * 3);
                f[bit / 8 % len] ^= 1 << (bit % 8);
            }
            let _ = n.on_frame(now, id, &f[..len]);
            if ctl & 0x04 != 0 {
                let _ = n.on_frame(now, id, &f[..len]);
            }
        }
    }
});
