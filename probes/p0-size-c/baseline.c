/* 不碰協定的外殼，與 Rust 的 size-baseline 對應。 */
#include <stdint.h>
#include <stddef.h>
void     sink(uint32_t id, const void* data, size_t len);
uint32_t input(void);

int main(void)
{
    for (;;) {
        const uint32_t x = input();
        sink(x, &x, sizeof(x));
    }
}
