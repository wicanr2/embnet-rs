/* C 探針共用的裸機外殼：向量表、reset、volatile 輸出。與 Rust 的 shell.rs 對應。 */
#include <stdint.h>
#include <stddef.h>

extern uint32_t _sidata, _sdata, _edata, _sbss, _ebss, _estack;
int main(void);

void reset_handler(void)
{
    uint32_t* src = &_sidata;
    for (uint32_t* dst = &_sdata; dst < &_edata;) { *dst++ = *src++; }
    for (uint32_t* dst = &_sbss; dst < &_ebss;) { *dst++ = 0; }
    (void) main();
    for (;;) {}
}

static void default_handler(void) { for (;;) {} }

__attribute__((section(".vector_table"), used))
const void* const vector_table[16] = {
    &_estack, (void*) reset_handler,
    (void*) default_handler, (void*) default_handler, (void*) default_handler,
    (void*) default_handler, (void*) default_handler, 0, 0, 0, 0,
    (void*) default_handler, (void*) default_handler, 0,
    (void*) default_handler, (void*) default_handler,
};

static volatile uint8_t sink_buf[16];

__attribute__((noinline)) void sink(uint32_t id, const void* data, size_t len)
{
    *(volatile uint32_t*) (void*) sink_buf = id;
    for (size_t i = 0; (i < len) && (i < 12U); i++) { sink_buf[4U + i] = ((const uint8_t*) data)[i]; }
}

__attribute__((noinline)) uint32_t input(void) { return *(volatile uint32_t*) (void*) sink_buf; }

/* 標準函式庫不連結；libcanard 用到的 assert 失敗時停住。 */
void __assert_func(const char* f, int l, const char* fn, const char* e)
{
    (void) f; (void) l; (void) fn; (void) e;
    for (;;) {}
}
