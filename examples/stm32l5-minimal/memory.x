MEMORY
{
  /* The final four 2 KiB pages in flash bank 2 are unused by this example. */
  FLASH (rx)  : ORIGIN = 0x08000000, LENGTH = 248K
  RAM   (rwx) : ORIGIN = 0x20000000, LENGTH = 128K
}
