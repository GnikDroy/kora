#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#include "array.h"
#include "net.h"
#include "platform.h"

extern long __kora_main(void);

extern void GC_init(void);

static int kora_argc;
static char **kora_argv;

int __kora_argc(void) { return kora_argc; }

const char *__kora_argv(int i) {
  if (i < 0 || i >= kora_argc) {
    return NULL;
  }
  return kora_argv[i];
}

int main(int argc, char **argv) {
  GC_init();
  __kora_net_init();
  atexit(__kora_net_cleanup);
  kora_argc = argc;
  kora_argv = argv;
  srand((unsigned)time(NULL));
  return (int)__kora_main();
}

_Noreturn void __kora_panic(const char *message) {
  fprintf(stderr, "panic: %s\n", message);
  exit(EXIT_FAILURE);
}

