#ifndef DREAMCORE_STDLIB_H
#define DREAMCORE_STDLIB_H

#include <stddef.h>
#include <stdarg.h>
#include <setjmp.h>

#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1

extern char **environ;

_Noreturn void abort(void);
_Noreturn void exit(int status);
void *malloc(size_t size);
void *calloc(size_t count, size_t size);
void *realloc(void *pointer, size_t size);
void free(void *pointer);
int abs(int value);
int atoi(const char *text);
unsigned long strtoul(const char *text, char **endptr, int base);
unsigned long long strtoull(const char *text, char **endptr, int base);
long strtol(const char *text, char **endptr, int base);
long long strtoll(const char *text, char **endptr, int base);
double strtod(const char *text, char **endptr);
float strtof(const char *text, char **endptr);
long double strtold(const char *text, char **endptr);
char *getenv(const char *name);
int setenv(const char *name, const char *value, int overwrite);
int unsetenv(const char *name);
int putenv(const char *string);
void *realpath(const char *path, char *resolved_path);
void *bsearch(const void *key, const void *base, size_t count, size_t size,
              int (*compare)(const void *, const void *));
void qsort(void *base, size_t count, size_t size, int (*compare)(const void *, const void *));

#endif