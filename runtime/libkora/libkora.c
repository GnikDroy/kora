#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#include "array.h"
#include "platform.h"

extern long __kora_main(void);

extern void GC_init(void);

int main(void) {
  GC_init();
  srand((unsigned)time(NULL));
  return (int)__kora_main();
}

_Noreturn void __kora_panic(const char *message) {
  fprintf(stderr, "panic: %s\n", message);
  exit(EXIT_FAILURE);
}

