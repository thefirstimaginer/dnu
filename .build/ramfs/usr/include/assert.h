#ifndef DREAMCORE_ASSERT_H
#define DREAMCORE_ASSERT_H

#include <stdlib.h>

void __assert_fail(const char *expression, const char *file, unsigned int line,
                   const char *function) __attribute__((noreturn));

#define assert(expression) \
    ((expression) ? (void)0 : __assert_fail(#expression, __FILE__, __LINE__, __func__))

#endif
