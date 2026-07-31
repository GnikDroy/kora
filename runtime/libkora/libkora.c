#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#include "array.h"

extern long __kora_main(void);

extern void GC_init(void);

int main(void) {
  GC_init();
  return (int)__kora_main();
}

void __kora_exit(long code) { exit((int)code); }

_Noreturn void __kora_panic(const char *message) {
  fprintf(stderr, "panic: %s\n", message);
  exit(EXIT_FAILURE);
}

void sleep(long ms) {
  if (ms <= 0) {
    return;
  }
  struct timespec duration = {ms / 1000, (ms % 1000) * 1000000L};
  nanosleep(&duration, NULL);
}

void __kora_write(const char *s) {
  fputs(s, stdout);
  fflush(stdout);
}

int __kora_getchar(void) { return getchar(); }

double __kora_random(void) {
  static int seeded = 0;
  if (!seeded) {
    struct timespec now;
    clock_gettime(CLOCK_REALTIME, &now);
    srand48(now.tv_nsec ^ now.tv_sec);
    seeded = 1;
  }
  return drand48();
}
