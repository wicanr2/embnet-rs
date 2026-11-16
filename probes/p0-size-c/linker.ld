/* STM32F767ZI：與 Rust 探針相同的記憶體配置 */
MEMORY
{
  FLASH (rx)  : ORIGIN = 0x08000000, LENGTH = 2048K
  RAM   (rwx) : ORIGIN = 0x20000000, LENGTH = 512K
}
ENTRY(reset_handler)
SECTIONS
{
  .vector_table ORIGIN(FLASH) : { KEEP(*(.vector_table)) } > FLASH
  .text : { *(.text .text.*) } > FLASH
  .rodata : { *(.rodata .rodata.*) } > FLASH
  .data : { _sdata = .; *(.data .data.*) _edata = .; } > RAM AT > FLASH
  _sidata = LOADADDR(.data);
  .bss (NOLOAD) : { _sbss = .; *(.bss .bss.* COMMON) _ebss = .; } > RAM
  _estack = ORIGIN(RAM) + LENGTH(RAM);
  /DISCARD/ : { *(.ARM.exidx*) *(.ARM.attributes) *(.comment) }
}
