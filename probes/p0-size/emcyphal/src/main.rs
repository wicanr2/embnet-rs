//! emcyphal 0.1.0：初始化一個 Node，發一筆 5 B 的 ByteArray（同 uavcan.primitive.Unstructured），取出框寫進 volatile 位址。
//! emcyphal 只用靜態配置。它是 async API，這裡用 block_on 跑，不含 embassy 執行器。
//! emcyphal 把 CAN ID 的打包留給驅動，這裡以幾行位元運算代替。
#![no_std]
#![no_main]

#[path = "../../shell.rs"]
mod shell;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time::Duration;
use emcyphal::buffer;
use emcyphal::core::SubjectId;
use emcyphal::data_types::ByteArray;
use emcyphal::frame::{DataSpecifier, Header, Mtu};
use emcyphal::node::CoreNode;
use emcyphal::socket::Publisher;
use emcyphal_core::{NodeId, Priority, PrioritySet};
use static_cell::StaticCell;

struct NullDriver;
impl embassy_time_driver::Driver for NullDriver {
    fn now(&self) -> u64 {
        shell::input() as u64
    }
    fn schedule_wake(&self, _at: u64, _waker: &core::task::Waker) {}
}
embassy_time_driver::time_driver_impl!(static DRIVER: NullDriver = NullDriver);

fn pack(h: &Header) -> u32 {
    let p = u8::from(h.priority) as u32;
    let src = h.source.map(|n| u8::from(n) as u32).unwrap_or(0);
    match h.data_spec {
        DataSpecifier::Message(s) => (p << 26) | ((u16::from(s) as u32) << 8) | src,
        _ => 0,
    }
}

static NODE: StaticCell<CoreNode<CriticalSectionRawMutex>> = StaticCell::new();
static BUF: StaticCell<buffer::tx_msg::Blocking<ByteArray>> = StaticCell::new();

#[cortex_m_rt::entry]
fn main() -> ! {
    let node = NODE.init(CoreNode::new(NodeId::new(42).unwrap(), 0));
    let (hub, link) = node.split();
    let (_, _, mut tx) = link.split();

    let buf = BUF.init(Default::default());
    let mut publisher = Publisher::create(
        hub,
        buf,
        SubjectId::new(7509).unwrap(),
        Priority::Nominal,
        Duration::from_secs(1),
        false,
    )
    .unwrap();

    embassy_futures::block_on(async {
        loop {
            let x = shell::input().to_le_bytes();
            let msg = ByteArray { bytes: heapless::Vec::from_slice(&[x[0], x[1], x[2], x[3], 0]).unwrap_or_default() };
            publisher.push(msg).await;
            let f = tx.pop(PrioritySet::new_eq(Priority::Nominal), Mtu::Classic).await;
            shell::sink(pack(&f.header), f.data.as_ref());
        }
    })
}
