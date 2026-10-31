#ifndef DREAMCORE_H
#define DREAMCORE_H

#include <stddef.h>

#define DC_SYS_WRITE 1
#define DC_SYS_READ 2
#define DC_SYS_EXIT 4
#define DC_SYS_CLEAR 6
#define DC_SYS_READ_FILE 10
#define DC_SYS_WRITE_FILE 15
#define DC_SYS_ABI_VERSION 16
#define DC_SYS_OPEN 17
#define DC_SYS_READ_FD 18
#define DC_SYS_WRITE_FD 19
#define DC_SYS_CLOSE 20
#define DC_SYS_STAT 24
#define DC_SYS_SBRK 36
#define DC_SYS_LSEEK 37
#define DC_SYS_FSTAT 38
#define DC_SYS_GETTIMEOFDAY 39
#define DC_SYS_GETIDENTITY 34

#define DC_OPEN_READ 1
#define DC_OPEN_WRITE 2
#define DC_OPEN_CREATE 4
#define DC_OPEN_TRUNCATE 8
#define DC_OPEN_APPEND 16

static inline long dc_syscall0(long number)
{
    long result = number;
    __asm__ volatile("int $0x80" : "+a"(result) : : "memory", "rcx", "r11");
    return result;
}

static inline long dc_syscall1(long number, long first)
{
    long result = number;
    __asm__ volatile("int $0x80" : "+a"(result) : "D"(first) : "memory", "rcx", "r11");
    return result;
}

static inline long dc_syscall2(long number, long first, long second)
{
    long result = number;
    __asm__ volatile("int $0x80" : "+a"(result) : "D"(first), "S"(second) : "memory", "rcx", "r11");
    return result;
}

static inline long dc_syscall3(long number, long first, long second, long third)
{
    long result = number;
    __asm__ volatile("int $0x80" : "+a"(result) : "D"(first), "S"(second), "d"(third) : "memory", "rcx", "r11");
    return result;
}

static inline long dc_syscall4(long number, long first, long second, long third, long fourth)
{
    long result = number;
    register long fourth_register __asm__("r10") = fourth;
    __asm__ volatile("int $0x80" : "+a"(result) : "D"(first), "S"(second), "d"(third), "r"(fourth_register) : "memory", "rcx", "r11");
    return result;
}

static inline long dc_write(const void *bytes, unsigned long length)
{
    const unsigned char *cursor = bytes;
    unsigned long written = 0;
    while (written < length) {
        unsigned long chunk = length - written;
        if (chunk > 512) chunk = 512;
        long result = dc_syscall2(DC_SYS_WRITE, (long)(cursor + written), (long)chunk);
        if (result <= 0) return result < 0 ? result : (long)written;
        written += (unsigned long)result;
    }
    return (long)written;
}

static inline long dc_read_character(void)
{
    return dc_syscall0(DC_SYS_READ);
}

static inline long dc_open(const char *path, unsigned long length, int flags, unsigned long mode)
{
    return dc_syscall4(DC_SYS_OPEN, (long)path, (long)length, flags, (long)mode);
}

static inline long dc_read_fd(int descriptor, void *buffer, unsigned long count)
{
    return dc_syscall3(DC_SYS_READ_FD, descriptor, (long)buffer, (long)count);
}

static inline long dc_write_fd(int descriptor, const void *buffer, unsigned long count)
{
    return dc_syscall3(DC_SYS_WRITE_FD, descriptor, (long)buffer, (long)count);
}

static inline long dc_close(int descriptor)
{
    return dc_syscall1(DC_SYS_CLOSE, descriptor);
}

static inline long dc_lseek(int descriptor, long offset, int whence)
{
    return dc_syscall3(DC_SYS_LSEEK, descriptor, offset, whence);
}

static inline long dc_sbrk(long increment)
{
    return dc_syscall1(DC_SYS_SBRK, increment);
}

int dc_resolve_path(const char *path, char *output, size_t capacity);

static inline long dc_clear(void)
{
    return dc_syscall0(DC_SYS_CLEAR);
}

static inline unsigned long dc_strlen(const char *text)
{
    unsigned long length = 0;
    while (text[length]) length++;
    return length;
}

static inline long dc_read_file(const char *path, char *buffer, unsigned long capacity)
{
    return dc_syscall4(DC_SYS_READ_FILE, (long)path, (long)dc_strlen(path), (long)buffer, (long)capacity);
}

static inline long dc_write_file(const char *path, const char *buffer, unsigned long length)
{
    return dc_syscall4(DC_SYS_WRITE_FILE, (long)path, (long)dc_strlen(path), (long)buffer, (long)length);
}

static inline __attribute__((noreturn)) void dc_exit(int status)
{
    dc_syscall1(DC_SYS_EXIT, status);
    for (;;) __asm__ volatile("pause");
}

#endif