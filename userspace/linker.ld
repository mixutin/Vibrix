ENTRY(_start)

PHDRS
{
  text PT_LOAD FLAGS(5);
  rodata PT_LOAD FLAGS(4);
  data PT_LOAD FLAGS(6);
}

SECTIONS
{
  . = 0x00400000;

  .text ALIGN(0x1000) :
  {
    *(.text .text.*)
  } :text

  /*
   * The kernel ELF sink reserves one unmapped guard page on both sides of
   * every PT_LOAD payload. Keep two whole pages between independently mapped
   * segments so the previous upper guard and the next lower guard are distinct.
   */
  . = ALIGN(0x1000) + 0x2000;
  .rodata ALIGN(0x1000) :
  {
    *(.rodata .rodata.*)
    *(.srodata .srodata.*)
  } :rodata

  . = ALIGN(0x1000) + 0x2000;
  .data ALIGN(0x1000) :
  {
    *(.data .data.*)
    *(.sdata .sdata.*)
    *(.bss .bss.*)
    *(COMMON)
  } :data

  /DISCARD/ :
  {
    *(.eh_frame .eh_frame_hdr)
    *(.comment)
    *(.note .note.*)
  }
}
