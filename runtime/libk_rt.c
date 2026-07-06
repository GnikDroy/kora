#include <stdio.h>
#include <stdlib.h>

extern long k_main(void);

int main(void) { return (int)k_main(); }

void print_int(long x) { printf("%ld\n", x); }
void print_real(double x) { printf("%g\n", x); }
void print_char(unsigned char c) { putchar(c); }
void print_bool(_Bool b) { puts(b ? "true" : "false"); }
void k_exit(long code) { exit((int)code); }
