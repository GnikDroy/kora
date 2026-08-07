#ifndef KORA_PLATFORM_H
#define KORA_PLATFORM_H

#include <stddef.h>
#include <stdint.h>

extern void *GC_malloc(size_t);

#ifdef _WIN32

#include <direct.h>
#include <errno.h>
#include <io.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <windows.h>

void __kora_sleep_ms(int64_t ms) {
  if (ms > 0) {
    Sleep((DWORD)ms);
  }
}

void __kora_write(const char *buf, int64_t n) {
  _write(1, buf, (unsigned int)n);
}

int __kora_system(const char *cmd) {
  int status = system(cmd);
  return status < 0 ? -1 : status;
}

int __kora_pid(void) { return (int)GetCurrentProcessId(); }
int __kora_setenv(const char *n, const char *v) { return _putenv_s(n, v); }
int __kora_unsetenv(const char *n) { return _putenv_s(n, ""); }
int __kora_mkdir(const char *p) { return _mkdir(p); }
int __kora_rmdir(const char *p) { return _rmdir(p); }
int __kora_chdir(const char *p) { return _chdir(p); }
int __kora_exists(const char *p) { return _access(p, 0); }
int __kora_chmod(const char *p, unsigned m) { return _chmod(p, (int)m); }
void *__kora_popen(const char *cmd, const char *mode) {
  return _popen(cmd, mode);
}
int __kora_pclose(void *f) { return _pclose((FILE *)f); }
int __kora_isatty(int fd) { return _isatty(fd); }
int __kora_errno(void) { return errno; }

const char *__kora_getcwd(void) {
  static char buf[4096];
  return _getcwd(buf, sizeof(buf));
}

typedef struct {
  HANDLE handle;
  WIN32_FIND_DATAA data;
  int pending;
} KoraDir;

void *__kora_dir_open(const char *p) {
  size_t n = strlen(p);
  if (n + 3 >= 4096) {
    return NULL;
  }
  char pattern[4096];
  memcpy(pattern, p, n);
  memcpy(pattern + n, "\\*", 3);
  KoraDir *d = (KoraDir *)GC_malloc(sizeof(KoraDir));
  d->handle = FindFirstFileA(pattern, &d->data);
  if (d->handle == INVALID_HANDLE_VALUE) {
    return NULL;
  }
  d->pending = 1;
  return d;
}

const char *__kora_dir_next(void *dp) {
  KoraDir *d = (KoraDir *)dp;
  if (!d->pending && !FindNextFileA(d->handle, &d->data)) {
    return NULL;
  }
  d->pending = 0;
  return d->data.cFileName;
}

void __kora_dir_close(void *dp) { FindClose(((KoraDir *)dp)->handle); }

int64_t __kora_time_ns(void) {
  LARGE_INTEGER freq, count;
  QueryPerformanceFrequency(&freq);
  QueryPerformanceCounter(&count);
  return (int64_t)((count.QuadPart * 1000000000LL) / freq.QuadPart);
}

int64_t __kora_file_size(const char *p) {
  struct _stat64 st;
  return _stat64(p, &st) == 0 ? (int64_t)st.st_size : -1;
}

int __kora_is_dir(const char *p) {
  struct _stat64 st;
  return (_stat64(p, &st) == 0 && (st.st_mode & _S_IFDIR)) ? 1 : 0;
}

int64_t __kora_file_mtime(const char *p) {
  struct _stat64 st;
  return _stat64(p, &st) == 0 ? (int64_t)st.st_mtime : -1;
}

#else

#include <dirent.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

void __kora_sleep_ms(int64_t ms) {
  if (ms <= 0) {
    return;
  }
  struct timespec duration = {(time_t)(ms / 1000), (long)((ms % 1000) * 1000000L)};
  nanosleep(&duration, NULL);
}

void __kora_write(const char *buf, int64_t n) {
  (void)write(1, buf, (size_t)n);
}

int __kora_system(const char *cmd) {
  int status = system(cmd);
  if (status < 0) {
    return -1;
  }
  if (WIFEXITED(status)) {
    return WEXITSTATUS(status);
  }
  if (WIFSIGNALED(status)) {
    return 128 + WTERMSIG(status);
  }
  return -1;
}

int __kora_pid(void) { return (int)getpid(); }
int __kora_setenv(const char *n, const char *v) { return setenv(n, v, 1); }
int __kora_unsetenv(const char *n) { return unsetenv(n); }
int __kora_mkdir(const char *p) { return mkdir(p, 0777); }
int __kora_rmdir(const char *p) { return rmdir(p); }
int __kora_chdir(const char *p) { return chdir(p); }
int __kora_exists(const char *p) { return access(p, F_OK); }
int __kora_chmod(const char *p, unsigned m) { return chmod(p, (mode_t)m); }
void *__kora_popen(const char *cmd, const char *mode) {
  return popen(cmd, mode);
}
int __kora_pclose(void *f) { return pclose((FILE *)f); }
int __kora_isatty(int fd) { return isatty(fd); }
int __kora_errno(void) { return errno; }

const char *__kora_getcwd(void) {
  static char buf[4096];
  return getcwd(buf, sizeof(buf));
}

void *__kora_dir_open(const char *p) { return opendir(p); }

const char *__kora_dir_next(void *d) {
  struct dirent *e = readdir((DIR *)d);
  return e ? e->d_name : NULL;
}

void __kora_dir_close(void *d) { closedir((DIR *)d); }

int64_t __kora_time_ns(void) {
  struct timespec ts;
  clock_gettime(CLOCK_MONOTONIC, &ts);
  return (int64_t)ts.tv_sec * 1000000000LL + (int64_t)ts.tv_nsec;
}

int64_t __kora_file_size(const char *p) {
  struct stat st;
  return stat(p, &st) == 0 ? (int64_t)st.st_size : -1;
}

int __kora_is_dir(const char *p) {
  struct stat st;
  return (stat(p, &st) == 0 && S_ISDIR(st.st_mode)) ? 1 : 0;
}

int64_t __kora_file_mtime(const char *p) {
  struct stat st;
  return stat(p, &st) == 0 ? (int64_t)st.st_mtime : -1;
}

#endif

#endif
