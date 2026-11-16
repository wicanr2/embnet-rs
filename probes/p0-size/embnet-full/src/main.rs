//! embnet-node 的完整路徑：protected 發佈（E2E-32）、訂閱與接收、Heartbeat、E2E Sync、Service。
#![no_std]
#![no_main]

#[path = "../../shell.rs"]
mod shell;

use embnet_core::can::{Mtu, NodeId, Priority, ServiceId, SubjectId};
use embnet_core::e2e::StateParams;
use embnet_node::{Event, Node, PubProtection, Publication, SubProtection, Subscription};
use static_cell::StaticCell;

type N = Node<4, 4, 4, 4, 64, 16, 2>;
static NODE: StaticCell<N> = StaticCell::new();

#[cortex_m_rt::entry]
fn main() -> ! {
    let (Some(id), Some(p), Some(s1), Some(s2), Some(svc), Some(srv)) =
        (NodeId::new(42), Priority::new(4), SubjectId::new(100), SubjectId::new(200), ServiceId::new(10), NodeId::new(3))
    else {
        loop {}
    };
    let node = NODE.init(N::new(id, Mtu::Classic, 0));
    node.add_publication(Publication { subject: s1, priority: p, protection: PubProtection::E2e32 { type_hash: 0x1234 } });
    node.add_subscription(Subscription {
        subject: s2,
        protection: SubProtection::E2e32 { type_hash: 0x5678, max_delta: 3, deadline: 50_000, params: StateParams::default() },
        tid_timeout: 100_000,
    });
    let mut buf = [0u8; 64];
    let mut now = 0u64;
    loop {
        now += u64::from(shell::input());
        node.handle_timeout(now);
        let x = shell::input().to_le_bytes();
        let _ = node.publish(s1, &[x[0], x[1]]);
        if x[0] == 0x55 {
            let _ = node.request(now, svc, srv, p, 100_000, &[x[1]]);
        }
        while let Some((cid, n)) = node.poll_transmit(&mut buf) {
            shell::sink(cid, buf.get(..n).unwrap_or(&[]));
        }
        let rid = shell::input();
        let len = (x[2] % 9) as usize;
        let frame = [x[0], x[1], x[2], x[3], x[0], x[1], x[2], x[3]];
        match node.on_frame(now, rid, frame.get(..len).unwrap_or(&[])) {
            Some(Event::Message { payload, .. }) => shell::sink(1, payload),
            Some(Event::Request { service, source, transfer_id, .. }) => {
                let _ = node.respond(service, source, transfer_id, p, &[0]);
            }
            Some(_) => shell::sink(2, &[]),
            None => {}
        }
    }
}
