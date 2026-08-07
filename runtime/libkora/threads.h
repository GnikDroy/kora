#ifndef KORA_THREADS_H
#define KORA_THREADS_H

#include <stddef.h>

extern void *GC_malloc(size_t);

#ifdef _WIN32

#include <windows.h>

extern HANDLE WINAPI GC_CreateThread(LPSECURITY_ATTRIBUTES, DWORD,
                                     LPTHREAD_START_ROUTINE, LPVOID, DWORD,
                                     LPDWORD);

#else

#include <pthread.h>
#include <sched.h>
#include <time.h>

extern int GC_pthread_create(pthread_t *, const pthread_attr_t *,
                             void *(*)(void *), void *);
extern int GC_pthread_join(pthread_t, void **);
extern int GC_pthread_detach(pthread_t);

#endif

typedef struct {
  void (*fn)(void *);
  void *arg;
} kora_thread_start;

typedef struct {
#ifdef _WIN32
  HANDLE handle;
#else
  pthread_t handle;
#endif
  void *start;
} kora_thread;

typedef struct {
#ifdef _WIN32
  SRWLOCK lock;
#else
  pthread_mutex_t lock;
#endif
} kora_mutex;

typedef struct {
#ifdef _WIN32
  CONDITION_VARIABLE cond;
#else
  pthread_cond_t cond;
#endif
} kora_cond;

#ifdef _WIN32
static DWORD WINAPI kora_thread_trampoline(LPVOID p) {
  kora_thread_start *s = (kora_thread_start *)p;
  s->fn(s->arg);
  return 0;
}
#else
static void *kora_thread_trampoline(void *p) {
  kora_thread_start *s = (kora_thread_start *)p;
  s->fn(s->arg);
  return NULL;
}
#endif

void *__kora_thread_spawn(void (*entry)(void *), void *arg) {
  kora_thread_start *s =
      (kora_thread_start *)GC_malloc((long)sizeof(kora_thread_start));
  if (s == NULL) {
    return NULL;
  }
  s->fn = entry;
  s->arg = arg;

  kora_thread *t = (kora_thread *)GC_malloc((long)sizeof(kora_thread));
  if (t == NULL) {
    return NULL;
  }
  t->start = s;

#ifdef _WIN32
  t->handle = GC_CreateThread(NULL, 0, kora_thread_trampoline, s, 0, NULL);
  if (t->handle == NULL) {
    return NULL;
  }
#else
  if (GC_pthread_create(&t->handle, NULL, kora_thread_trampoline, s) != 0) {
    return NULL;
  }
#endif
  return t;
}

void __kora_thread_join(void *thread) {
  kora_thread *t = (kora_thread *)thread;
#ifdef _WIN32
  WaitForSingleObject(t->handle, INFINITE);
  CloseHandle(t->handle);
#else
  GC_pthread_join(t->handle, NULL);
#endif
}

void __kora_thread_detach(void *thread) {
  kora_thread *t = (kora_thread *)thread;
#ifdef _WIN32
  CloseHandle(t->handle);
#else
  GC_pthread_detach(t->handle);
#endif
}

void __kora_thread_yield(void) {
#ifdef _WIN32
  SwitchToThread();
#else
  sched_yield();
#endif
}

void *__kora_mutex_new(void) {
  kora_mutex *m = (kora_mutex *)GC_malloc((long)sizeof(kora_mutex));
  if (m == NULL) {
    return NULL;
  }
#ifdef _WIN32
  InitializeSRWLock(&m->lock);
#else
  if (pthread_mutex_init(&m->lock, NULL) != 0) {
    return NULL;
  }
#endif
  return m;
}

void __kora_mutex_lock(void *mutex) {
  kora_mutex *m = (kora_mutex *)mutex;
#ifdef _WIN32
  AcquireSRWLockExclusive(&m->lock);
#else
  pthread_mutex_lock(&m->lock);
#endif
}

void __kora_mutex_unlock(void *mutex) {
  kora_mutex *m = (kora_mutex *)mutex;
#ifdef _WIN32
  ReleaseSRWLockExclusive(&m->lock);
#else
  pthread_mutex_unlock(&m->lock);
#endif
}

void *__kora_cond_new(void) {
  kora_cond *c = (kora_cond *)GC_malloc((long)sizeof(kora_cond));
  if (c == NULL) {
    return NULL;
  }
#ifdef _WIN32
  InitializeConditionVariable(&c->cond);
#else
  if (pthread_cond_init(&c->cond, NULL) != 0) {
    return NULL;
  }
#endif
  return c;
}

void __kora_cond_wait(void *cond, void *mutex) {
  kora_cond *c = (kora_cond *)cond;
  kora_mutex *m = (kora_mutex *)mutex;
#ifdef _WIN32
  SleepConditionVariableSRW(&c->cond, &m->lock, INFINITE, 0);
#else
  pthread_cond_wait(&c->cond, &m->lock);
#endif
}

int __kora_cond_wait_timeout(void *cond, void *mutex, int ms) {
  kora_cond *c = (kora_cond *)cond;
  kora_mutex *m = (kora_mutex *)mutex;
#ifdef _WIN32
  DWORD timeout = ms < 0 ? INFINITE : (DWORD)ms;
  return SleepConditionVariableSRW(&c->cond, &m->lock, timeout, 0) ? 0 : -1;
#else
  struct timespec ts;
  clock_gettime(CLOCK_REALTIME, &ts);
  ts.tv_sec += ms / 1000;
  ts.tv_nsec += (long)(ms % 1000) * 1000000L;
  if (ts.tv_nsec >= 1000000000L) {
    ts.tv_sec += 1;
    ts.tv_nsec -= 1000000000L;
  }
  return pthread_cond_timedwait(&c->cond, &m->lock, &ts) == 0 ? 0 : -1;
#endif
}

void __kora_cond_signal(void *cond) {
  kora_cond *c = (kora_cond *)cond;
#ifdef _WIN32
  WakeConditionVariable(&c->cond);
#else
  pthread_cond_signal(&c->cond);
#endif
}

void __kora_cond_broadcast(void *cond) {
  kora_cond *c = (kora_cond *)cond;
#ifdef _WIN32
  WakeAllConditionVariable(&c->cond);
#else
  pthread_cond_broadcast(&c->cond);
#endif
}

#endif
