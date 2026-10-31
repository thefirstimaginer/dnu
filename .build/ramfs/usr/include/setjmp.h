#ifndef DREAMCORE_SETJMP_H
#define DREAMCORE_SETJMP_H

typedef struct {
    unsigned long registers[8];
} __jmp_buf_tag;

typedef __jmp_buf_tag jmp_buf[1];

int setjmp(jmp_buf environment);
void longjmp(jmp_buf environment, int value) __attribute__((noreturn));

#endif
