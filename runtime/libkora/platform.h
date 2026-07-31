#ifndef KORA_PLATFORM_H
#define KORA_PLATFORM_H

#ifdef _WIN32

#include <io.h>
#include <windows.h>

void __kora_sleep_ms(long ms) {
  if (ms > 0) {
    Sleep((DWORD)ms);
  }
}

void __kora_write(const char *buf, long n) {
  _write(1, buf, (unsigned int)n);
}

#else

#include <time.h>
#include <unistd.h>

void __kora_sleep_ms(long ms) {
  if (ms <= 0) {
    return;
  }
  struct timespec duration = {ms / 1000, (ms % 1000) * 1000000L};
  nanosleep(&duration, NULL);
}

void __kora_write(const char *buf, long n) {
  (void)write(1, buf, (size_t)n);
}

#endif

#endif
