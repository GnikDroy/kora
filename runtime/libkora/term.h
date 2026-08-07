#ifndef KORA_TERM_H
#define KORA_TERM_H

#include <limits.h>
#include <stdint.h>
#include <stdlib.h>

#ifdef _WIN32

#include <io.h>
#include <windows.h>

static DWORD kora_term_saved_in;
static DWORD kora_term_saved_out;
static int kora_term_is_raw = 0;

static void kora_term_restore(void) {
  if (kora_term_is_raw) {
    SetConsoleMode(GetStdHandle(STD_INPUT_HANDLE), kora_term_saved_in);
    SetConsoleMode(GetStdHandle(STD_OUTPUT_HANDLE), kora_term_saved_out);
    kora_term_is_raw = 0;
  }
}

int __kora_term_raw(int enable) {
  if (!enable) {
    kora_term_restore();
    return 0;
  }
  if (kora_term_is_raw) {
    return 0;
  }
  HANDLE hin = GetStdHandle(STD_INPUT_HANDLE);
  HANDLE hout = GetStdHandle(STD_OUTPUT_HANDLE);
  if (!GetConsoleMode(hin, &kora_term_saved_in) ||
      !GetConsoleMode(hout, &kora_term_saved_out)) {
    return -1;
  }
  DWORD in = kora_term_saved_in;
  in &= ~(ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT |
          ENABLE_MOUSE_INPUT);
  in |= ENABLE_VIRTUAL_TERMINAL_INPUT;
  DWORD out = kora_term_saved_out | ENABLE_PROCESSED_OUTPUT |
              ENABLE_VIRTUAL_TERMINAL_PROCESSING;
  if (!SetConsoleMode(hin, in) || !SetConsoleMode(hout, out)) {
    kora_term_restore();
    return -1;
  }
  static int registered = 0;
  if (!registered) {
    atexit(kora_term_restore);
    registered = 1;
  }
  kora_term_is_raw = 1;
  return 0;
}

static unsigned char kora_key_queue[256];
static int kora_key_head = 0;
static int kora_key_len = 0;

static void kora_key_push(unsigned char c) {
  if (kora_key_len < (int)sizeof(kora_key_queue)) {
    kora_key_queue[(kora_key_head + kora_key_len) % sizeof(kora_key_queue)] = c;
    kora_key_len++;
  }
}

static int kora_key_pop(void) {
  if (kora_key_len == 0) {
    return -1;
  }
  int c = kora_key_queue[kora_key_head];
  kora_key_head = (kora_key_head + 1) % sizeof(kora_key_queue);
  kora_key_len--;
  return c;
}

static void kora_term_drain_console(HANDLE hin) {
  DWORD avail = 0;
  if (!GetNumberOfConsoleInputEvents(hin, &avail)) {
    return;
  }
  while (avail > 0) {
    INPUT_RECORD recs[32];
    DWORD want = avail > 32 ? 32 : avail;
    DWORD got = 0;
    if (!ReadConsoleInputA(hin, recs, want, &got) || got == 0) {
      return;
    }
    for (DWORD i = 0; i < got; i++) {
      if (recs[i].EventType == KEY_EVENT && recs[i].Event.KeyEvent.bKeyDown) {
        char ch = recs[i].Event.KeyEvent.uChar.AsciiChar;
        WORD rep = recs[i].Event.KeyEvent.wRepeatCount;
        if (ch != 0) {
          for (WORD r = 0; r < (rep ? rep : 1); r++) {
            kora_key_push((unsigned char)ch);
          }
        }
      }
    }
    avail -= got;
  }
}

int __kora_term_read_key(int64_t timeout_ms) {
  int c = kora_key_pop();
  if (c >= 0) {
    return c;
  }
  HANDLE hin = GetStdHandle(STD_INPUT_HANDLE);
  uint64_t deadline =
      timeout_ms < 0 ? UINT64_MAX : GetTickCount64() + (uint64_t)timeout_ms;

  if (GetFileType(hin) == FILE_TYPE_CHAR) {
    for (;;) {
      kora_term_drain_console(hin);
      c = kora_key_pop();
      if (c >= 0) {
        return c;
      }
      uint64_t now = GetTickCount64();
      if (now >= deadline) {
        return -1;
      }
      DWORD wait = timeout_ms < 0 ? INFINITE : (DWORD)(deadline - now);
      if (WaitForSingleObject(hin, wait) != WAIT_OBJECT_0) {
        return -1;
      }
    }
  }

  if (GetFileType(hin) == FILE_TYPE_PIPE) {
    for (;;) {
      DWORD avail = 0;
      if (!PeekNamedPipe(hin, NULL, 0, NULL, &avail, NULL)) {
        return -1; // broken pipe: end of input
      }
      if (avail > 0) {
        break;
      }
      if (GetTickCount64() >= deadline) {
        return -1;
      }
      Sleep(4);
    }
  }

  // Pipe with data, or a regular file. Read one byte.
  unsigned char b;
  DWORD rd = 0;
  if (!ReadFile(hin, &b, 1, &rd, NULL) || rd != 1) {
    return -1;
  }
  return (int)b;
}

int __kora_term_cols(void) {
  CONSOLE_SCREEN_BUFFER_INFO info;
  if (!GetConsoleScreenBufferInfo(GetStdHandle(STD_OUTPUT_HANDLE), &info)) {
    return -1;
  }
  return (int)(info.srWindow.Right - info.srWindow.Left + 1);
}

int __kora_term_rows(void) {
  CONSOLE_SCREEN_BUFFER_INFO info;
  if (!GetConsoleScreenBufferInfo(GetStdHandle(STD_OUTPUT_HANDLE), &info)) {
    return -1;
  }
  return (int)(info.srWindow.Bottom - info.srWindow.Top + 1);
}

#else

#include <poll.h>
#include <sys/ioctl.h>
#include <termios.h>
#include <unistd.h>

static struct termios kora_term_saved;
static int kora_term_is_raw = 0;

static void kora_term_restore(void) {
  if (kora_term_is_raw) {
    tcsetattr(0, TCSAFLUSH, &kora_term_saved);
    kora_term_is_raw = 0;
  }
}

int __kora_term_raw(int enable) {
  if (!enable) {
    kora_term_restore();
    return 0;
  }
  if (kora_term_is_raw) {
    return 0;
  }
  if (!isatty(0) || tcgetattr(0, &kora_term_saved) != 0) {
    return -1;
  }
  struct termios raw = kora_term_saved;
  raw.c_iflag &= ~(BRKINT | ICRNL | INPCK | ISTRIP | IXON);
  raw.c_lflag &= ~(ECHO | ICANON | IEXTEN | ISIG);
  raw.c_cc[VMIN] = 1;
  raw.c_cc[VTIME] = 0;
  if (tcsetattr(0, TCSAFLUSH, &raw) != 0) {
    return -1;
  }
  static int registered = 0;
  if (!registered) {
    atexit(kora_term_restore);
    registered = 1;
  }
  kora_term_is_raw = 1;
  return 0;
}

int __kora_term_read_key(int64_t timeout_ms) {
  struct pollfd pfd = {0, POLLIN, 0};
  int wait;
  if (timeout_ms < 0) {
    wait = -1;
  } else if (timeout_ms > INT_MAX) {
    wait = INT_MAX;
  } else {
    wait = (int)timeout_ms;
  }
  if (poll(&pfd, 1, wait) <= 0) {
    return -1;
  }
  unsigned char c;
  return read(0, &c, 1) == 1 ? (int)c : -1;
}

int __kora_term_cols(void) {
  struct winsize ws;
  if (ioctl(1, TIOCGWINSZ, &ws) != 0 || ws.ws_col == 0) {
    return -1;
  }
  return (int)ws.ws_col;
}

int __kora_term_rows(void) {
  struct winsize ws;
  if (ioctl(1, TIOCGWINSZ, &ws) != 0 || ws.ws_row == 0) {
    return -1;
  }
  return (int)ws.ws_row;
}

#endif

#endif
