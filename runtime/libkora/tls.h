#ifndef KORA_TLS_H
#define KORA_TLS_H

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "mbedtls/ctr_drbg.h"
#include "mbedtls/entropy.h"
#include "mbedtls/error.h"
#include "mbedtls/net_sockets.h"
#include "mbedtls/ssl.h"
#include "mbedtls/threading.h"
#include "mbedtls/x509_crt.h"

#ifdef _WIN32
#include <wincrypt.h>
#include <windows.h>
#else
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <pthread.h>
#endif

#ifdef _WIN32
static void kora_mbedtls_mutex_init(mbedtls_threading_mutex_t *m) {
  InitializeCriticalSection(&m->cs);
  m->initialized = 1;
}

static void kora_mbedtls_mutex_free(mbedtls_threading_mutex_t *m) {
  if (m->initialized) {
    DeleteCriticalSection(&m->cs);
    m->initialized = 0;
  }
}

static int kora_mbedtls_mutex_lock(mbedtls_threading_mutex_t *m) {
  if (!m->initialized) {
    return MBEDTLS_ERR_THREADING_BAD_INPUT_DATA;
  }
  EnterCriticalSection(&m->cs);
  return 0;
}

static int kora_mbedtls_mutex_unlock(mbedtls_threading_mutex_t *m) {
  if (!m->initialized) {
    return MBEDTLS_ERR_THREADING_BAD_INPUT_DATA;
  }
  LeaveCriticalSection(&m->cs);
  return 0;
}

static void kora_tls_init(void) {
  mbedtls_threading_set_alt(kora_mbedtls_mutex_init, kora_mbedtls_mutex_free,
                            kora_mbedtls_mutex_lock, kora_mbedtls_mutex_unlock);
}
#else
static void kora_tls_init(void) {}
#endif

typedef struct {
  mbedtls_ssl_config conf;
  mbedtls_ctr_drbg_context drbg;
  mbedtls_entropy_context entropy;
  mbedtls_x509_crt own_cert;
  mbedtls_pk_context own_key;
  mbedtls_x509_crt custom_ca;
  char **alpn; // NULL-terminated
} KoraTlsConfig;

typedef struct {
  mbedtls_ssl_context ssl;
  int64_t fd;
  KoraTlsConfig *owned;
} KoraTlsSession;


static mbedtls_x509_crt kora_tls_roots;
static int kora_tls_roots_state = 0; /* 0 not tried, 1 loaded, -1 unavailable */

#ifdef _WIN32
static SRWLOCK kora_tls_roots_lock = SRWLOCK_INIT;
static void kora_tls_lock(void) { AcquireSRWLockExclusive(&kora_tls_roots_lock); }
static void kora_tls_unlock(void) { ReleaseSRWLockExclusive(&kora_tls_roots_lock); }
#else
static pthread_mutex_t kora_tls_roots_lock = PTHREAD_MUTEX_INITIALIZER;
static void kora_tls_lock(void) { pthread_mutex_lock(&kora_tls_roots_lock); }
static void kora_tls_unlock(void) { pthread_mutex_unlock(&kora_tls_roots_lock); }
#endif

#ifdef _WIN32
static void kora_tls_load_system_roots(void) {
  HCERTSTORE store = CertOpenSystemStoreA(0, "ROOT");
  if (!store) {
    return;
  }
  PCCERT_CONTEXT ctx = NULL;
  while ((ctx = CertEnumCertificatesInStore(store, ctx)) != NULL) {
    mbedtls_x509_crt_parse_der(&kora_tls_roots, ctx->pbCertEncoded,
                               ctx->cbCertEncoded);
  }
  CertCloseStore(store, 0);
}
#else
static void kora_tls_load_system_roots(void) {
  static const char *paths[] = {
      "/etc/ssl/certs/ca-certificates.crt", /* Debian, Ubuntu, Arch */
      "/etc/pki/tls/certs/ca-bundle.crt",   /* Fedora, RHEL */
      "/etc/ssl/ca-bundle.pem",             /* openSUSE */
      "/etc/ssl/cert.pem",                  /* macOS, Alpine, BSDs */
  };
  for (size_t i = 0; i < sizeof(paths) / sizeof(paths[0]); i++) {
    if (mbedtls_x509_crt_parse_file(&kora_tls_roots, paths[i]) >= 0 &&
        kora_tls_roots.version != 0) {
      return;
    }
  }
}
#endif

static mbedtls_x509_crt *kora_tls_get_roots(void) {
  kora_tls_lock();
  if (kora_tls_roots_state == 0) {
    mbedtls_x509_crt_init(&kora_tls_roots);
    const char *bundle = getenv("KORA_CA_BUNDLE");
    if (bundle != NULL) {
      mbedtls_x509_crt_parse_file(&kora_tls_roots, bundle);
    }
    if (kora_tls_roots.version == 0) {
      kora_tls_load_system_roots();
    }
    kora_tls_roots_state = kora_tls_roots.version != 0 ? 1 : -1;
  }
  int state = kora_tls_roots_state;
  kora_tls_unlock();
  return state == 1 ? &kora_tls_roots : NULL;
}

#ifdef _WIN32
static int kora_tls_would_block(int64_t fd) {
  (void)fd;
  return WSAGetLastError() == WSAEWOULDBLOCK;
}

static void kora_tls_wait(int64_t fd, int want_write) {
  WSAPOLLFD p;
  p.fd = (SOCKET)fd;
  p.events = want_write ? POLLOUT : POLLIN;
  p.revents = 0;
  WSAPoll(&p, 1, -1);
}
#else
static int kora_tls_would_block(int64_t fd) {
  if (errno == EINTR) {
    return 1;
  }
  if (errno != EAGAIN && errno != EWOULDBLOCK) {
    return 0;
  }
  /* EAGAIN from a blocking socket is a receive timeout, not would-block. */
  int fl = fcntl((int)fd, F_GETFL, 0);
  return fl >= 0 && (fl & O_NONBLOCK);
}

static void kora_tls_wait(int64_t fd, int want_write) {
  struct pollfd p;
  p.fd = (int)fd;
  p.events = want_write ? POLLOUT : POLLIN;
  p.revents = 0;
  while (poll(&p, 1, -1) < 0 && errno == EINTR) {
  }
}
#endif

static int kora_tls_bio_send(void *ctx, const unsigned char *buf, size_t len) {
  KoraTlsSession *s = (KoraTlsSession *)ctx;
  int64_t n = __kora_net_send(s->fd, (const char *)buf, (int64_t)len);
  if (n < 0) {
    return kora_tls_would_block(s->fd) ? MBEDTLS_ERR_SSL_WANT_WRITE
                                       : MBEDTLS_ERR_NET_SEND_FAILED;
  }
  return (int)n;
}

static int kora_tls_bio_recv(void *ctx, unsigned char *buf, size_t len) {
  KoraTlsSession *s = (KoraTlsSession *)ctx;
  int64_t n = __kora_net_recv(s->fd, (char *)buf, (int64_t)len);
  if (n < 0) {
    return kora_tls_would_block(s->fd) ? MBEDTLS_ERR_SSL_WANT_READ
                                       : MBEDTLS_ERR_NET_RECV_FAILED;
  }
  return (int)n;
}

static void kora_tls_trace(const char *stage, int ret) {
  if (getenv("KORA_TLS_DEBUG") == NULL) {
    return;
  }
  char msg[128];
  mbedtls_strerror(ret, msg, sizeof(msg));
  fprintf(stderr, "kora tls: %s failed: -0x%04x %s\n", stage, (unsigned)-ret,
          msg);
}


static void kora_tls_config_free(KoraTlsConfig *c) {
  if (c->alpn != NULL) {
    for (char **p = c->alpn; *p != NULL; p++) {
      free(*p);
    }
    free(c->alpn);
  }
  mbedtls_x509_crt_free(&c->custom_ca);
  mbedtls_x509_crt_free(&c->own_cert);
  mbedtls_pk_free(&c->own_key);
  mbedtls_ssl_config_free(&c->conf);
  mbedtls_ctr_drbg_free(&c->drbg);
  mbedtls_entropy_free(&c->entropy);
  free(c);
}

void *__kora_tls_config_new(int is_server) {
  KoraTlsConfig *c = (KoraTlsConfig *)calloc(1, sizeof(KoraTlsConfig));
  if (c == NULL) {
    return NULL;
  }
  mbedtls_ssl_config_init(&c->conf);
  mbedtls_ctr_drbg_init(&c->drbg);
  mbedtls_entropy_init(&c->entropy);
  mbedtls_x509_crt_init(&c->own_cert);
  mbedtls_pk_init(&c->own_key);
  mbedtls_x509_crt_init(&c->custom_ca);

  static const char pers[] = "kora-tls";
  int ret = mbedtls_ctr_drbg_seed(&c->drbg, mbedtls_entropy_func, &c->entropy,
                                  (const unsigned char *)pers, sizeof(pers) - 1);
  if (ret == 0) {
    ret = mbedtls_ssl_config_defaults(
        &c->conf, is_server ? MBEDTLS_SSL_IS_SERVER : MBEDTLS_SSL_IS_CLIENT,
        MBEDTLS_SSL_TRANSPORT_STREAM, MBEDTLS_SSL_PRESET_DEFAULT);
  }
  if (ret != 0) {
    kora_tls_trace("config", ret);
    kora_tls_config_free(c);
    return NULL;
  }
  mbedtls_ssl_conf_rng(&c->conf, mbedtls_ctr_drbg_random, &c->drbg);

  if (!is_server) {
    mbedtls_ssl_conf_authmode(&c->conf, MBEDTLS_SSL_VERIFY_REQUIRED);
    mbedtls_x509_crt *roots = kora_tls_get_roots();
    if (roots != NULL) {
      mbedtls_ssl_conf_ca_chain(&c->conf, roots, NULL);
    }
  }
  return c;
}

int __kora_tls_config_own_cert(void *cfg, const char *cert_pem,
                               const char *key_pem) {
  KoraTlsConfig *c = (KoraTlsConfig *)cfg;
  int ret = mbedtls_x509_crt_parse(&c->own_cert,
                                   (const unsigned char *)cert_pem,
                                   strlen(cert_pem) + 1);
  if (ret != 0) {
    kora_tls_trace("own cert", ret);
    return -1;
  }
  ret = mbedtls_pk_parse_key(&c->own_key, (const unsigned char *)key_pem,
                             strlen(key_pem) + 1, NULL, 0,
                             mbedtls_ctr_drbg_random, &c->drbg);
  if (ret != 0) {
    kora_tls_trace("own key", ret);
    return -1;
  }
  ret = mbedtls_ssl_conf_own_cert(&c->conf, &c->own_cert, &c->own_key);
  if (ret != 0) {
    kora_tls_trace("own cert conf", ret);
    return -1;
  }
  return 0;
}

int __kora_tls_config_ca(void *cfg, const char *ca_pem) {
  KoraTlsConfig *c = (KoraTlsConfig *)cfg;
  int ret = mbedtls_x509_crt_parse(&c->custom_ca, (const unsigned char *)ca_pem,
                                   strlen(ca_pem) + 1);
  if (ret != 0 || c->custom_ca.version == 0) {
    kora_tls_trace("ca", ret);
    return -1;
  }
  mbedtls_ssl_conf_ca_chain(&c->conf, &c->custom_ca, NULL);
  return 0;
}

int __kora_tls_config_alpn(void *cfg, const char *protos) {
  KoraTlsConfig *c = (KoraTlsConfig *)cfg;
  if (c->alpn != NULL) {
    return -1; /* set once */
  }
  size_t count = 1;
  for (const char *p = protos; *p != '\0'; p++) {
    if (*p == ',') {
      count++;
    }
  }
  char **list = (char **)calloc(count + 1, sizeof(char *));
  if (list == NULL) {
    return -1;
  }
  size_t i = 0;
  const char *start = protos;
  for (const char *p = protos;; p++) {
    if (*p == ',' || *p == '\0') {
      size_t len = (size_t)(p - start);
      if (len == 0) {
        goto fail;
      }
      list[i] = (char *)malloc(len + 1);
      if (list[i] == NULL) {
        goto fail;
      }
      memcpy(list[i], start, len);
      list[i][len] = '\0';
      i++;
      start = p + 1;
      if (*p == '\0') {
        break;
      }
    }
  }
  if (mbedtls_ssl_conf_alpn_protocols(&c->conf, (const char **)list) != 0) {
    goto fail;
  }
  c->alpn = list;
  return 0;

fail:
  for (size_t j = 0; list[j] != NULL; j++) {
    free(list[j]);
  }
  free(list);
  return -1;
}

int __kora_tls_config_verify(void *cfg, int mode) {
  KoraTlsConfig *c = (KoraTlsConfig *)cfg;
  mbedtls_ssl_conf_authmode(&c->conf, mode ? MBEDTLS_SSL_VERIFY_REQUIRED
                                           : MBEDTLS_SSL_VERIFY_NONE);
  return 0;
}

static void kora_tls_session_free(KoraTlsSession *s) {
  mbedtls_ssl_free(&s->ssl);
  if (s->owned != NULL) {
    kora_tls_config_free(s->owned);
  }
  free(s);
}

static KoraTlsSession *kora_tls_do_handshake(KoraTlsConfig *c, int64_t fd,
                                             const char *host) {
  KoraTlsSession *s = (KoraTlsSession *)calloc(1, sizeof(KoraTlsSession));
  if (s == NULL) {
    return NULL;
  }
  s->fd = fd;
  mbedtls_ssl_init(&s->ssl);

  int ret = mbedtls_ssl_setup(&s->ssl, &c->conf);
  if (ret == 0 && host != NULL && host[0] != '\0') {
    ret = mbedtls_ssl_set_hostname(&s->ssl, host);
  }
  if (ret != 0) {
    kora_tls_trace("setup", ret);
    kora_tls_session_free(s);
    return NULL;
  }
  mbedtls_ssl_set_bio(&s->ssl, s, kora_tls_bio_send, kora_tls_bio_recv, NULL);

  while ((ret = mbedtls_ssl_handshake(&s->ssl)) != 0) {
    if (ret == MBEDTLS_ERR_SSL_WANT_READ) {
      kora_tls_wait(fd, 0);
    } else if (ret == MBEDTLS_ERR_SSL_WANT_WRITE) {
      kora_tls_wait(fd, 1);
    } else {
      kora_tls_trace("handshake", ret);
      kora_tls_session_free(s);
      return NULL;
    }
  }
  return s;
}

void *__kora_tls_handshake(void *cfg, int64_t fd, const char *host) {
  return kora_tls_do_handshake((KoraTlsConfig *)cfg, fd, host);
}

void *__kora_tls_connect(int64_t fd, const char *host, int insecure) {
  KoraTlsConfig *c = (KoraTlsConfig *)__kora_tls_config_new(0);
  if (c == NULL) {
    return NULL;
  }
  if (insecure) {
    __kora_tls_config_verify(c, 0);
  }
  KoraTlsSession *s = kora_tls_do_handshake(c, fd, host);
  if (s == NULL) {
    kora_tls_config_free(c);
    return NULL;
  }
  s->owned = c;
  return s;
}

int64_t __kora_tls_send(void *handle, const char *buf, int64_t len) {
  KoraTlsSession *s = (KoraTlsSession *)handle;
  for (;;) {
    int ret = mbedtls_ssl_write(&s->ssl, (const unsigned char *)buf,
                                (size_t)len);
    if (ret >= 0) {
      return (int64_t)ret;
    }
    if (ret == MBEDTLS_ERR_SSL_WANT_READ) {
      kora_tls_wait(s->fd, 0);
    } else if (ret == MBEDTLS_ERR_SSL_WANT_WRITE) {
      kora_tls_wait(s->fd, 1);
    } else {
      kora_tls_trace("write", ret);
      return -1;
    }
  }
}

int64_t __kora_tls_recv(void *handle, char *buf, int64_t len) {
  KoraTlsSession *s = (KoraTlsSession *)handle;
  for (;;) {
    int ret = mbedtls_ssl_read(&s->ssl, (unsigned char *)buf, (size_t)len);
    if (ret >= 0) {
      return (int64_t)ret; // 0 is end of stream
    }
    if (ret == MBEDTLS_ERR_SSL_PEER_CLOSE_NOTIFY) {
      return 0;
    }
    if (ret == MBEDTLS_ERR_SSL_WANT_READ) {
      kora_tls_wait(s->fd, 0);
    } else if (ret == MBEDTLS_ERR_SSL_WANT_WRITE) {
      kora_tls_wait(s->fd, 1);
    } else if (ret == MBEDTLS_ERR_SSL_RECEIVED_NEW_SESSION_TICKET) {
      // TLS 1.3 servers may hand out tickets mid-stream
    } else {
      kora_tls_trace("read", ret);
      return -1;
    }
  }
}

void __kora_tls_close(void *handle) {
  KoraTlsSession *s = (KoraTlsSession *)handle;
  mbedtls_ssl_close_notify(&s->ssl); /* best effort */
  kora_tls_session_free(s);
}


int64_t __kora_tls_verify_result(void *handle) {
  KoraTlsSession *s = (KoraTlsSession *)handle;
  return (int64_t)mbedtls_ssl_get_verify_result(&s->ssl);
}

int64_t __kora_tls_peer_cert_der(void *handle, char *out, int64_t cap) {
  KoraTlsSession *s = (KoraTlsSession *)handle;
  const mbedtls_x509_crt *peer = mbedtls_ssl_get_peer_cert(&s->ssl);
  if (peer == NULL || peer->raw.len == 0) {
    return -1;
  }
  int64_t len = (int64_t)peer->raw.len;
  if (out != NULL && cap > 0) {
    memcpy(out, peer->raw.p, (size_t)(len < cap ? len : cap));
  }
  return len;
}

const char *__kora_tls_version(void *handle) {
  KoraTlsSession *s = (KoraTlsSession *)handle;
  const char *v = mbedtls_ssl_get_version(&s->ssl);
  return v != NULL ? v : "unknown";
}

const char *__kora_tls_alpn(void *handle) {
  KoraTlsSession *s = (KoraTlsSession *)handle;
  return mbedtls_ssl_get_alpn_protocol(&s->ssl);
}

#endif
