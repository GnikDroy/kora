#ifndef KORA_ARRAY_H
#define KORA_ARRAY_H

#include <stddef.h>
#include <stdint.h>
#include <string.h>

extern void *GC_malloc(size_t);
_Noreturn void __kora_panic(const char *message);

typedef struct {
  int64_t len;
  int64_t cap;
  void *buf;
} KoraArray;

static char *slot(const KoraArray *a, int64_t i, int64_t elem_size) {
  return (char *)a->buf + i * elem_size;
}

static void zero_sentinel(KoraArray *a, int64_t elem_size) {
  memset(slot(a, a->len, elem_size), 0, (size_t)elem_size);
}

static void ensure_capacity(KoraArray *a, int64_t len, int64_t elem_size) {
  if (len + 1 <= a->cap) {
    return;
  }
  int64_t cap = a->cap * 2;
  if (cap < 4) {
    cap = 4;
  }
  if (cap < len + 1) {
    cap = len + 1;
  }
  void *buf = GC_malloc((size_t)(cap * elem_size));
  memcpy(buf, a->buf, (size_t)(a->len * elem_size));
  a->buf = buf;
  a->cap = cap;
}

KoraArray *__kora_array_new(int64_t len, int64_t cap, int64_t elem_size) {
  if (len < 0) {
    __kora_panic("negative array length");
  }
  KoraArray *a = (KoraArray *)GC_malloc(sizeof(KoraArray));
  if (cap < len + 1) {
    cap = len + 1;
  }
  a->len = len;
  a->cap = cap;
  a->buf = GC_malloc((size_t)(cap * elem_size));
  return a;
}

KoraArray *__kora_array_lit(const void *data, int64_t len, int64_t elem_size) {
  KoraArray *a = __kora_array_new(len, 0, elem_size);
  memcpy(a->buf, data, (size_t)(len * elem_size));
  return a;
}

KoraArray *__kora_array_from_cstring(const char *s) {
  if (s == NULL) {
    return NULL;
  }
  return __kora_array_lit(s, (int64_t)strlen(s), 1);
}

void __kora_array_push(KoraArray *a, const void *elem, int64_t elem_size) {
  ensure_capacity(a, a->len + 1, elem_size);
  memcpy(slot(a, a->len, elem_size), elem, (size_t)elem_size);
  a->len++;
  zero_sentinel(a, elem_size);
}

void __kora_array_pop(KoraArray *a, void *out, int64_t elem_size) {
  if (a->len == 0) {
    __kora_panic("pop from empty array");
  }
  a->len--;
  memcpy(out, slot(a, a->len, elem_size), (size_t)elem_size);
  zero_sentinel(a, elem_size);
}

void __kora_array_insert(KoraArray *a, int64_t i, const void *elem,
                         int64_t elem_size) {
  if (i < 0 || i > a->len) {
    __kora_panic("index out of bounds");
  }
  ensure_capacity(a, a->len + 1, elem_size);
  memmove(slot(a, i + 1, elem_size), slot(a, i, elem_size),
          (size_t)((a->len - i) * elem_size));
  memcpy(slot(a, i, elem_size), elem, (size_t)elem_size);
  a->len++;
  zero_sentinel(a, elem_size);
}

void __kora_array_remove(KoraArray *a, int64_t i, void *out, int64_t elem_size) {
  if (i < 0 || i >= a->len) {
    __kora_panic("index out of bounds");
  }
  memcpy(out, slot(a, i, elem_size), (size_t)elem_size);
  memmove(slot(a, i, elem_size), slot(a, i + 1, elem_size),
          (size_t)((a->len - i - 1) * elem_size));
  a->len--;
  zero_sentinel(a, elem_size);
}

/* A pure copy; bounds clamp like JS slice. */
KoraArray *__kora_array_slice(KoraArray *a, int64_t start, int64_t end,
                              int64_t elem_size) {
  if (start < 0) {
    start = 0;
  }
  if (end > a->len) {
    end = a->len;
  }
  int64_t len = end - start;
  if (len < 0) {
    len = 0;
  }
  return __kora_array_lit(slot(a, start, elem_size), len, elem_size);
}

/* Mutating append-many. The length snapshot makes a.extend(a) safe: after a
 * growth the source buffer is the (already copied) new one, and the source
 * and destination element ranges never overlap. */
void __kora_array_extend(KoraArray *a, const KoraArray *b, int64_t elem_size) {
  int64_t n = b->len;
  ensure_capacity(a, a->len + n, elem_size);
  memcpy(slot(a, a->len, elem_size), b->buf, (size_t)(n * elem_size));
  a->len += n;
  zero_sentinel(a, elem_size);
}

/* Pure concatenation the `+` operator. */
KoraArray *__kora_array_concat(const KoraArray *a, const KoraArray *b,
                               int64_t elem_size) {
  KoraArray *r = __kora_array_new(a->len + b->len, 0, elem_size);
  memcpy(r->buf, a->buf, (size_t)(a->len * elem_size));
  memcpy(slot(r, a->len, elem_size), b->buf, (size_t)(b->len * elem_size));
  return r;
}

KoraArray *__kora_array_copy(const KoraArray *a, int64_t elem_size) {
  return __kora_array_lit(a->buf, a->len, elem_size);
}

#endif
