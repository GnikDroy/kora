#ifndef KORA_NET_H
#define KORA_NET_H

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

extern void *GC_malloc(size_t);

#ifdef _WIN32

#include <winsock2.h>
#include <ws2tcpip.h>
#pragma comment(lib, "ws2_32.lib")

typedef SOCKET kora_sock;
#define KORA_BADSOCK INVALID_SOCKET

static void kora_closesock(kora_sock s) { closesocket(s); }

#else

#include <arpa/inet.h>
#include <fcntl.h>
#include <netdb.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <signal.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <sys/types.h>
#include <unistd.h>

typedef int kora_sock;
#define KORA_BADSOCK (-1)

static void kora_closesock(kora_sock s) { close(s); }

#endif

typedef struct {
  struct sockaddr_storage ss;
  socklen_t len;
} kora_addr;

static int64_t kora_net_wrap(kora_sock s) {
  return s == KORA_BADSOCK ? -1 : (int64_t)s;
}

static kora_addr *kora_addr_alloc(void) {
  kora_addr *a = (kora_addr *)GC_malloc(sizeof(kora_addr));
  if (a != NULL) {
    memset(a, 0, sizeof(*a));
    a->len = sizeof(a->ss);
  }
  return a;
}

static int kora_domain(int domain) { return domain == 6 ? AF_INET6 : AF_INET; }

static int kora_socktype(int type) {
  return type == 2 ? SOCK_DGRAM : SOCK_STREAM;
}

static int kora_setopt(kora_sock s, int option, int value) {
  int level;
  int name;
  switch (option) {
  case 0:
    level = SOL_SOCKET;
    name = SO_REUSEADDR;
    break;
  case 1:
    level = SOL_SOCKET;
    name = SO_KEEPALIVE;
    break;
  case 2:
    level = SOL_SOCKET;
    name = SO_BROADCAST;
    break;
  case 3:
    level = IPPROTO_TCP;
    name = TCP_NODELAY;
    break;
  default:
    return -1;
  }
  return setsockopt(s, level, name, (const char *)&value, sizeof(value)) == 0
             ? 0
             : -1;
}

int __kora_net_init(void) {
#ifdef _WIN32
  WSADATA wsa;
  return WSAStartup(MAKEWORD(2, 2), &wsa) == 0 ? 0 : -1;
#else
  signal(SIGPIPE, SIG_IGN);
  return 0;
#endif
}

void __kora_net_cleanup(void) {
#ifdef _WIN32
  WSACleanup();
#endif
}

void *__kora_net_addr_new(void) { return kora_addr_alloc(); }

void *__kora_net_resolve(const char *host, int port, int socktype,
                         int passive) {
  char service[16];
  snprintf(service, sizeof(service), "%d", port);

  struct addrinfo hints;
  memset(&hints, 0, sizeof(hints));
  hints.ai_family = AF_UNSPEC;
  hints.ai_socktype = kora_socktype(socktype);
  if (passive) {
    hints.ai_flags = AI_PASSIVE;
  }

  const char *node = host != NULL && host[0] != '\0' ? host : NULL;
  struct addrinfo *res;
  if (getaddrinfo(node, service, &hints, &res) != 0) {
    return NULL;
  }

  kora_addr *a = kora_addr_alloc();
  if (a != NULL) {
    memcpy(&a->ss, res->ai_addr, res->ai_addrlen);
    a->len = (socklen_t)res->ai_addrlen;
  }
  freeaddrinfo(res);
  return a;
}

int __kora_net_addr_host(void *addr, char *out, int cap) {
  kora_addr *a = (kora_addr *)addr;
  return getnameinfo((struct sockaddr *)&a->ss, a->len, out, (socklen_t)cap,
                     NULL, 0, NI_NUMERICHOST) == 0
             ? 0
             : -1;
}

int __kora_net_addr_port(void *addr) {
  kora_addr *a = (kora_addr *)addr;
  if (a->ss.ss_family == AF_INET) {
    return ntohs(((struct sockaddr_in *)&a->ss)->sin_port);
  }
  if (a->ss.ss_family == AF_INET6) {
    return ntohs(((struct sockaddr_in6 *)&a->ss)->sin6_port);
  }
  return -1;
}

int __kora_net_addr_family(void *addr) {
  kora_addr *a = (kora_addr *)addr;
  return a->ss.ss_family == AF_INET6 ? 6 : 4;
}

int64_t __kora_net_socket(int domain, int type) {
  return kora_net_wrap(socket(kora_domain(domain), kora_socktype(type), 0));
}

int __kora_net_connect(int64_t sock, void *addr) {
  kora_addr *a = (kora_addr *)addr;
  return connect((kora_sock)sock, (struct sockaddr *)&a->ss, a->len) == 0 ? 0
                                                                          : -1;
}

int __kora_net_bind(int64_t sock, void *addr) {
  kora_addr *a = (kora_addr *)addr;
  return bind((kora_sock)sock, (struct sockaddr *)&a->ss, a->len) == 0 ? 0 : -1;
}

int __kora_net_listen(int64_t sock, int backlog) {
  return listen((kora_sock)sock, backlog) == 0 ? 0 : -1;
}

int64_t __kora_net_accept(int64_t sock) {
  return kora_net_wrap(accept((kora_sock)sock, NULL, NULL));
}

int64_t __kora_net_send(int64_t sock, const char *buf, int64_t len) {
#ifdef _WIN32
  int n = send((kora_sock)sock, buf, (int)len, 0);
#else
  ssize_t n = send((kora_sock)sock, buf, (size_t)len, 0);
#endif
  return n < 0 ? -1 : (int64_t)n;
}

int64_t __kora_net_recv(int64_t sock, char *buf, int64_t len) {
#ifdef _WIN32
  int n = recv((kora_sock)sock, buf, (int)len, 0);
#else
  ssize_t n = recv((kora_sock)sock, buf, (size_t)len, 0);
#endif
  return n < 0 ? -1 : (int64_t)n;
}

int64_t __kora_net_sendto(int64_t sock, const char *buf, int64_t len,
                            void *addr) {
  kora_addr *a = (kora_addr *)addr;
#ifdef _WIN32
  int n = sendto((kora_sock)sock, buf, (int)len, 0, (struct sockaddr *)&a->ss,
                 a->len);
#else
  ssize_t n = sendto((kora_sock)sock, buf, (size_t)len, 0,
                     (struct sockaddr *)&a->ss, a->len);
#endif
  return n < 0 ? -1 : (int64_t)n;
}

int64_t __kora_net_recvfrom(int64_t sock, char *buf, int64_t len,
                              void *addr) {
  kora_addr *a = (kora_addr *)addr;
  a->len = sizeof(a->ss);
#ifdef _WIN32
  int n = recvfrom((kora_sock)sock, buf, (int)len, 0, (struct sockaddr *)&a->ss,
                   &a->len);
#else
  ssize_t n = recvfrom((kora_sock)sock, buf, (size_t)len, 0,
                       (struct sockaddr *)&a->ss, &a->len);
#endif
  return n < 0 ? -1 : (int64_t)n;
}

int __kora_net_local(int64_t sock, void *addr) {
  kora_addr *a = (kora_addr *)addr;
  a->len = sizeof(a->ss);
  return getsockname((kora_sock)sock, (struct sockaddr *)&a->ss, &a->len) == 0
             ? 0
             : -1;
}

int __kora_net_peer(int64_t sock, void *addr) {
  kora_addr *a = (kora_addr *)addr;
  a->len = sizeof(a->ss);
  return getpeername((kora_sock)sock, (struct sockaddr *)&a->ss, &a->len) == 0
             ? 0
             : -1;
}

int __kora_net_set_blocking(int64_t sock, int blocking) {
#ifdef _WIN32
  u_long mode = blocking ? 0 : 1;
  return ioctlsocket((kora_sock)sock, FIONBIO, &mode) == 0 ? 0 : -1;
#else
  int flags = fcntl((kora_sock)sock, F_GETFL, 0);
  if (flags < 0) {
    return -1;
  }
  flags = blocking ? flags & ~O_NONBLOCK : flags | O_NONBLOCK;
  return fcntl((kora_sock)sock, F_SETFL, flags) == 0 ? 0 : -1;
#endif
}

int __kora_net_set_option(int64_t sock, int option, int value) {
  return kora_setopt((kora_sock)sock, option, value);
}

int __kora_net_set_timeout(int64_t sock, int ms) {
#ifdef _WIN32
  DWORD tv = (DWORD)(ms < 0 ? 0 : ms);
  int ok = setsockopt((kora_sock)sock, SOL_SOCKET, SO_RCVTIMEO,
                      (const char *)&tv, sizeof(tv)) == 0 &&
           setsockopt((kora_sock)sock, SOL_SOCKET, SO_SNDTIMEO,
                      (const char *)&tv, sizeof(tv)) == 0;
#else
  struct timeval tv;
  tv.tv_sec = ms <= 0 ? 0 : ms / 1000;
  tv.tv_usec = ms <= 0 ? 0 : (ms % 1000) * 1000;
  int ok = setsockopt((kora_sock)sock, SOL_SOCKET, SO_RCVTIMEO,
                      (const char *)&tv, sizeof(tv)) == 0 &&
           setsockopt((kora_sock)sock, SOL_SOCKET, SO_SNDTIMEO,
                      (const char *)&tv, sizeof(tv)) == 0;
#endif
  return ok ? 0 : -1;
}

int __kora_net_shutdown(int64_t sock, int how) {
  return shutdown((kora_sock)sock, how);
}

int __kora_net_close(int64_t sock) {
  kora_closesock((kora_sock)sock);
  return 0;
}

#endif
