#ifndef KORA_MBEDTLS_THREADING_ALT_H
#define KORA_MBEDTLS_THREADING_ALT_H


// winsock2 must precede windows.h
#include <winsock2.h>
#include <windows.h>

typedef struct mbedtls_threading_mutex_t {
  CRITICAL_SECTION cs;
  int initialized;
} mbedtls_threading_mutex_t;

#endif
