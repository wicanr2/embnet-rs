//! canadensis 0.6.1：初始化一個 Node，發一筆 5 B 的 uavcan.primitive.Unstructured，框寫進 volatile 位址。
//! canadensis 需要配置器（K-35），heap 固定 16 KB，另列於通過行。
#![no_std]
#![no_main]

extern crate alloc;

#[path = "../../shell.rs"]
mod shell;

use core::convert::Infallible;
use core::mem::MaybeUninit;

use canadensis::core::time::{Clock, MicrosecondDuration32, Microseconds32};
use canadensis::core::{Priority, SubjectId};
use canadensis::node::CoreNode;
use canadensis::requester::TransferIdFixedMap;
use canadensis::Node;
use canadensis_can::driver::{ReceiveDriver, TransmitDriver};
use canadensis_can::queue::{ArrayQueue, SingleQueueDriver};
use canadensis_can::{CanNodeId, CanReceiver, CanTransmitter, CanTransport, Frame, Mtu};
use canadensis_data_types::uavcan::primitive::unstructured_1_0::Unstructured;
use embedded_alloc::LlffHeap as Heap;

pub const HEAP_SIZE: usize = 16 * 1024;

#[global_allocator]
static HEAP: Heap = Heap::empty();

#[derive(Clone)]
struct Clk;
impl Clock for Clk {
    fn now(&mut self) -> Microseconds32 {
        Microseconds32::from_ticks(shell::input())
    }
}

struct Drv;
impl TransmitDriver<Clk> for Drv {
    type Error = Infallible;
    fn try_reserve(&mut self, _frames: usize) -> Result<(), canadensis::core::OutOfMemoryError> {
        Ok(())
    }
    fn transmit(&mut self, frame: Frame, _clock: &mut Clk) -> canadensis::core::nb::Result<Option<Frame>, Infallible> {
        shell::sink(u32::from(frame.id()), frame.data());
        Ok(None)
    }
    fn flush(&mut self, _clock: &mut Clk) -> canadensis::core::nb::Result<(), Infallible> {
        Ok(())
    }
}
impl ReceiveDriver<Clk> for Drv {
    type Error = Infallible;
    fn receive(&mut self, _clock: &mut Clk) -> canadensis::core::nb::Result<Frame, Infallible> {
        Err(canadensis::core::nb::Error::WouldBlock)
    }
    fn apply_filters<S>(&mut self, _local_node: Option<CanNodeId>, _subscriptions: S)
    where
        S: IntoIterator<Item = canadensis::core::subscription::Subscription>,
    {
    }
    fn apply_accept_all(&mut self) {}
}

type Queue = SingleQueueDriver<Clk, ArrayQueue<8>, Drv>;
type N = CoreNode<
    Clk,
    CanTransmitter<Clk, Queue>,
    CanReceiver<Clk, Queue>,
    TransferIdFixedMap<CanTransport, 4>,
    Queue,
    4,
    4,
>;

// Node 放在靜態區，讓 .bss 反映它的大小（三個探針一致）。
static NODE: static_cell::StaticCell<N> = static_cell::StaticCell::new();

#[cortex_m_rt::entry]
fn main() -> ! {
    {
        static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
        unsafe { HEAP.init(&raw mut HEAP_MEM as usize, HEAP_SIZE) }
    }

    let node_id = CanNodeId::from_truncating(42);
    let node: &mut N = NODE.init(CoreNode::new(
        Clk,
        node_id,
        CanTransmitter::new(Mtu::Can8),
        CanReceiver::new(node_id),
        SingleQueueDriver::new(ArrayQueue::new(), Drv),
    ));
    let subject = SubjectId::from_truncating(7509);
    node.start_publishing(subject, MicrosecondDuration32::from_ticks(1_000_000), Priority::Nominal.into())
        .ok();

    loop {
        let x = shell::input().to_le_bytes();
        let msg = Unstructured { value: heapless::Vec::from_slice(&[x[0], x[1], x[2], x[3], 0]).unwrap_or_default() };
        node.publish(subject, &msg).ok();
        node.flush().ok();
    }
}
