#ifndef DREAMCORE_UNISTD_H
#define DREAMCORE_UNISTD_H

#include <stddef.h>

#include <sys/types.h>
#include <sys/stat.h>
#include <sys/time.h>

#define STDIN_FILENO 0
#define STDOUT_FILENO 1
#define STDERR_FILENO 2

#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2

#define F_OK 0
#define X_OK 1
#define W_OK 2
#define R_OK 4

extern int errno;

ssize_t read(int descriptor, void *buffer, size_t count);
ssize_t write(int descriptor, const void *buffer, size_t count);
int open(const char *path, int flags, ...);
int close(int descriptor);
off_t lseek(int descriptor, off_t offset, int whence);
void *sbrk(long increment);
int stat(const char *path, struct stat *buffer);
int fstat(int descriptor, struct stat *buffer);
int unlink(const char *path);
int remove(const char *path);
int execvp(const char *file, char *const arguments[]);
char *getcwd(char *buffer, size_t capacity);
int access(const char *path, int mode);
void _exit(int status) __attribute__((noreturn));

#endif