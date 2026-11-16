//! embnet-node：初始化一個 Node，發一筆 5 B 的 normal Subject，取出框寫進 volatile 位址。與 P0 的三個探針做同一件事。
#![no_std]
#![no_main]

#[path = "../../shell.rs"]
mod shell;

use embnet_core::can::{Mtu, NodeId, Priority, SubjectId};
use embnet_node::{Node, PubProtection, Publication};
use static_cell::StaticCell;

type N = Node<1, 1, 1, 1, 16, 4, 1>;
static NODE: StaticCell<N> = StaticCell::new();

#[cortex_m_rt::entry]
fn main() -> ! {
    let (Some(id), Some(p), Some(s)) = (NodeId::new(42), Priority::new(4), SubjectId::new(7509)) else { loop {} };
    let node = NODE.init(N::new(id, Mtu::Classic, 0));
    node.add_publication(Publication { subject: s, priority: p, protection: PubProtection::None });
    let mut buf = [0u8; 64];
    loop {
        let x = shell::input().to_le_bytes();
        let _ = node.publish(s, &[x[0], x[1], x[2], x[3], 0]);
        while let Some((cid, n)) = node.poll_transmit(&mut buf) {
            shell::sink(cid, buf.get(..n).unwrap_or(&[]));
        }
    }
}
