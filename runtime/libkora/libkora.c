#include <stdio.h>
#include <stdlib.h>

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

void print(const KoraArray *s) {
    fwrite(s->buf, 1, s->len, stdout);
    putchar('\n');
}

void print_int(long x) { printf("%ld\n", x); }

void print_real(double x) { printf("%g\n", x); }

void print_char(unsigned char c) { putchar(c); }

void print_bool(_Bool b) { puts(b ? "true" : "false"); }
