MEMORY
{
  /* NOTE K = KiBi = 1024 bytes */
  /* FLASH is 128k, but last two 2k pages are reserved for config data */
  FLASH  (rx)  : ORIGIN = 0x08000000, LENGTH = 124K
  RAM    (rwx) : ORIGIN = 0x20000000, LENGTH = 128K
}
