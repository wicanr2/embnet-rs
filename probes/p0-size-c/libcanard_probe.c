/* libcanard v5.0.0：初始化一個節點，發一筆 5 B 的 Subject（13 位元 Subject-ID，Cyphal v1.0），
   框寫進 volatile 位址。配置器用固定大小的記憶池，大小另列。 */
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include "canard.h"

void     sink(uint32_t id, const void* data, size_t len);
uint32_t input(void);

#define POOL_BLOCKS     32U
#define POOL_BLOCK_SIZE 128U
static _Alignas(8) uint8_t pool[POOL_BLOCKS][POOL_BLOCK_SIZE];
static uint32_t            pool_used; /* 每個位元代表一個區塊 */

static void* pool_alloc(canard_mem_t mem, size_t size)
{
    (void) mem;
    if (size > POOL_BLOCK_SIZE) { return NULL; }
    for (uint32_t i = 0; i < POOL_BLOCKS; i++) {
        if ((pool_used & (1UL << i)) == 0U) { pool_used |= (1UL << i); return pool[i]; }
    }
    return NULL;
}

static void pool_free(canard_mem_t mem, size_t size, void* ptr)
{
    (void) mem; (void) size;
    if (ptr != NULL) { pool_used &= ~(1UL << (uint32_t) (((uint8_t(*)[POOL_BLOCK_SIZE]) ptr) - pool)); }
}

static const canard_mem_vtable_t pool_vtable = { .free = pool_free, .alloc = pool_alloc };

static canard_us_t now(const canard_t* self) { (void) self; return (canard_us_t) input(); }

static bool tx(canard_t* self, void* ctx, canard_us_t deadline, uint_least8_t iface, bool fd,
               uint32_t can_id, canard_bytes_t data)
{
    (void) self; (void) ctx; (void) deadline; (void) iface; (void) fd;
    sink(can_id, data.data, data.size);
    return true;
}

static bool filter(canard_t* self, size_t n, const canard_filter_t* f) { (void) self; (void) n; (void) f; return true; }

static const canard_vtable_t vtable = { .now = now, .tx = tx, .filter = filter };

static canard_t node;

int main(void)
{
    const canard_mem_t     mem = { .vtable = &pool_vtable, .context = NULL };
    const canard_mem_set_t mems = { mem, mem, mem, mem, mem };
    if (!canard_new(&node, &vtable, mems, 1U, 8U, 42U, 0U)) { for (;;) {} }
    (void) canard_set_node_id(&node, 42U);
    uint_least8_t tid = 0;
    for (;;) {
        const uint32_t             x = input();
        const uint8_t              payload[5] = { (uint8_t) x, (uint8_t) (x >> 8), (uint8_t) (x >> 16), (uint8_t) (x >> 24), 0 };
        const canard_bytes_chain_t chain = { .bytes = { .size = sizeof(payload), .data = payload }, .next = NULL };
        (void) canard_publish_13b(&node, (canard_us_t) x + 1000000, 1U, canard_prio_nominal, 7509U, tid++, chain, NULL);
        canard_poll(&node, 1U);
    }
}
